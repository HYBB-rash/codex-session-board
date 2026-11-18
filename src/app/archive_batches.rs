use super::{App, remote::remote_identity};
use crate::{
    archive::{self, ArchiveEffect, NewInput},
    catalog::account_fingerprint,
    rpc::{DesktopConnection, Role, Rpc},
    store::archive_batches::{Batch, Item, Target},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{cell::Cell, collections::HashSet, sync::atomic::Ordering, time::Duration};

impl App {
    pub(super) fn archive_route(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "兼容模式不执行本机或远程会话的真实归档");
        match (path, body) {
            ("/api/archive", Some(body)) if body.get("batchId").is_some() => self.submit_archive_item(body),
            ("/api/archive", Some(body)) => archive::request(self, body),
            ("/api/archive-batches", None) => Ok(json!({"batches":self.store.archive_batches()?})),
            (_, Some(body)) if path != "/api/archive" => self.archive_batch_action(path, body),
            _ => anyhow::bail!("接口不存在"),
        }
    }

    fn archive_batch_action(&self, path: &str, body: &Value) -> Result<Value> {
        let id = batch_id(&body["id"])?;
        let batch = match path {
            "/api/archive-batches" => self.start_archive_batch(id, body)?,
            "/api/archive-batches/finish" => self.store.finish_archive_batch(id)?,
            "/api/archive-batches/check" => self.check_archive_batch(id)?,
            _ => anyhow::bail!("未知归档批次操作"),
        };
        Ok(json!({"batch":batch}))
    }

    fn start_archive_batch(&self, id: &str, body: &Value) -> Result<Batch> {
        let targets: Vec<Target> = serde_json::from_value(body["items"].clone())?;
        ensure!(!targets.is_empty(), "归档批次为空");
        let mut ids = HashSet::new();
        for target in &targets {
            host_thread(&target.id)?;
            ensure!(ids.insert(&target.id), "归档批次包含重复会话");
            ensure!(target.title.chars().count() <= 500 && target.version >= 0
                && target.input_version >= 0, "归档清单格式无效");
        }
        self.store.create_archive_batch(id, &targets)
    }

    fn submit_archive_item(&self, body: &Value) -> Result<Value> {
        let id = batch_id(&body["batchId"])?;
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        let item = self.store.archive_item(id, identifier)?;
        if item.status != "awaiting" {
            return Ok(json!({"item":item}));
        }
        let claimed = Cell::new(false);
        let result = self.write_archive_item(id, &item.target, &claimed);
        match result {
            Ok(receipt) if receipt["archived"] == true =>
                self.store.record_archive_result(id, identifier, "pending", "archived", None)?,
            Ok(_) => {}
            Err(error) => {
                let from = if claimed.get() { "pending" } else { "awaiting" };
                let (status, reason) = classify(&error);
                eprintln!("归档批次 {id} 会话 {identifier}: {error:#}");
                if status != "pending" {
                    self.store.record_archive_result(id, identifier, from, status, Some(reason))?;
                }
            }
        }
        Ok(json!({"item":self.store.archive_item(id, identifier)?}))
    }

    fn write_archive_item(&self, batch: &str, target: &Target, claimed: &Cell<bool>) -> Result<Value> {
        let remote = target.id.starts_with("remote:")
            .then(|| self.remote_write_target(&target.id)).transpose()?;
        let (app, thread) = remote.as_ref().map_or((self, target.id.as_str()),
            |(app, thread)| (app.as_ref(), thread.as_str()));
        let mut body = serde_json::to_value(target)?;
        body["id"] = json!(thread);
        let mut writer = app.connection(archive::timeout(app), Role::BatchArchiveWriter)?;
        let source = archive_source(&mut writer, app);
        if let Err(error) = &source {
            eprintln!("归档批次 {batch} 来源暂不可核对: {error:#}");
        }
        let source = source.unwrap_or(Value::Null);
        archive::request_with_writer(app, &body, Some(&mut writer), || {
            let accepted = self.store.claim_archive_item(batch, &target.id, &source)?;
            claimed.set(accepted);
            Ok(accepted)
        })
    }

    fn check_archive_batch(&self, id: &str) -> Result<Batch> {
        let batch = self.store.archive_batch(id)?;
        let mut hosts = std::collections::BTreeMap::<String, Vec<(&Item, String)>>::new();
        for item in batch.items.iter().filter(|item| item.status == "pending") {
            let (host, thread) = host_thread(&item.target.id)?;
            hosts.entry(host).or_default().push((item, thread));
        }
        std::thread::scope(|scope| {
            for (host, items) in hosts {
                scope.spawn(move || {
                    let result = if host == self.config.host_id {
                        check_host(self, self, id, &items)
                    } else {
                        self.remote_app(&host)
                            .and_then(|remote| check_host(self, &remote, id, &items))
                    };
                    if let Err(error) = result {
                        eprintln!("核对归档批次 {id} 主机 {host}: {error:#}");
                    }
                });
            }
        });
        self.store.archive_batch(id)
    }
}

fn host_thread(id: &str) -> Result<(String, String)> {
    if id.starts_with("remote:") {
        return remote_identity(id);
    }
    ensure!(uuid::Uuid::parse_str(id)?.to_string() == id, "无效会话 ID");
    Ok(("local".into(), id.into()))
}

fn batch_id(value: &Value) -> Result<&str> {
    let id = value.as_str().context("缺少归档批次 ID")?;
    ensure!(uuid::Uuid::parse_str(id)?.to_string() == id, "无效归档批次 ID");
    Ok(id)
}

fn archive_source(connection: &mut DesktopConnection, app: &App) -> Result<Value> {
    let mut source = connection.catalog_source()?;
    let response = connection.call("account/read", json!({"refreshToken":false}))?;
    let account = match account_fingerprint(&response)? {
        Some(value) => value,
        None => {
            ensure!(response["requiresOpenaiAuth"] == false && response["account"].is_null()
                && response["userId"].is_null() && response["workspaceRouting"].is_null(),
                "无法确认 Codex 归档账号");
            "no-openai-auth".into()
        }
    };
    Ok(json!({"home":source["codexHome"].take(), "account":account,
        "peer":if app.config.host_id == "local" { Value::Null } else {source["peerFingerprint"].take()}}))
}

fn check_host(root: &App, app: &App, batch: &str, items: &[(&Item, String)]) -> Result<()> {
    let _operation = app.operation()?;
    let timeout = Duration::from_secs(3);
    let deadline = crate::rpc::deadline_after(timeout)?;
    let mut reader = app.connection(timeout, Role::CatalogReader)?.bound_to_deadline(deadline);
    let source = archive_source(&mut reader, app)?;
    let source_json = source.to_string();
    let eligible: HashSet<_> = items.iter()
        .filter(|(item, _)| item.source.as_deref() == Some(source_json.as_str()))
        .map(|(_, thread)| thread.as_str()).collect();
    if eligible.is_empty() {
        return Ok(());
    }
    let mut matched = HashSet::new();
    let scan = archive::archived_ids(&mut reader, eligible, app.config.max_pages, &mut matched);
    if matched.is_empty() {
        return scan;
    }
    let mut identity = app.connection(Duration::from_secs(2), Role::CatalogReader)?;
    ensure!(archive_source(&mut identity, app)? == source, "核对期间 Codex 归档来源已变化");
    for (item, thread) in items.iter().filter(|(_, thread)| matched.contains(thread.as_str())) {
        let target = &item.target;
        let lock = app.thread_lock(thread)?;
        let Ok(_guard) = lock.try_lock() else { continue; };
        if root.store.archive_item(batch, &target.id)?.status != "pending" {
            continue;
        }
        let local = app.store.confirm_archive(thread, (target.version, target.input_version));
        if let Err(error) = &local {
            eprintln!("归档批次 {batch} 本地会话 {thread}: {error:#}");
        }
        app.refresh.store(true, Ordering::Relaxed);
        root.store.record_archive_result(batch, &target.id, "pending", "verified",
            local.err().map(|_| "已核实归档，看板状态仍在同步"))?;
    }
    scan
}

fn classify(error: &anyhow::Error) -> (&'static str, &'static str) {
    if let Some(effect) = error.downcast_ref::<ArchiveEffect>() {
        match effect {
            ArchiveEffect::Unconfirmed => ("pending", "unconfirmed"),
            ArchiveEffect::Confirmed => ("archived", "已归档，看板状态仍在同步"),
        }
    } else if error.is::<NewInput>() {
        ("newInput", "有新输入，本次未提交原生归档")
    } else {
        ("rejected", "本次未提交原生归档。请刷新看板，核对连接和会话状态。")
    }
}
