use anyhow::{Result, ensure};
use std::ffi::OsStr;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::ErrorKind;
use std::net::TcpListener;
use std::ops::RangeInclusive;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

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
    _port: Arc<PortLease>,
}

impl ChildGuard {
    pub fn spawn(
        executable: impl AsRef<OsStr>,
        config: &Path,
        port: Arc<PortLease>,
    ) -> Result<Self> {
        let child = Command::new(executable)
            .arg("--config")
            .arg(config)
            .arg("serve")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        Ok(Self { child, _port: port })
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
