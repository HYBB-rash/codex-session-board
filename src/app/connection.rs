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
        ensure!(!self.config.legacy_mode || !matches!(role, Role::DraftWriter | Role::ArchiveWriter),
            "六状态兼容模式不允许创建或归档远程会话");
        DesktopConnection::configured(ConnectionRequest(&self.config), timeout, role)
    }
}
