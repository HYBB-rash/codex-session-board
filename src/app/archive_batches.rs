use super::{App, remote::remote_identity};
use crate::{
    archive::{self, ArchiveEffect, NewInput},
    operations::Action,
    store::archive_batches::{Batch, Item, Target},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{cell::Cell, collections::HashSet};

mod host;
use host::HostArchive;

impl App {
    pub(crate) fn reconcile_restored_archives(&self, observed: &[Value]) -> Result<()> {
        let failed = self.mapping_store.failed_archive_marks()?;
        if failed.is_empty() { return Ok(()) }
        let active: HashSet<_> = observed.iter().filter_map(|entry| entry["id"].as_str()).collect();
        let mut candidates = Vec::new();
        for (target, source) in failed {
            let (host, thread) = host_thread(&target.id)?;
            if host == self.config.host_id && active.contains(thread.as_str())
                && self.store.matches_active_revision(&thread, &target)? {
                candidates.push((thread, target, source));
            }
        }
        if candidates.is_empty() { return Ok(()) }
        HostArchive { root_batches: &self.mapping_store, target_host: self }.restore(&candidates)
    }

    pub(super) fn archive_route(&self, action: Action, body: &Value) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "兼容模式不执行本机或远程会话的真实归档");
        let batch = match action {
            Action::Archive if body.get("batchId").is_some() => return self.submit_archive_item(body),
            Action::Archive => return archive::request(self, body),
            Action::ArchiveBatches => return Ok(json!({"batches":self.store.archive_batches()?})),
            Action::ArchiveBatchStart => self.start_archive_batch(batch_id(&body["id"])?, body)?,
            Action::ArchiveBatchFinish => self.store.finish_archive_batch(batch_id(&body["id"])?)?,
            Action::ArchiveBatchCheck => self.check_archive_batch(batch_id(&body["id"])?)?,
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
        let (target_host, thread) = remote.as_ref().map_or((self, target.id.as_str()),
            |(host, thread)| (host.as_ref(), thread.as_str()));
        HostArchive { root_batches: &self.store, target_host }
            .submit(batch, target, thread, claimed)
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
                        HostArchive { root_batches: &self.store, target_host: self }.check(id, &items)
                    } else {
                        self.remote_app(&host)
                            .and_then(|remote| HostArchive {
                                root_batches: &self.store, target_host: &remote,
                            }.check(id, &items))
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
