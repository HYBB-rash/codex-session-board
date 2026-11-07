use super::database::import_database;
use super::paths::{SIDECARS, database_paths, missing, no_symlinks, source_file};
use super::transaction::Created;
use crate::config;
use anyhow::{Context, Result, ensure};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

struct Entry {
    source: PathBuf,
    relative: PathBuf,
    database: bool,
}

pub(super) struct Plan {
    entries: Vec<Entry>,
    hosts: Vec<PathBuf>,
    pub(super) main_database: bool,
}

impl Plan {
    pub(super) fn new(source: &Path) -> Result<Self> {
        let mut plan = Self {
            entries: Vec::new(),
            hosts: Vec::new(),
            main_database: source_file(source)?,
        };
        plan.add_database(source, Path::new("status.sqlite3"))?;
        let hosts = source
            .parent()
            .context("源状态库缺少父目录")?
            .join("remote-hosts");
        no_symlinks(&hosts)?;
        if hosts.try_exists()? {
            let mut directories = fs::read_dir(&hosts)?.collect::<std::io::Result<Vec<_>>>()?;
            directories.sort_by_key(|entry| entry.file_name());
            for entry in directories {
                no_symlinks(&entry.path())?;
                if entry.file_type()?.is_dir() {
                    plan.add_host(&entry.path())?;
                }
            }
        }
        Ok(plan)
    }

    fn add_database(&mut self, source: &Path, relative: &Path) -> Result<()> {
        database_paths(source, |path| source_file(path).map(|_| ()))?;
        if source_file(source)? {
            self.entries.push(Entry {
                source: source.into(),
                relative: relative.into(),
                database: true,
            });
        }
        Ok(())
    }

    fn add_host(&mut self, directory: &Path) -> Result<()> {
        let name = directory.file_name().context("主机目录缺少名称")?;
        let text = name.to_str().context("主机目录名称无效")?;
        ensure!(
            text.len() == 64
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "主机目录不是持久化主机哈希：{text}"
        );
        for entry in fs::read_dir(directory)? {
            no_symlinks(&entry?.path())?;
        }
        let relative = Path::new("remote-hosts").join(name);
        self.hosts.push(relative.clone());
        self.add_database(
            &directory.join("status.sqlite3"),
            &relative.join("status.sqlite3"),
        )?;
        for filename in SIDECARS {
            let source = directory.join(filename);
            if source_file(&source)? {
                self.entries.push(Entry {
                    source,
                    relative: relative.join(filename),
                    database: false,
                });
            }
        }
        Ok(())
    }

    pub(super) fn validate(&self, config_path: &Path, target: &Path) -> Result<()> {
        missing(config_path)?;
        no_symlinks(target)?;
        database_paths(&target.join("status.sqlite3"), missing)?;
        for host in &self.hosts {
            missing(&target.join(host))?;
        }
        for entry in &self.entries {
            ensure!(
                config_path != target.join(&entry.relative),
                "配置目标与迁移数据路径重叠"
            );
        }
        Ok(())
    }

    pub(super) fn prepare(&self, directory: &Path) -> Result<()> {
        for host in &self.hosts {
            config::create_private_dir(&directory.join(host))?;
        }
        for entry in &self.entries {
            let target = directory.join(&entry.relative);
            if entry.database {
                import_database(&entry.source, &target)?;
            } else {
                ensure!(source_file(&entry.source)?, "源 sidecar 已消失");
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(target)?;
                file.write_all(&fs::read(&entry.source)?)?;
                file.sync_all()?;
            }
        }
        Ok(())
    }

    pub(super) fn install(&self, stage: &Path, target: &Path, created: &mut Created) -> Result<()> {
        for host in &self.hosts {
            let path = target.join(host);
            created.directory(path.parent().context("主机目标缺少父目录")?)?;
            created.new_directory(&path)?;
        }
        for entry in &self.entries {
            let source = stage.join(&entry.relative);
            let path = target.join(&entry.relative);
            if entry.database {
                database_paths(&path, missing)?;
            } else {
                missing(&path)?;
            }
            let metadata = fs::metadata(&source)?;
            fs::hard_link(source, &path)?;
            created.entries.push((path, metadata));
        }
        Ok(())
    }
}
