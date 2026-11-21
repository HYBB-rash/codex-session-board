use crate::{
    app::App,
    archive,
    catalog::account_fingerprint,
    rpc::{DesktopConnection, Role, Rpc},
    store::{Store, archive_batches::{Item, Target}},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{cell::Cell, collections::HashSet, sync::atomic::Ordering, time::Duration};

pub(super) struct HostArchive<'a> {
    pub root_batches: &'a Store,
    pub target_host: &'a App,
}

impl HostArchive<'_> {
    pub(super) fn restore(&self, candidates: &[(String, Target, String)]) -> Result<()> {
        let _operation = self.target_host.operation()?;
        let timeout = Duration::from_secs(2);
        let deadline = crate::rpc::deadline_after(timeout)?;
        let mut reader = self.target_host.connection(timeout, Role::CatalogReader)?.bound_to_deadline(deadline);
        let source = self.source(&mut reader)?;
        let source_json = source.to_string();
        let matching: Vec<_> = candidates.iter().filter(|(_, _, recorded)| recorded == &source_json).collect();
        if matching.is_empty() { return Ok(()) }
        let active = self.listed(&mut reader, false)?;
        if !matching.iter().any(|(thread, _, _)| active.contains(thread)) { return Ok(()) }
        let archived = self.listed(&mut reader, true)?;
        ensure!(self.source(&mut reader)? == source, "核对恢复期间 Codex 来源已变化");
        for (thread, revision, _) in matching {
            if !active.contains(thread) || archived.contains(thread) { continue; }
            let lock = self.target_host.thread_lock(thread)?;
            let Ok(_guard) = lock.try_lock() else { continue; };
            self.target_host.store.restore_failed_archive(thread, revision)?;
        }
        Ok(())
    }

    fn listed(&self, reader: &mut DesktopConnection, archived: bool) -> Result<HashSet<String>> {
        let config = &self.target_host.config;
        let (entries, complete) = crate::rpc::collect_threads(reader, config.page_size, config.max_pages, archived)?;
        ensure!(!archived || complete, "归档列表尚未完整读取");
        entries.iter().map(|entry| {
            let id = entry["id"].as_str().context("列表缺少会话 ID")?;
            ensure!(uuid::Uuid::parse_str(id)?.to_string() == id &&
                entry.get("hostId").is_none_or(|host| host == &config.host_id), "会话列表身份不匹配");
            Ok(id.to_owned())
        }).collect()
    }

    pub(super) fn submit(
        &self, batch: &str, target: &Target, thread: &str, claimed: &Cell<bool>,
    ) -> Result<Value> {
        let mut body = serde_json::to_value(target)?;
        body["id"] = json!(thread);
        let mut writer = self.target_host.connection(
            archive::timeout(self.target_host), Role::BatchArchiveWriter)?;
        let source = self.source(&mut writer).context("暂时无法验证归档来源")?;
        archive::request_with_writer(self.target_host, &body, Some(&mut writer), || {
            let accepted = self.root_batches.claim_archive_item(batch, &target.id, &source)?;
            claimed.set(accepted);
            Ok(accepted)
        })
    }

    pub(super) fn check(&self, batch: &str, items: &[(&Item, String)]) -> Result<()> {
        let _operation = self.target_host.operation()?;
        let timeout = Duration::from_secs(3);
        let deadline = crate::rpc::deadline_after(timeout)?;
        let mut reader = self.target_host.connection(timeout, Role::CatalogReader)?
            .bound_to_deadline(deadline);
        let source = self.source(&mut reader)?;
        let source_json = source.to_string();
        let eligible: HashSet<_> = items.iter()
            .filter(|(item, _)| item.source.as_deref() == Some(source_json.as_str()))
            .map(|(_, thread)| thread.as_str()).collect();
        if eligible.is_empty() {
            return Ok(());
        }
        let mut matched = HashSet::new();
        let scan = archive::archived_ids(&mut reader, eligible,
            self.target_host.config.max_pages, &mut matched);
        if matched.is_empty() {
            return scan;
        }
        let mut identity = self.target_host.connection(Duration::from_secs(2), Role::CatalogReader)?;
        ensure!(self.source(&mut identity)? == source, "核对期间 Codex 归档来源已变化");
        for (item, thread) in items.iter().filter(|(_, thread)| matched.contains(thread.as_str())) {
            let target = &item.target;
            let lock = self.target_host.thread_lock(thread)?;
            let Ok(_guard) = lock.try_lock() else { continue; };
            if self.root_batches.archive_item(batch, &target.id)?.status != "pending" {
                continue;
            }
            let local = self.target_host.store.confirm_archive(thread, (target.version, target.input_version));
            if let Err(error) = &local {
                eprintln!("归档批次 {batch} 主机会话 {thread}: {error:#}");
            }
            self.target_host.refresh.store(true, Ordering::Relaxed);
            self.root_batches.record_archive_result(batch, &target.id, "pending", "verified",
                local.err().map(|_| "已核实归档，看板状态仍在同步"))?;
        }
        scan
    }

    fn source(&self, connection: &mut DesktopConnection) -> Result<Value> {
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
            "peer":if self.target_host.config.host_id == "local" {
                Value::Null
            } else {source["peerFingerprint"].take()}}))
    }
}
