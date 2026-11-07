use super::*;
use std::sync::atomic::Ordering;

impl App {
    pub(super) fn requested_hosts(&self) -> Result<Vec<hosts::Host>> {
        let mut hosts = hosts::registered(&self.config)?;
        let mut requested = self
            .remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?;
        requested.retain(|id| hosts.iter().any(|host| &host.host_id == id));
        for host in &mut hosts {
            host.enabled |= requested.contains(&host.host_id);
        }
        Ok(hosts)
    }

    pub(super) fn retire_remote(&self, identifier: &str, remote: &App) -> Result<()> {
        let operation = self.thread_lock(&format!("connect:{identifier}"))?;
        let _guard = operation
            .lock()
            .map_err(|_| anyhow::anyhow!("主机连接锁已损坏"))?;
        if !self
            .remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
            .contains(identifier)
        {
            remote.deactivate()?;
        }
        Ok(())
    }

    pub(crate) fn connect_remote(&self, body: &Value) -> Result<Value> {
        ensure!(
            !self.config.legacy_mode && self.config.host_id == "local",
            "当前模式不支持远程连接"
        );
        let identifier = body["hostId"].as_str().context("缺少主机 ID")?;
        let mut host = hosts::registered(&self.config)?
            .into_iter()
            .find(|host| host.host_id == identifier)
            .context("主机已移除或未登记")?;
        self.remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
            .insert(identifier.into());
        host.enabled = true;
        self.attach_remote(&host)?;
        let remote = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .get(identifier)
            .cloned()
            .context("远程实例未创建")?;
        let timeout = Duration::from_secs(self.config.rpc_timeout_seconds);
        drop(remote.connection(timeout, crate::rpc::Role::RemoteObserver)?);
        self.remote_refresh.store(true, Ordering::Relaxed);
        Ok(json!({"ok":true,"hostId":identifier}))
    }
}
