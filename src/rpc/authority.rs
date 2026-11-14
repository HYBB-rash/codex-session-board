use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub(super) mod sidebar;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocalAuthority {
    pid: u32,
    started: u64,
}

impl LocalAuthority {
    fn capture(pid: u32) -> Result<Self> {
        let started = crate::service::process_start(pid)?.context("客户端后台已经退出")?;
        Ok(Self { pid, started })
    }

    pub(crate) fn parent() -> Result<Self> {
        Self::capture(rustix::process::getppid().context("缺少客户端后台父进程")?.as_raw_nonzero().get() as u32)
    }

    pub fn owned_listener(listener: &UnixListener) -> Result<Self> {
        ensure!(listener.local_addr()?.as_pathname().is_some(), "需要本进程持有的监听端点");
        Self::capture(std::process::id())
    }

    pub(crate) fn confirm(&self) -> Result<()> {
        ensure!(
            crate::service::process_start(self.pid)? == Some(self.started),
            "客户端后台已退出或重启，请重新连接客户端"
        );
        Ok(())
    }

    pub(super) fn verify(&self, stream: &UnixStream) -> Result<()> {
        ensure!(peer_pid(stream)? == self.pid, "连接对端不是启动本看板的客户端后台，已拒绝使用其他服务");
        self.confirm()
    }
}

fn peer_pid(stream: &UnixStream) -> Result<u32> {
    #[cfg(target_os = "macos")]
    let peer = u32::try_from(nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::LocalPeerPid)?)?;
    #[cfg(target_os = "linux")]
    let peer = rustix::net::sockopt::socket_peercred(stream)?.pid.as_raw_nonzero().get() as u32;
    Ok(peer)
}

pub(crate) fn inherit(config: &mut crate::config::Config) -> Result<()> {
    if let Some(url) = std::env::var_os("CODEX_APP_SERVER_WS_URL") {
        config.inherited_socket = Some(socket_url(url.to_str().context("客户端连接地址不是 UTF-8")?)?);
    }
    config.local_authority = Some(LocalAuthority::parent()?);
    Ok(())
}

fn socket_url(url: &str) -> Result<PathBuf> {
    let socket = url.strip_prefix("ws+unix://localhost")
        .and_then(|value| value.strip_suffix(":/rpc"))
        .context("客户端必须使用可核对进程归属的 Unix 共享连接")?;
    let path = PathBuf::from(socket);
    ensure!(path.is_absolute() && !socket.contains(['?', '#', '%']), "客户端共享连接地址无效");
    Ok(path)
}

#[cfg(test)]
mod tests;
