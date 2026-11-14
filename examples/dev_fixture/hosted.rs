use super::*;
use std::os::fd::OwnedFd;
use std::process::{Child, Command, Stdio};

pub(super) fn record(root: &Path, name: &str, child: &Child, exit: Option<i32>) -> Result<()> {
    let temporary = root.join(format!("{name}-child.pending"));
    std::fs::write(&temporary, json!({
        "pid":child.id(),"parent":std::process::id(),"exit":exit
    }).to_string())?;
    std::fs::rename(temporary, root.join(format!("{name}-child.json")))?;
    Ok(())
}

pub(super) fn accept(listener: &UnixListener, root: &Path, program: &str) -> Result<()> {
    match listener.accept() {
        Ok((stream, _)) => {
            let root = root.to_path_buf();
            let program = program.to_owned();
            std::thread::spawn(move || {
                if let Err(error) = mcp(&root, &program, stream) {
                    eprintln!("fixture MCP 子进程失败：{error}");
                }
            });
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn mcp(root: &Path, program: &str, stream: UnixStream) -> Result<()> {
    stream.set_nonblocking(false)?;
    let input: OwnedFd = stream.try_clone()?.into();
    let output: OwnedFd = stream.into();
    let log = std::fs::File::create(root.join("mcp.log"))?;
    let mut process = BoardProcess(Command::new(program).arg("--config").arg(root.join("config.json"))
        .arg("mcp").stdin(Stdio::from(input)).stdout(Stdio::from(output)).stderr(log).spawn()?);
    record(root, "mcp", &process.0, None)?;
    let status = process.0.wait()?;
    record(root, "mcp", &process.0, Some(status.code().unwrap_or(-1)))?;
    ensure!(status.success(), "包内 MCP 异常退出：{status}");
    Ok(())
}
