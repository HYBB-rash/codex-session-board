use crate::config::{self, Config};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub(super) const SIDECARS: [&str; 2] = ["source.json", "scan-progress.json"];
const DATABASE_SUFFIXES: [&str; 4] = ["", "-wal", "-shm", "-journal"];

pub(super) fn no_symlinks(path: &Path) -> Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        match fs::symlink_metadata(&prefix) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "迁移路径包含符号链接：{}",
                prefix.display()
            ),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn missing_entry(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => anyhow::bail!("迁移目标已经存在，不会覆盖：{}", path.display()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn missing(path: &Path) -> Result<()> {
    no_symlinks(path)?;
    missing_entry(path)
}

pub(super) fn resolve_parent(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().context("迁移文件缺少父目录")?;
    let name = path.file_name().context("迁移文件缺少名称")?;
    Ok(config::resolve_path(parent)?.join(name))
}

pub(in crate::service) fn configuration(path: &Path) -> Result<PathBuf> {
    missing_entry(path)?;
    let path = resolve_parent(path)?;
    missing(&path)?;
    Ok(path)
}

pub(super) fn source_file(path: &Path) -> Result<bool> {
    no_symlinks(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(metadata.is_file(), "迁移源不是普通文件：{}", path.display());
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(in crate::service) fn lock(data: &Path) -> Result<std::fs::File> {
    let data = config::resolve_path(data)?;
    source_file(&data.join("launch.lock"))?;
    let file = super::super::lock_file(&data)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(file),
        Err(error) if error.kind() == ErrorKind::WouldBlock => {
            bail!("目标数据目录仍有受管进程运行，不能迁移")
        }
        Err(error) => Err(error.into()),
    }
}

pub(in crate::service) fn load_source(path: &Path) -> Result<Config> {
    let path = config::resolve_path(path)?;
    ensure!(source_file(&path)?, "源配置不存在");
    config::load(Some(&path))
}

pub(super) fn database_paths(
    path: &Path,
    mut check: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    for suffix in DATABASE_SUFFIXES {
        let mut sibling = path.as_os_str().to_os_string();
        sibling.push(suffix);
        check(Path::new(&sibling))?;
    }
    Ok(())
}
