use super::hosts::{self, Host};
use crate::{app::App, config, observer};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};

impl App {
    fn build_remote(&self, host: &Host) -> Result<Arc<App>> {
        let mut config = self.config.clone();
        config.host_id = host.host_id.clone();
        config.data_dir = host.directory(&self.config);
        config.proxy_command = host.proxy(&self.config)?;
        let remote = Arc::new(App::new(config)?);
        remote.update_health(json!({"hostLabel":host.display_name}))?;
        let path = remote.config.data_dir.join("source.json");
        let source = json!({"hostId":host.host_id,"command":remote.config.proxy_command});
        if path.exists() {
            let previous: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            ensure!(previous == source, "主机连接身份已改变，需要显式迁移");
        } else {
            config::write_json(&path, &source, false)?;
        }
        remote.store.import_remote(&self.store, &host.host_id)?;
        Ok(remote)
    }

    pub(crate) fn refresh_remote_instances(&self) -> Result<()> {
        let hosts = hosts::registered(&self.config)?;
        let mut errors = Vec::new();
        for host in &hosts {
            if let Err(error) = self.attach_remote(host) {
                errors.push(json!({"hostId":host.host_id,"error":error.to_string()}));
            }
        }
        let mut remotes = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?;
        remotes.retain(|identifier, remote| {
            let registered = hosts
                .iter()
                .any(|host| host.host_id == *identifier && host.enabled);
            if !registered {
                remote.enabled.store(false, Ordering::SeqCst);
            }
            registered
        });
        drop(remotes);
        self.update_health(json!({"remoteConnectionErrors":errors}))
    }

    fn attach_remote(&self, host: &Host) -> Result<()> {
        if !host.enabled {
            return Ok(());
        }
        let mut remotes = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?;
        if let Some(remote) = remotes.get(&host.host_id) {
            let matches = remote.config.proxy_command == host.proxy(&self.config)?;
            if !matches {
                remote.enabled.store(false, Ordering::SeqCst);
            }
            ensure!(matches, "主机连接身份已改变，需要显式迁移");
            remote.update_health(json!({"hostLabel":host.display_name}))?;
            return Ok(());
        }
        let remote = self.build_remote(host)?;
        observer::start_host(&remote);
        crate::auto_archive::start(&remote);
        remotes.insert(host.host_id.clone(), remote);
        Ok(())
    }

    pub(crate) fn remote_app(&self, host: &str) -> Result<Arc<App>> {
        let descriptor = hosts::registered(&self.config)?
            .into_iter()
            .find(|entry| entry.host_id == host)
            .context("主机已移除或未登记")?;
        ensure!(descriptor.enabled, "该主机已禁用自动连接，请先在客户端连接");
        let remote = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .get(host)
            .cloned()
            .context("远程主机尚未连接，请刷新后重试")?;
        ensure!(
            remote.config.proxy_command == descriptor.proxy(&self.config)?,
            "主机连接身份已改变，需要显式迁移"
        );
        ensure!(remote.enabled.load(Ordering::SeqCst), "远程主机登记已失效");
        Ok(remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn remote_instances_reuse_store_preserve_notes_and_reject_reassigned_targets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            sync_titles: false,
            ..Config::default()
        })?;
        let identifier = uuid::Uuid::new_v4().to_string();
        app.store.discover_remote(
            "remote:a",
            "测试",
            &[json!({"id":identifier,
            "hostId":"remote:a","title":"旧标题","cwd":"/remote","updatedAt":1})],
        )?;
        app.store.save_remote(
            &json!({"id":crate::store::remote::identity("remote:a",&identifier),
            "state":"review","note":"保留备注","version":0,"inputVersion":0}),
        )?;
        let mut host: Host = serde_json::from_value(json!({"hostId":"remote:a",
            "displayName":"测试","alias":"example","hostname":null,"sshPort":null,"identity":null}))?;
        let remote = app.build_remote(&host)?;
        assert_eq!(remote.store.note(&identifier)?, "保留备注");
        let body =
            json!({"id":identifier,"state":"review","note":"新备注","version":1,"inputVersion":0});
        remote.request("/api/state", Some(&body))?;
        let reopened = app.build_remote(&host)?;
        assert_eq!(reopened.store.note(&identifier)?, "新备注");
        assert_eq!(
            app.store
                .remote_session(&crate::store::remote::identity("remote:a", &identifier))?["note"],
            "保留备注"
        );
        host.hostname = Some("other".into());
        host.alias = None;
        assert!(app.build_remote(&host).is_err());
        Ok(())
    }
}
