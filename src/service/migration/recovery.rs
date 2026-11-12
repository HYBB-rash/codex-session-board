use super::database::copy_database;
use super::paths::source_file;
use super::transaction::Stage;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use rustix::fs::{FlockOperation, OFlags, fcntl_lock};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

fn sidecar(source: &Path, suffix: &str) -> PathBuf {
    let mut path = source.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

pub(super) fn copy(source: &Path, target: &Path) -> Result<()> {
    let mut original = OpenOptions::new()
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(source)?;
    fcntl_lock(&original, FlockOperation::NonBlockingLockShared)
        .context("状态库正在写入，暂不能复制热回滚日志")?;
    ensure!(
        !source_file(&sidecar(source, "-wal"))?,
        "热回滚日志与 WAL 同时存在，保留来源等待人工诊断"
    );
    let journal = sidecar(source, "-journal");
    ensure!(source_file(&journal)?, "热回滚日志已消失，请重试备份");
    let stage = Stage::new(target.parent().context("恢复副本缺少父目录")?)?;
    let recovered = stage.0.join("status.sqlite3");
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&recovered)?;
    std::io::copy(&mut original, &mut destination)?;
    destination.sync_all()?;
    drop(destination);
    fs::copy(journal, sidecar(&recovered, "-journal"))?;
    let database = Connection::open_with_flags(
        &recovered,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    database.query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))?;
    drop(database);
    copy_database(&recovered, target)
}

#[cfg(test)]
mod tests {
    fn live_writer_fixture() -> bool {
        use rusqlite::Connection;
        use std::io::Write;

        const FIXTURE: &str = "SESSION_BOARD_LIVE_WRITER_FIXTURE";
        if let Some(path) = std::env::var_os(FIXTURE) {
            let database = Connection::open(path).unwrap();
            database
                .execute_batch("BEGIN IMMEDIATE; UPDATE fixture SET value = 'pending';")
                .unwrap();
            println!("writer-ready");
            std::io::stdout().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            return true;
        }
        false
    }

    #[test]
    fn private_recovery_rejects_a_live_sqlite_writer() {
        use std::io::{BufRead, Write};
        use std::process::Stdio;

        if live_writer_fixture() {
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.sqlite3");
        let target = directory.path().join("backup.sqlite3");
        let database = Connection::open(&source).unwrap();
        database
            .execute_batch(
                "CREATE TABLE fixture(value TEXT); INSERT INTO fixture VALUES ('committed');",
            )
            .unwrap();
        drop(database);
        let mut writer = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "service::migration::recovery::tests::private_recovery_rejects_a_live_sqlite_writer", "--nocapture"])
            .env("SESSION_BOARD_LIVE_WRITER_FIXTURE", &source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn().unwrap();
        let mut output = std::io::BufReader::new(writer.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert_ne!(output.read_line(&mut line).unwrap(), 0);
            if line.trim() == "writer-ready" {
                break;
            }
        }
        let result = super::copy(&source, &target);
        writer.stdin.take().unwrap().write_all(b"finish\n").unwrap();
        assert!(writer.wait().unwrap().success());
        assert!(result.unwrap_err().to_string().contains("状态库正在写入"));
        assert!(!target.exists());
        assert!(!std::fs::read_dir(directory.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".migration")
        }));
    }

    use anyhow::{Context, Result};
    use rusqlite::{Connection, OpenFlags};
    use std::fs;
    use std::process::Command;

    #[test]
    fn hot_journal_is_recovered_privately_without_changing_source() -> Result<()> {
        const CHILD: &str = "SESSION_BOARD_HOT_JOURNAL_FIXTURE";
        if let Some(path) = std::env::var_os(CHILD) {
            let database = Connection::open(path)?;
            database.execute_batch(
                "PRAGMA cache_size=1; BEGIN IMMEDIATE;
                UPDATE records SET value='uncommitted', payload=zeroblob(3000);",
            )?;
            std::process::exit(0);
        }
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source.sqlite3");
        let journal = directory.path().join("source.sqlite3-journal");
        create_hot_journal(&source)?;
        let original = fs::read(&source)?;
        let original_journal = fs::read(&journal)?;
        assert!(original_journal.len() > 512);
        verify_hot_journal(&source)?;
        let target = directory.path().join("snapshot.sqlite3");
        assert!(super::super::import_database(&source, &target)?);
        let snapshot = Connection::open(&target)?;
        assert_eq!(
            snapshot.query_row(
                "SELECT count(*) FROM records WHERE value='committed'",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            128
        );
        assert_eq!(
            snapshot.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?,
            "ok"
        );
        assert_eq!(fs::read(&source)?, original);
        assert_eq!(fs::read(&journal)?, original_journal);
        assert!(
            !fs::read_dir(directory.path())?.any(|entry| entry.is_ok_and(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".migration-")))
        );
        Ok(())
    }

    fn create_hot_journal(source: &std::path::Path) -> Result<()> {
        let database = Connection::open(source)?;
        database.execute_batch(
            "PRAGMA journal_mode=DELETE;
            CREATE TABLE records(value TEXT, payload BLOB);
            WITH RECURSIVE numbers(number) AS
              (SELECT 1 UNION ALL SELECT number+1 FROM numbers WHERE number<128)
            INSERT INTO records SELECT 'committed',zeroblob(3000) FROM numbers;",
        )?;
        drop(database);
        let child = Command::new(std::env::current_exe()?)
            .args(["--exact", "service::migration::recovery::tests::hot_journal_is_recovered_privately_without_changing_source"])
            .env("SESSION_BOARD_HOT_JOURNAL_FIXTURE", source)
            .stdout(std::process::Stdio::null())
            .status()?;
        assert!(child.success());
        Ok(())
    }

    fn verify_hot_journal(source: &std::path::Path) -> Result<()> {
        let read_only = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let failure = read_only
            .query_row("SELECT count(*) FROM records", [], |row| {
                row.get::<_, i64>(0)
            })
            .err()
            .context("夹具未产生热回滚日志")?;
        assert!(matches!(failure, rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_READONLY_ROLLBACK));
        Ok(())
    }
}
