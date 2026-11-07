use super::*;

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
        assert!(!Path::new(&format!("/proc/{identifier}")).exists());
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
