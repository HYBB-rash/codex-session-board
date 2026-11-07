use super::paths::{database_paths, missing, source_file};
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

fn copy_database(source: &Path, target: &Path) -> Result<()> {
    let source = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let mut target = Connection::open_with_flags(
        target,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    Backup::new(&source, &mut target)?.run_to_completion(128, Duration::from_millis(10), None)?;
    target.execute_batch("PRAGMA journal_mode=DELETE;")?;
    Ok(())
}

pub(in crate::service) fn import_database(source: &Path, target: &Path) -> Result<bool> {
    database_paths(target, missing)?;
    database_paths(source, |path| source_file(path).map(|_| ()))?;
    if !source_file(source)? {
        return Ok(false);
    }
    let reserved = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)?;
    drop(reserved);
    if let Err(error) = copy_database(source, target) {
        fs::remove_file(target).context("迁移失败且新建状态库未能清理")?;
        return Err(error);
    }
    Ok(true)
}
