use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

pub(crate) struct Repository {
    pub root: PathBuf,
    pub common: PathBuf,
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法读取 {}", path.display())),
    }
}

fn common_directory(root: &Path, metadata: &Path) -> Result<PathBuf> {
    let directory = if metadata.is_dir() {
        metadata.to_path_buf()
    } else {
        let text = std::fs::read_to_string(metadata)?;
        let target = text
            .trim()
            .strip_prefix("gitdir:")
            .context("Git 目录指针无效")?
            .trim();
        ensure!(!target.is_empty(), "Git 目录指针为空");
        root.join(target)
    };
    ensure!(directory.join("HEAD").is_file(), "Git 工作树元数据不完整");
    let common = match read_optional(&directory.join("commondir"))? {
        Some(target) => {
            ensure!(!target.trim().is_empty(), "Git 共享目录指针为空");
            directory.join(target.trim())
        }
        None => directory,
    };
    common.canonicalize().context("无法解析 Git 共享目录")
}

pub(crate) fn repository(path: &Path) -> Result<Option<Repository>> {
    for root in path.ancestors() {
        let metadata = root.join(".git");
        match std::fs::metadata(&metadata) {
            Ok(_) => {
                return Ok(Some(Repository {
                    root: root.to_path_buf(),
                    common: common_directory(root, &metadata)?,
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("无法探测 Git 工作树"),
        }
    }
    Ok(None)
}
