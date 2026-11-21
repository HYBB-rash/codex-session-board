use anyhow::{Context, Result, ensure};
use std::ffi::OsStr;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::net::TcpListener;
use std::ops::RangeInclusive;
use std::os::unix::fs::{DirBuilderExt, FileExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct PortLease {
    pub port: u16,
    _lock: File,
}

impl PortLease {
    pub fn acquire() -> Result<Arc<Self>> {
        let (minimum, automatic) = port_ranges()?;
        let directory = lock_directory()?;
        for port in (minimum..=u16::MAX).rev() {
            if automatic.contains(&port) {
                continue;
            }
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(directory.join(format!("{port}.lock")))?;
            match fs2::FileExt::try_lock_exclusive(&lock) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
                Err(error) => return Err(error.into()),
            }
            match TcpListener::bind(("127.0.0.1", port)) {
                Ok(listener) => {
                    drop(listener);
                    return Ok(Arc::new(Self { port, _lock: lock }));
                }
                Err(error) if error.kind() == ErrorKind::AddrInUse => continue,
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!("No available test port outside the automatic port range")
    }
}

fn lock_directory() -> Result<PathBuf> {
    let owner = rustix::process::getuid().as_raw();
    let directory = PathBuf::from(format!("/tmp/codex-session-board-test-ports-{owner}"));
    match DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(&directory)?;
    ensure!(
        metadata.is_dir() && metadata.uid() == owner && metadata.permissions().mode() & 0o077 == 0,
        "Test port lock directory must be private and owned by the current user"
    );
    Ok(directory)
}

#[cfg(target_os = "linux")]
fn port_ranges() -> Result<(u16, RangeInclusive<u16>)> {
    let automatic = fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range")?
        .split_whitespace()
        .map(str::parse::<u16>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        automatic.len() == 2 && automatic[0] <= automatic[1],
        "Invalid automatic port range"
    );
    let minimum = fs::read_to_string("/proc/sys/net/ipv4/ip_unprivileged_port_start")?
        .trim()
        .parse::<u16>()?
        .max(1);
    Ok((minimum, automatic[0]..=automatic[1]))
}

#[cfg(not(target_os = "linux"))]
fn port_ranges() -> Result<(u16, RangeInclusive<u16>)> {
    let first = sysctl_port("first")?.min(sysctl_port("hifirst")?);
    let last = sysctl_port("last")?.max(sysctl_port("hilast")?);
    ensure!(first <= last, "Invalid automatic port range");
    Ok((1024, first..=last))
}

#[cfg(not(target_os = "linux"))]
fn sysctl_port(name: &str) -> Result<u16> {
    let output = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", &format!("net.inet.ip.portrange.{name}")])
        .output()?;
    ensure!(output.status.success(), "Cannot read automatic port range");
    Ok(std::str::from_utf8(&output.stdout)?.trim().parse()?)
}

pub struct ChildGuard {
    pub child: Child,
    port: Arc<PortLease>,
    stderr: File,
}

fn read_startup(mut stdout: ChildStdout, bytes: &mut Vec<u8>) -> Result<serde_json::Value> {
    rustix::io::ioctl_fionbio(&stdout, true)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut buffer = [0; 4096];
    loop {
        ensure!(Instant::now() < deadline, "serve 首条启动回执超时（30 秒）");
        match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let chunk = &buffer[..count];
                let end = chunk.iter().position(|&byte| byte == b'\n');
                let used = end.map_or(count, |index| index + 1);
                ensure!(bytes.len() + used <= 1024 * 1024, "serve 启动回执超过 1 MiB");
                bytes.extend_from_slice(&chunk[..used]);
                if end.is_some() {
                    break;
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error).context("读取 serve 启动回执"),
        }
    }
    ensure!(!bytes.is_empty(), "serve 启动回执为空");
    serde_json::from_slice(bytes).context("serve 启动回执 JSON 无效")
}

impl ChildGuard {
    pub fn startup_receipt(&mut self) -> Result<serde_json::Value> {
        let stdout = self.child.stdout.take().context("serve 缺少启动回执管道")?;
        let mut bytes = Vec::new();
        let error = match read_startup(stdout, &mut bytes) {
            Ok(receipt) => return Ok(receipt),
            Err(error) => error,
        };
        let deadline = Instant::now() + Duration::from_millis(250);
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status.to_string();
            }
            if Instant::now() >= deadline {
                break "子进程仍在运行".into();
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let length = self.stderr.metadata()?.len();
        let offset = length.saturating_sub(4096);
        let mut tail = vec![0; (length - offset) as usize];
        self.stderr.read_exact_at(&mut tail, offset)?;
        Err(error).context(format!("serve 启动：port={}，pid={}，{status}，stdout={} 字节，stderr 末尾：{}",
            self.port.port, self.child.id(), bytes.len(), String::from_utf8_lossy(&tail)))
    }

    pub fn spawn(
        executable: impl AsRef<OsStr>,
        config: &Path,
        port: Arc<PortLease>,
    ) -> Result<Self> {
        Self::spawn_in(Command::new(executable), config, port, &std::env::current_dir()?)
    }

    pub fn spawn_in(
        mut command: Command,
        config: &Path,
        port: Arc<PortLease>,
        directory: &Path,
    ) -> Result<Self> {
        let stderr = tempfile::tempfile()?;
        let child = command
            .current_dir(directory)
            .arg("--config")
            .arg(config)
            .arg("serve")
            .stdout(Stdio::piped())
            .stderr(stderr.try_clone()?)
            .spawn()?;
        Ok(Self { child, port, stderr })
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
