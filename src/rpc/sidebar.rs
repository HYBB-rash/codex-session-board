use super::*;
use std::os::unix::fs::FileTypeExt;

fn stale(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Errno>()
        .is_some_and(|error| matches!(*error, Errno::CONNREFUSED | Errno::NOENT))
        || error.downcast_ref::<io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            )
        })
}

fn endpoint(path: &Path, deadline: Instant) -> Result<UnixStream> {
    remaining(deadline)?;
    let path = path.canonicalize()?;
    let metadata = path.metadata()?;
    let parent = path.parent().context("分区连接缺少父目录")?.metadata()?;
    let owner = rustix::process::getuid();
    ensure!(
        metadata.file_type().is_socket()
            && metadata.uid() == owner.as_raw()
            && metadata.mode() & 0o7777 == 0o600
            && parent.uid() == owner.as_raw()
            && parent.mode() & 0o022 == 0,
        "分区连接所有者或权限无效"
    );
    let stream = connect_unix(&path, deadline)?;
    ensure!(
        rustix::net::sockopt::socket_peercred(&stream)?.uid == owner,
        "分区连接服务端身份无效"
    );
    Ok(stream)
}

pub(super) fn connect(configured: &Path, deadline: Instant) -> Result<UnixStream> {
    match endpoint(configured, deadline) {
        Ok(stream) => return Ok(stream),
        Err(error) if stale(&error) => {}
        Err(error) => return Err(error),
    }
    let parent = configured.parent().context("分区连接缺少父目录")?;
    let mut connected = None;
    for entry in std::fs::read_dir(parent)? {
        remaining(deadline)?;
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "sock") {
            continue;
        }
        match endpoint(&path, deadline) {
            Ok(stream) => {
                ensure!(
                    connected.is_none(),
                    "多个分区连接仍存活，无法唯一确定新 socket"
                );
                connected = Some(stream);
            }
            Err(error) if stale(&error) => {}
            Err(error) => return Err(error),
        }
    }
    connected.context("客户端分区连接暂不可用，等待客户端恢复后自动重试")
}
