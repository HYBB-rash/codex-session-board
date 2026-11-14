use super::*;
use std::os::fd::{AsFd, BorrowedFd};

#[derive(Debug)]
pub(super) enum Transport {
    Unix(UnixStream),
    Proxy {
        _process: ManagedChild,
        input: ChildStdin,
        output: ChildStdout,
    },
}

impl Transport {
    fn descriptor(&self, writable: bool) -> BorrowedFd<'_> {
        match self {
            Self::Unix(stream) => stream.as_fd(),
            Self::Proxy { input, .. } if writable => input.as_fd(),
            Self::Proxy { output, .. } => output.as_fd(),
        }
    }

    pub(super) fn io<Output>(
        &mut self,
        deadline: Instant,
        writable: bool,
        mut operation: impl FnMut(&mut Self) -> io::Result<Output>,
    ) -> io::Result<Output> {
        loop {
            remaining(deadline)?;
            match operation(self) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let events = if writable { rustix::event::PollFlags::OUT }
                        else { rustix::event::PollFlags::IN };
                    wait_ready(&self.descriptor(writable), events, deadline)?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                result => {
                    remaining(deadline)?;
                    return result;
                }
            }
        }
    }

    pub(super) fn proxy(command: &[String]) -> Result<Self> {
        let program = command.first().context("缺少远程代理命令")?;
        let mut process = ManagedChild(
            Command::new(program)
                .args(&command[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .context("无法启动共享代理")?,
        );
        let input = process.0.stdin.take().context("代理缺少 stdin")?;
        let output = process.0.stdout.take().context("代理缺少 stdout")?;
        rustix::io::ioctl_fionbio(&input, true)?;
        rustix::io::ioctl_fionbio(&output, true)?;
        Ok(Self::Proxy {
            _process: process,
            input,
            output,
        })
    }

    pub(super) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            Self::Unix(stream) => stream.shutdown(how),
            Self::Proxy { .. } => Ok(()),
        }
    }
}

pub(crate) fn first_line(command: &[String], timeout: Duration) -> Result<String> {
    let deadline = deadline_after(timeout)?;
    let mut stream = DeadlineStream {
        stream: Transport::proxy(command)?,
        deadline,
    };
    line(&mut stream)
}

pub(super) fn line(stream: &mut impl Read) -> Result<String> {
    let mut line = Vec::new();
    loop {
        ensure!(line.len() < 256, "时钟探针响应过长");
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return Ok(String::from_utf8(line)?);
        }
        line.push(byte[0]);
    }
}

impl From<UnixStream> for Transport {
    fn from(stream: UnixStream) -> Self {
        Self::Unix(stream)
    }
}

impl Read for Transport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Unix(stream) => stream.read(bytes),
            Self::Proxy { output, .. } => output.read(bytes),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Unix(stream) => stream.write(bytes),
            Self::Proxy { input, .. } => input.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Unix(stream) => stream.flush(),
            Self::Proxy { input, .. } => input.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_waits_for_readiness_without_repeated_would_block_reads() -> Result<()> {
        let (reader, mut writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        let sender = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            writer.write_all(b"x")
        });
        let mut transport = Transport::Unix(reader);
        let (mut attempts, mut received) = (0, [0]);
        let count = transport.io(deadline_after(Duration::from_secs(2))?, false, |stream| {
            attempts += 1;
            if attempts == 1 { return Err(io::ErrorKind::Interrupted.into()); }
            stream.read(&mut received)
        })?;
        sender.join().unwrap()?;
        assert_eq!((count, received), (1, *b"x"));
        assert!((2..=3).contains(&attempts), "未就绪时重复读取了 {attempts} 次");
        Ok(())
    }

    #[test]
    fn proxy_deadline_applies_to_a_silent_child_and_drop_reaps_it() -> Result<()> {
        let command = vec![
            "python3".into(),
            "-c".into(),
            "import time; time.sleep(30)".into(),
        ];
        let transport = Transport::proxy(&command)?;
        let identifier = match &transport {
            Transport::Proxy { _process, .. } => _process.0.id(),
            _ => unreachable!(),
        };
        let started = Instant::now();
        let result = DesktopConnection::handshake(
            transport,
            Duration::from_millis(80),
            Role::Reader,
            deadline_after(Duration::from_millis(80))?,
        );
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        let pid = rustix::process::Pid::from_raw(identifier.try_into()?);
        assert!(matches!(
            rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
            Err(Errno::CHILD)
        ));
        Ok(())
    }

    #[test]
    fn proxy_stream_preserves_arbitrary_websocket_bytes() -> Result<()> {
        let mut transport = Transport::proxy(&[
            "python3".into(),
            "-u".into(),
            "-c".into(),
            "import os; os.write(1,os.read(0,32)[::-1])".into(),
        ])?;
        let bytes = [0, 255, 128, 13, 10, 81];
        let deadline = deadline_after(Duration::from_secs(2))?;
        deadline_io(deadline, || transport.write_all(&bytes))?;
        let mut actual = [0; 6];
        let mut stream = DeadlineStream {
            stream: transport,
            deadline,
        };
        stream.read_exact(&mut actual)?;
        assert_eq!(actual, [81, 10, 13, 128, 255, 0]);
        Ok(())
    }
}
