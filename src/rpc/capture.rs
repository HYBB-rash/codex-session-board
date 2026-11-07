use super::*;
use std::os::unix::process::CommandExt;

const CAPTURE_LIMIT: usize = 65536;

fn drain(reader: &mut impl Read, buffer: &mut Vec<u8>) -> Result<bool> {
    let mut bytes = [0; 4096];
    match reader.read(&mut bytes) {
        Ok(0) => Ok(true),
        Ok(length) => {
            ensure!(
                buffer.len() + length <= CAPTURE_LIMIT,
                "SSH 配置探针输出超限"
            );
            buffer.extend_from_slice(&bytes[..length]);
            Ok(false)
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn output(command: &[String], deadline: Instant) -> Result<Vec<u8>> {
    let program = command.first().context("缺少 SSH 配置探针")?;
    let mut process = ManagedChild(
        Command::new(program)
            .args(&command[1..])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("无法启动 SSH 配置探针")?,
    );
    let result = collect(&mut process, deadline);
    if let Ok(pid) = i32::try_from(process.0.id())
        && let Some(group) = rustix::process::Pid::from_raw(pid)
    {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    result
}

fn collect(process: &mut ManagedChild, deadline: Instant) -> Result<Vec<u8>> {
    let mut stdout = process.0.stdout.take().context("探针缺少 stdout")?;
    let mut stderr = process.0.stderr.take().context("探针缺少 stderr")?;
    rustix::io::ioctl_fionbio(&stdout, true)?;
    rustix::io::ioctl_fionbio(&stderr, true)?;
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    let (mut out_done, mut err_done) = (false, false);
    loop {
        remaining(deadline)?;
        if !out_done {
            out_done = drain(&mut stdout, &mut output)?;
        }
        if !err_done {
            err_done = drain(&mut stderr, &mut errors)?;
        }
        if let Some(status) = process.0.try_wait()?
            && out_done
            && err_done
        {
            ensure!(
                status.success(),
                "SSH 配置解析失败，退出状态 {}",
                status.code().unwrap_or(-1)
            );
            return Ok(output);
        }
        thread::sleep(remaining(deadline)?.min(Duration::from_millis(1)));
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
