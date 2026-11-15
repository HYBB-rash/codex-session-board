use anyhow::{Context, Result};
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct Proxy {
    _child: OwnedChild,
    input: ChildStdin,
    output: ChildStdout,
    deadline: Instant,
}

impl Proxy {
    pub fn new(command: &[String], timeout: Duration) -> Result<Self> {
        let mut child = OwnedChild(Command::new(command.first().context("fixture program missing")?)
            .args(&command[1..]).stdin(Stdio::piped()).stdout(Stdio::piped())
            .stderr(Stdio::null()).spawn()?);
        let input = child.0.stdin.take().context("fixture stdin missing")?;
        let output = child.0.stdout.take().context("fixture stdout missing")?;
        rustix::io::ioctl_fionbio(&input, true)?;
        rustix::io::ioctl_fionbio(&output, true)?;
        Ok(Self { _child: child, input, output, deadline: Instant::now() + timeout })
    }

    fn wait(&self, descriptor: BorrowedFd<'_>, events: rustix::event::PollFlags) -> io::Result<()> {
        loop {
            let remaining = self.deadline.checked_duration_since(Instant::now())
                .ok_or(io::ErrorKind::TimedOut)?;
            let timeout = rustix::event::Timespec::try_from(remaining).map_err(io::Error::other)?;
            let mut descriptors = [rustix::event::PollFd::new(&descriptor, events)];
            match rustix::event::poll(&mut descriptors, Some(&timeout)) {
                Ok(0) | Err(rustix::io::Errno::INTR) => {}
                Ok(_) => return Ok(()),
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl Read for Proxy {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.wait(self.output.as_fd(), rustix::event::PollFlags::IN)?;
            match self.output.read(bytes) {
                Err(error) if matches!(error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => continue,
                result => return result,
            }
        }
    }
}

impl Write for Proxy {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.wait(self.input.as_fd(), rustix::event::PollFlags::OUT)?;
            match self.input.write(bytes) {
                Err(error) if matches!(error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
