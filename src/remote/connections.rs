use super::*;
use std::sync::atomic::Ordering;

impl App {
    pub(super) fn requested_hosts(&self) -> Result<Vec<hosts::Host>> {
        let mut hosts = hosts::registered(&self.config)?;
        let running = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?;
        let mut requested = self
            .remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?;
        requested.retain(|id| {
            hosts.iter().any(|host| {
                host.host_id == *id
                    && (host.auto_connect
                        || running
                            .get(id)
                            .is_none_or(|remote| remote.enabled.load(Ordering::SeqCst)))
            })
        });
        for host in &mut hosts {
            host.enabled = host.auto_connect || requested.contains(&host.host_id);
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
        if let Err(error) = self.attach_remote(&host) {
            self.forget_request(identifier)?;
            return Err(error);
        }
        let remote = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .get(identifier)
            .cloned()
            .context("远程实例未创建")?;
        let timeout = Duration::from_secs(self.config.rpc_timeout_seconds);
        if let Err(error) = remote.connection(timeout, crate::rpc::Role::RemoteObserver) {
            self.finish_request(identifier, &remote)?;
            return Err(error);
        }
        self.remote_refresh.store(true, Ordering::Relaxed);
        Ok(json!({"ok":true,"hostId":identifier}))
    }

    fn forget_request(&self, identifier: &str) -> Result<()> {
        self.remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
            .remove(identifier);
        Ok(())
    }

    fn finish_request(&self, identifier: &str, remote: &App) -> Result<()> {
        self.forget_request(identifier)?;
        remote.deactivate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::sync::Arc;

    fn preference(directory: &std::path::Path, enabled: bool) -> Result<App> {
        let state = directory.join("state.json");
        std::fs::write(
            &state,
            json!({
                "codex-managed-remote-connections":[{
                    "hostId":"remote:a","displayName":"A","alias":"example"
                }],
                "remote-connection-auto-connect-by-host-id":{"remote:a":enabled}
            })
            .to_string(),
        )?;
        App::new(Config {
            data_dir: directory.join("root"),
            sidebar_state_file: state,
            sync_titles: false,
            ..Config::default()
        })
    }

    #[test]
    fn failed_manual_connection_stops_when_auto_connect_is_off() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = preference(directory.path(), false)?;
        let remote = Arc::new(App::new(Config {
            data_dir: directory.path().join("child"),
            host_id: "remote:a".into(),
            sync_titles: false,
            remote_retry: false,
            ..Config::default()
        })?);
        remote.deactivate()?;
        root.remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
            .insert("remote:a".into());
        root.remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .insert("remote:a".into(), remote);
        let hosts = root.requested_hosts()?;
        assert!(!hosts[0].enabled);
        assert!(
            root.remote_requested
                .lock()
                .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn explicit_request_waits_until_a_connection_exists() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = preference(directory.path(), false)?;
        root.remote_requested
            .lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
            .insert("remote:a".into());
        let hosts = root.requested_hosts()?;
        assert!(hosts[0].enabled);
        assert!(!hosts[0].auto_connect);
        Ok(())
    }
}
