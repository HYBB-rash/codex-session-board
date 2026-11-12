use super::hosts::{self, Host};
use crate::{app::App, observer};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::sync::{Arc, atomic::Ordering};

impl App {
    fn build_remote(&self, host: &Host) -> Result<Arc<App>> {
        host.verify_source(&self.config)?;
        let mut config = self.config.clone();
        config.host_id = host.host_id.clone();
        config.data_dir = host.directory(&self.config);
        config.auto_archive_seconds = self.config.remote_auto_archive_seconds;
        config.native_sections = false;
        config.remote_retry = host.auto_connect;
        config.proxy_command = host.proxy(&self.config)?;
        config.clock_command = host.command(
            &self.config,
            "python3 -c 'import time; print(time.time(), flush=True)'",
        )?;
        config.remote_binding = Some(hosts::source::Binding::prepare(host, &self.config)?);
        let remote = Arc::new(App::new(config)?);
        remote.update_health(json!({"hostLabel":host.display_name}))?;
        remote.store.import_remote(&self.store, &host.host_id)?;
        Ok(remote)
    }

    pub(crate) fn refresh_remote_instances(&self) -> Result<()> {
        ensure!(!self.config.legacy_mode, "六状态兼容模式不连接远程主机");
        self.load_remote_cache()?;
        let hosts = self.requested_hosts()?;
        let cached = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .clone();
        for (identifier, remote) in cached {
            if !hosts
                .iter()
                .any(|host| host.host_id == identifier && host.enabled)
            {
                self.retire_remote(&identifier, &remote)?;
            }
        }
        let mut errors = Vec::new();
        for host in &hosts {
            if let Err(error) = self.attach_remote(host) {
                errors.push(json!({"hostId":host.host_id,"error":error.to_string()}));
            }
        }
        let registry: Vec<_> = hosts
            .iter()
            .map(|host| {
                json!({"hostId":host.host_id,
            "hostLabel":host.display_name,"enabled":host.enabled})
            })
            .collect();
        self.update_health(json!({"remoteConnectionErrors":errors,"remoteRegistry":registry}))
    }

    pub(super) fn attach_remote(&self, host: &Host) -> Result<()> {
        self.workers.while_open(|| Ok(()))?;
        let operation = self.thread_lock(&format!("connect:{}", host.host_id))?;
        let _guard = operation
            .lock()
            .map_err(|_| anyhow::anyhow!("主机连接锁已损坏"))?;
        let existing = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .get(&host.host_id)
            .cloned();
        if let Some(remote) = existing {
            if let Err(error) = host.verify_source(&self.config) {
                remote.deactivate()?;
                return Err(error);
            }
            let matches = remote.config.proxy_command == host.proxy(&self.config)?;
            remote.update_health(json!({"hostLabel":host.display_name}))?;
            if matches && (remote.enabled.load(Ordering::SeqCst) || !host.enabled) {
                return self.workers.while_open(|| Ok(()));
            }
            remote.shutdown()?;
        }
        let remote = self.build_remote(host)?;
        if !host.enabled {
            remote.deactivate()?;
        }
        self.install_remote(host.host_id.clone(), remote, host.enabled)
    }

    pub(crate) fn install_remote(
        &self,
        identifier: String,
        remote: Arc<App>,
        start: bool,
    ) -> Result<()> {
        let result = self.workers.while_open(|| {
            self.remotes
                .lock()
                .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
                .insert(identifier, Arc::clone(&remote));
            if start {
                observer::start_host(&remote)?;
                crate::auto_archive::start(&remote)?;
            }
            Ok(())
        });
        if let Err(error) = result {
            let mut errors = vec![format!("{error:#}")];
            if let Err(error) = remote.shutdown() {
                errors.push(format!("{error:#}"));
            }
            return crate::workers::finish(errors);
        }
        Ok(())
    }

    pub(crate) fn remote_entry(&self, host: &str) -> Result<(Host, Option<Arc<App>>)> {
        let descriptor = hosts::registered(&self.config)?
            .into_iter()
            .find(|entry| entry.host_id == host)
            .context("主机已移除或未登记")?;
        descriptor.verify_source(&self.config)?;
        let remote = self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .get(host)
            .cloned();
        Ok((descriptor, remote))
    }

    pub(crate) fn remote_app(&self, host: &str) -> Result<Arc<App>> {
        let (descriptor, remote) = self.remote_entry(host)?;
        ensure!(
            descriptor.enabled
                || self
                    .remote_requested
                    .lock()
                    .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
                    .contains(host),
            "该主机未自动连接，请在看板中连接"
        );
        let remote = remote.context("远程主机尚未连接，请刷新后重试")?;
        ensure!(remote.enabled.load(Ordering::SeqCst), "远程主机登记已失效");
        if let Some(binding) = &remote.config.remote_binding {
            crate::rpc::framing::guard(
                binding,
                std::time::Duration::from_secs(self.config.rpc_timeout_seconds),
            )
            .map_err(|error| anyhow::anyhow!("远程主机来源核验失败：{error:#}"))?;
        }
        Ok(remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn isolated(directory: &std::path::Path, name: &str) -> Result<App> {
        App::new(Config {
            data_dir: directory.join(name),
            sync_titles: false,
            ..Config::default()
        })
    }

    #[test]
    fn disabling_a_host_drains_its_old_operations_without_blocking_another_host() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = Arc::new(isolated(directory.path(), "remote")?);
        let local = isolated(directory.path(), "local")?;
        let identifier = uuid::Uuid::new_v4().to_string();
        for instance in [&*app, &local] {
            instance
                .store
                .discover(&[json!({"id":identifier,"name":"same"})])?;
        }
        let operation = app.operation()?;
        let worker = Arc::clone(&app);
        let (sender, received) = std::sync::mpsc::channel();
        let paused = std::thread::spawn(move || sender.send(worker.deactivate()).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while app.enabled.load(Ordering::SeqCst) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(app.operation().is_err());
        assert!(
            received
                .recv_timeout(std::time::Duration::from_millis(20))
                .is_err()
        );
        let body = json!({"id":identifier,"state":"review","note":"local",
            "version":0,"inputVersion":0});
        local.request("/api/state", Some(&body))?;
        assert!(app.request("/api/state", Some(&body)).is_err());
        assert_eq!(app.store.note(&identifier)?, "");
        drop(operation);
        received.recv_timeout(std::time::Duration::from_secs(1))??;
        paused.join().unwrap();
        assert_eq!(local.store.note(&identifier)?, "local");
        Ok(())
    }

    #[test]
    fn replacing_a_deactivated_remote_joins_the_old_workers() -> Result<()> {
        let fixture = crate::rpc::identity_fixture::Fixture::new()?;
        let root = Arc::new(App::new(fixture.config.clone())?);
        let old = root.build_remote(&fixture.host)?;
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed = finished.clone();
        let worker = old.clone();
        let owner = root.clone();
        old.workers.spawn(move || {
            crate::workers::pause(&worker.enabled, std::time::Duration::from_secs(3600), None);
            let _remotes = owner.remotes.lock().unwrap();
            completed.store(true, Ordering::SeqCst);
            Ok(())
        })?;
        root.install_remote(fixture.host.host_id.clone(), old.clone(), false)?;
        old.deactivate()?;
        root.attach_remote(&fixture.host)?;
        let replacement = root.remotes.lock().unwrap()[&fixture.host.host_id].clone();
        assert!(!Arc::ptr_eq(&old, &replacement));
        assert!(finished.load(Ordering::SeqCst));
        assert!(old.workers.spawn(|| Ok(())).is_err());
        root.shutdown()?;
        Ok(())
    }

    #[test]
    fn remote_instances_reuse_store_preserve_notes_and_reject_reassigned_targets() -> Result<()> {
        let fixture = crate::rpc::identity_fixture::Fixture::new()?;
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            sync_titles: false,
            ssh_command: fixture.config.ssh_command.clone(),
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
        fixture.update("hostname", json!("other"))?;
        assert!(app.build_remote(&host).is_err());
        Ok(())
    }
}

#[cfg(test)]
#[path = "connection_gate_tests.rs"]
mod identity_gate_tests;
