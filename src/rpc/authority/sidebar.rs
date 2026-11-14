use super::*;
use anyhow::bail;
use serde_json::Value;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(crate) fn parent(pid: u32) -> Result<u32> {
    let output = std::process::Command::new("/bin/ps")
        .env("LC_ALL", "C").args(["-p", &pid.to_string(), "-o", "ppid="]).output()?;
    ensure!(output.status.success(), "无法证明侧栏客户端进程归属");
    Ok(String::from_utf8(output.stdout)?.trim().parse()?)
}

fn recorded(value: &Value) -> Result<LocalAuthority> {
    let pid = u32::try_from(value["pid"].as_u64().context("共享客户端 PID 缺失")?)?;
    let current = LocalAuthority::capture(pid)?;
    ensure!(serde_json::to_value(&current)? == *value, "共享客户端已经退出或重启");
    Ok(current)
}

fn read(path: &Path) -> Result<Value> {
    let metadata = std::fs::symlink_metadata(path)?;
    let directory = path.parent().context("共享客户端凭证缺少目录")?;
    let parent = std::fs::symlink_metadata(directory)?;
    let owner = rustix::process::getuid().as_raw();
    ensure!(metadata.is_file() && metadata.uid() == owner && metadata.mode() & 0o7777 == 0o600
        && parent.is_dir() && parent.uid() == owner && parent.mode() & 0o7777 == 0o700,
        "共享客户端凭证所有者或权限无效");
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(4097).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "共享客户端凭证过大");
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn verify(backend: &LocalAuthority, stream: &UnixStream, proof: Option<&Path>) -> Result<()> {
    backend.confirm()?;
    if let Some(proof) = proof {
        let value = read(proof)?;
        ensure!(value["backend"] == serde_json::to_value(backend)?, "共享凭证不属于当前后台");
        let client = recorded(&value["client"])?;
        let launcher = recorded(&value["launcher"])?;
        ensure!(parent(backend.pid)? == launcher.pid && parent(client.pid)? == launcher.pid,
            "共享后台与侧栏客户端不属于本次启动");
        client.verify(stream).context("侧栏不属于已核验的共享客户端")?;
        launcher.confirm()?;
    } else if peer_pid(stream)? == backend.pid {
        backend.verify(stream)?;
    } else {
        let client = LocalAuthority::capture(parent(backend.pid)?)?;
        if client.pid <= 1 {
            bail!("侧栏客户端归属无法证明，已拒绝写入");
        }
        client.verify(stream).context("侧栏不属于当前后台的客户端")?;
        ensure!(parent(backend.pid)? == client.pid, "侧栏客户端已经退出或重启");
    }
    backend.confirm()
}

#[cfg(test)]
mod tests;
