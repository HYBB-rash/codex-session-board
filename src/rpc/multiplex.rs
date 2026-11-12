use super::*;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub(crate) struct Multiplex(Arc<Mutex<State>>);

#[derive(Debug, Default)]
struct State {
    master: Option<Master>,
    closed: bool,
}

#[derive(Debug)]
struct Master {
    process: ManagedChild,
    directory: tempfile::TempDir,
    command: Vec<String>,
}

fn options(command: &[String], values: &[String]) -> Result<Vec<String>> {
    let mut command = command.to_vec();
    let boundary = command
        .iter()
        .rposition(|value| value == "--")
        .context("SSH 命令缺少目标边界")?;
    command.splice(boundary..boundary, values.iter().cloned());
    Ok(command)
}

impl Master {
    fn start(command: &[String], deadline: Instant) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("csb-ssh-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")?;
        let path = directory.path().join("mux");
        let arguments = options(
            command,
            &[
                "-M".into(),
                "-N".into(),
                "-o".into(),
                "ControlMaster=yes".into(),
                "-o".into(),
                format!("ControlPath={}", path.display()),
                "-o".into(),
                "ControlPersist=no".into(),
                "-o".into(),
                "ForkAfterAuthentication=no".into(),
                "-o".into(),
                "ClearAllForwardings=yes".into(),
                "-o".into(),
                "ForwardAgent=no".into(),
            ],
        )?;
        let process = ManagedChild(
            Command::new(&arguments[0])
                .args(&arguments[1..])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let mut master = Self {
            process,
            directory,
            command: command.to_vec(),
        };
        master.ready(deadline)?;
        Ok(master)
    }

    fn ready(&mut self, deadline: Instant) -> Result<()> {
        let path = self.directory.path().join("mux");
        loop {
            let left = remaining(deadline)?;
            ensure!(self.process.0.try_wait()?.is_none(), "SSH 共享连接启动失败");
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    ensure!(metadata.file_type().is_socket(), "SSH 共享端点不是 socket");
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            thread::sleep(left.min(Duration::from_millis(2)));
        }
    }
}

impl Multiplex {
    pub(crate) fn close(&self) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| anyhow!("SSH 共享连接锁已损坏"))?;
        state.closed = true;
        state.master = None;
        Ok(())
    }

    pub(crate) fn command(&self, command: &[String], deadline: Instant) -> Result<Vec<String>> {
        let boundary = command
            .iter()
            .rposition(|value| value == "--")
            .context("SSH 命令缺少目标边界")?;
        let target = command.get(..boundary + 2).context("SSH 命令缺少目标")?;
        let mut state = loop {
            match self.0.try_lock() {
                Ok(state) => break state,
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(remaining(deadline)?.min(Duration::from_millis(2)))
                }
                Err(_) => bail!("SSH 共享连接锁已损坏"),
            }
        };
        remaining(deadline)?;
        ensure!(!state.closed, "SSH 主机连接已停止");
        let alive = match state.master.as_mut() {
            Some(master) => master.command == target && master.process.0.try_wait()?.is_none(),
            None => false,
        };
        if !alive {
            state.master = None;
            state.master = Some(Master::start(target, deadline)?);
        }
        let master = state.master.as_ref().context("SSH 共享连接未建立")?;
        options(
            command,
            &[
                "-o".into(),
                "ControlMaster=no".into(),
                "-o".into(),
                format!(
                    "ControlPath={}",
                    master.directory.path().join("mux").display()
                ),
            ],
        )
    }
}

#[cfg(test)]
#[path = "multiplex_tests.rs"]
mod tests;
