use super::paths::no_symlinks;
use anyhow::{Context, Result, ensure};
use std::fs::{self, DirBuilder, Metadata};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Created {
    pub(super) entries: Vec<(PathBuf, Metadata)>,
}

pub(super) struct Stage(pub(super) PathBuf);

impl Created {
    pub(super) fn directory(&mut self, path: &Path) -> Result<()> {
        no_symlinks(path)?;
        if path.is_dir() {
            return Ok(());
        }
        self.directory(path.parent().context("目标目录缺少父目录")?)?;
        self.new_directory(path)
    }

    pub(super) fn new_directory(&mut self, path: &Path) -> Result<()> {
        DirBuilder::new().mode(0o700).create(path)?;
        self.entries
            .push((path.into(), fs::symlink_metadata(path)?));
        Ok(())
    }

    pub(super) fn rollback(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        for (path, original) in self.entries.drain(..).rev() {
            let result = (|| -> Result<()> {
                let current = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                ensure!(
                    current.dev() == original.dev() && current.ino() == original.ino(),
                    "迁移目标被外部替换，保留：{}",
                    path.display()
                );
                if original.is_dir() {
                    fs::remove_dir(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                errors.push(format!("{}：{error:#}", path.display()));
            }
        }
        ensure!(errors.is_empty(), "迁移回滚未完成：{}", errors.join("；"));
        Ok(())
    }
}

impl Stage {
    pub(super) fn new(target: &Path) -> Result<Self> {
        let path = target.join(format!(".migration-{}", uuid::Uuid::new_v4()));
        DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
