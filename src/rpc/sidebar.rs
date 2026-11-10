use super::*;
use super::unavailable::{Unavailable, stale};
use std::os::unix::fs::FileTypeExt;

#[cfg(all(test, target_os = "linux"))]
#[path = "sidebar_tests.rs"]
mod tests;

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
    remaining(deadline)?;
    Ok(stream)
}

fn directories(configured: &Path) -> Result<Vec<PathBuf>> {
    let parent = configured.parent().context("分区连接缺少父目录")?;
    let mut directories = vec![parent.to_path_buf()];
    if cfg!(target_os = "linux")
        && let Some(root) = parent.parent()
    {
        let private = format!("codex-app-tools-{}", rustix::process::getuid().as_raw());
        let alternate = match parent.file_name().and_then(|name| name.to_str()) {
            Some("codex-app-tools") => Some(private.as_str()),
            Some(name) if name == private => Some("codex-app-tools"),
            _ => None,
        };
        if let Some(alternate) = alternate {
            directories.push(root.join(alternate));
        }
    }
    Ok(directories)
}

fn entries(directory: &Path) -> Result<Option<std::fs::ReadDir>> {
    let metadata = match std::fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_dir()
            && metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o022 == 0,
        "分区连接目录所有者或权限无效"
    );
    match std::fs::read_dir(directory) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn discover(configured: &Path, deadline: Instant) -> Result<UnixStream> {
    let mut connected = None;
    for directory in directories(configured)? {
        remaining(deadline)?;
        let Some(entries) = entries(&directory)? else {
            continue;
        };
        for entry in entries {
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
    }
    remaining(deadline)?;
    connected.ok_or_else(|| {
        anyhow!("未发现可用的侧栏连接：{}", configured.display())
            .context(Unavailable::Sidebar)
    })
}

pub(super) fn connect(configured: &Path, deadline: Instant) -> Result<UnixStream> {
    match endpoint(configured, deadline) {
        Ok(stream) => Ok(stream),
        Err(error) if stale(&error) => discover(configured, deadline),
        Err(error) => Err(error),
    }
}
