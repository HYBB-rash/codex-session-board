use super::App;
use crate::{config::Config, rpc::{DesktopConnection, Role}};
use anyhow::{Result, ensure};
use std::{sync::atomic::Ordering, time::Duration};

#[path = "../preflight.rs"]
pub mod preflight;

pub(crate) struct ConnectionRequest<'a>(&'a Config);

impl ConnectionRequest<'_> {
    pub(crate) fn config(&self) -> &Config {
        self.0
    }
}

impl App {
    pub fn connection(&self, timeout: Duration, role: Role) -> Result<DesktopConnection> {
        ensure!(self.enabled.load(Ordering::SeqCst), "主机登记已改变，请刷新核对");
        ensure!(!self.config.legacy_mode || !matches!(role, Role::DraftReader | Role::CalibrationWriter | Role::ArchiveWriter),
            "六状态兼容模式不允许创建或归档远程会话");
        DesktopConnection::configured(ConnectionRequest(&self.config), timeout, role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn legacy_mode_rejects_draft_calibration_and_archive_connections() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            desktop_socket: directory.path().join("absent.sock"),
            legacy_mode: true,
            ..Config::default()
        })?;
        for role in [Role::DraftReader, Role::CalibrationWriter, Role::ArchiveWriter] {
            let error = app.connection(Duration::from_millis(10), role)
                .err().context("兼容模式意外打开了业务连接")?;
            assert_eq!(error.to_string(), "六状态兼容模式不允许创建或归档远程会话");
        }
        Ok(())
    }
}
