use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

mod identity;
pub(crate) mod runtime;
use identity::{Identity, anchored, inode, private_directory, same_directory};

const LINKS: [&str; 3] = ["SingletonLock", "SingletonSocket", "SingletonCookie"];

pub struct Guard {
    profile: File,
    profile_path: PathBuf,
    directory: File,
    directory_path: PathBuf,
    links: [PathBuf; 3],
    process: Identity,
    socket_inode: (u64, u64),
}

fn link(path: &Path) -> Result<Option<PathBuf>> {
    match fs::read_link(path) {
        Ok(target) => Ok(Some(target)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("singleton 标记必须为符号链接"),
    }
}

fn links(profile: &File) -> Result<Option<[PathBuf; 3]>> {
    let values = LINKS.map(|name| link(&anchored(profile, name)));
    let [lock, socket, cookie] = values;
    match (lock?, socket?, cookie?) {
        (None, None, None) => Ok(None),
        (Some(lock), Some(socket), Some(cookie)) => Ok(Some([lock, socket, cookie])),
        _ => anyhow::bail!("singleton 标记不完整，客户端可能正在启动或退出；请稍后重试"),
    }
}

fn socket_inode(path: &Path) -> Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_socket() && metadata.uid() == rustix::process::getuid().as_raw(),
        "singleton socket 类型或所有者无效"
    );
    Ok(inode(&metadata))
}

impl Guard {
    pub fn acquire(profile_path: &Path) -> Result<Option<Self>> {
        if matches!(fs::symlink_metadata(profile_path), Err(e) if e.kind() == ErrorKind::NotFound) {
            return Ok(None);
        }
        let profile = private_directory(profile_path, false)?;
        let Some(links) = links(&profile)? else {
            return Ok(None);
        };
        ensure!(
            links[1].is_absolute() && links[1].file_name().is_some_and(|n| n == "SingletonSocket"),
            "singleton socket 路径无效"
        );
        let cookie = links[2].to_str().context("singleton cookie 不是文本")?;
        ensure!(
            !cookie.is_empty() && cookie.len() <= 128 && cookie.bytes().all(|c| c.is_ascii_digit()),
            "singleton cookie 格式无效"
        );
        let directory_path = links[1]
            .parent()
            .context("singleton socket 缺少父目录")?
            .to_path_buf();
        let directory = private_directory(&directory_path, true)
            .context("singleton 临时目录不可用；请重启客户端")?;
        FileExt::try_lock_shared(&directory).context("singleton 临时目录正在清理，请稍后重试")?;
        let socket_inode = socket_inode(&links[1])?;
        let process = identity::process(&links[0], &links[1])?;
        let guard = Self {
            profile,
            profile_path: profile_path.into(),
            directory,
            directory_path,
            links,
            process,
            socket_inode,
        };
        guard.verify()?;
        Ok(Some(guard))
    }

    pub fn verify(&self) -> Result<()> {
        same_directory(&self.profile, &self.profile_path, false)?;
        same_directory(&self.directory, &self.directory_path, true)?;
        ensure!(
            links(&self.profile)?.as_ref() == Some(&self.links),
            "singleton profile 已换代，请重新跳转"
        );
        ensure!(
            self.socket_inode == socket_inode(&self.links[1])?
                && self.process == identity::process(&self.links[0], &self.links[1])?,
            "singleton 运行实例已经改变，请重新跳转"
        );
        Ok(())
    }

    pub fn cookie_present(&self) -> Result<bool> {
        match link(&anchored(&self.directory, "SingletonCookie"))? {
            None => Ok(false),
            Some(value) => {
                ensure!(
                    value == self.links[2],
                    "singleton cookie 不一致，拒绝覆盖；请重启客户端"
                );
                Ok(true)
            }
        }
    }

    pub fn prepare(&self) -> Result<()> {
        self.verify()?;
        if !self.cookie_present()? {
            match rustix::fs::symlinkat(&self.links[2], &self.directory, "SingletonCookie") {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(error).context("无法恢复 singleton cookie"),
            }
        }
        ensure!(self.cookie_present()?, "singleton cookie 再次被删除");
        self.verify()
    }
}
