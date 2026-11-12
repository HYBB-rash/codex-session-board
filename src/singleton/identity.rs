use anyhow::{Context, Result, ensure};
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Identity {
    boot: String,
    start: String,
    executable: (u64, u64),
    listener: String,
}

pub(super) fn inode(metadata: &fs::Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

pub(super) fn anchored(directory: &File, name: &str) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd())).join(name)
}

pub(super) fn private_directory(path: &Path, temporary: bool) -> Result<File> {
    use rustix::fs::{Mode, OFlags, open};
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let directory = File::from(open(path, flags, Mode::empty())?);
    let metadata = directory.metadata()?;
    ensure!(
        metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o022 == 0
            && metadata.nlink() > 0,
        "singleton 目录所有者、权限或链接状态无效"
    );
    ensure!(
        !temporary || metadata.mode() & 0o777 == 0o700,
        "singleton 临时目录权限必须为 0700"
    );
    Ok(directory)
}

pub(super) fn same_directory(directory: &File, path: &Path, temporary: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o022 == 0
            && (!temporary || metadata.mode() & 0o777 == 0o700)
            && inode(&directory.metadata()?) == inode(&metadata)
            && directory.metadata()?.nlink() > 0,
        "singleton 目录已删除或替换"
    );
    Ok(())
}

pub(super) fn process(lock: &Path, socket: &Path) -> Result<Identity> {
    let lock = lock.to_str().context("singleton 进程锁不是文本")?;
    let (hostname, pid) = lock.rsplit_once('-').context("singleton 进程锁格式无效")?;
    ensure!(
        hostname == fs::read_to_string("/proc/sys/kernel/hostname")?.trim(),
        "singleton 属于其他主机"
    );
    let pid: u32 = pid.parse()?;
    ensure!(pid > 0, "singleton PID 无效");
    let root = PathBuf::from(format!("/proc/{pid}"));
    ensure!(
        root.metadata()?.uid() == rustix::process::getuid().as_raw(),
        "singleton 进程属于其他用户"
    );
    let start = start_time(&root)?;
    let executable = inode(&fs::metadata(root.join("exe"))?);
    let listener = listener_inode(socket)?;
    let target = PathBuf::from(format!("socket:[{listener}]"));
    let owns = fs::read_dir(root.join("fd"))?.any(|entry| {
        entry
            .ok()
            .and_then(|entry| fs::read_link(entry.path()).ok())
            .as_ref()
            == Some(&target)
    });
    ensure!(owns, "singleton socket 不属于锁中进程");
    ensure!(start == start_time(&root)?, "singleton 进程已经换代");
    Ok(Identity {
        boot: fs::read_to_string("/proc/sys/kernel/random/boot_id")?,
        start,
        executable,
        listener,
    })
}

fn start_time(root: &Path) -> Result<String> {
    let stat = fs::read_to_string(root.join("stat"))?;
    let (_, tail) = stat.rsplit_once(')').context("无效进程状态")?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    ensure!(
        !matches!(fields.first(), Some(&"Z" | &"X")),
        "singleton 进程已经退出"
    );
    Ok(fields.get(19).context("缺少进程启动时间")?.to_string())
}

fn listener_inode(socket: &Path) -> Result<String> {
    let path = socket.to_str().context("singleton socket 路径不是 UTF-8")?;
    let table = fs::read_to_string("/proc/net/unix")?;
    let mut matches = table.lines().filter_map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        (fields.len() >= 8
            && fields[3] == "00010000"
            && fields[4] == "0001"
            && fields[5] == "01"
            && fields[7..].join(" ") == path)
            .then(|| fields[6].to_string())
    });
    let inode = matches
        .next()
        .context("singleton socket 已失去监听；请重启客户端")?;
    ensure!(
        matches.next().is_none(),
        "singleton socket 存在多个监听实例"
    );
    Ok(inode)
}
