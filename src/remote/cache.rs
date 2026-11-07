use super::*;
use std::path::Path;
use std::sync::Arc;

impl App {
    pub(crate) fn load_remote_cache(&self) -> Result<()> {
        if self.config.legacy_mode || self.config.host_id != "local" {
            return Ok(());
        }
        let root = self.config.data_dir.join("remote-hosts");
        if !root.try_exists()? {
            return Ok(());
        }
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let result = (|| {
                ensure!(entry.file_type()?.is_dir(), "缓存主机目录不是普通目录");
                self.install_cached_remote(&entry.path())
            })();
            if let Err(error) = result {
                errors.push(json!({"directory":entry.file_name().to_string_lossy(),
                    "error":error.to_string()}));
            }
        }
        self.update_health(json!({"remoteCacheErrors":errors}))
    }

    fn install_cached_remote(&self, path: &Path) -> Result<()> {
        let host = hosts::source::cached_host(path, &self.config)?;
        let database = path.join("status.sqlite3");
        if !database.try_exists()? {
            return Ok(());
        }
        let metadata = std::fs::symlink_metadata(database)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "缓存数据库不是普通文件"
        );
        if self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .contains_key(&host.host_id)
        {
            return Ok(());
        }
        let mut config = self.config.clone();
        config.host_id = host.host_id.clone();
        config.data_dir = path.into();
        config.proxy_command.clear();
        config.clock_command.clear();
        config.remote_binding = None;
        let remote = Arc::new(App::new(config)?);
        remote.update_health(json!({"hostLabel":host.display_name}))?;
        remote.deactivate()?;
        self.remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .entry(host.host_id)
            .or_insert(remote);
        Ok(())
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
