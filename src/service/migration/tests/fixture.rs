use crate::config::Config;
use crate::store::Store;
use anyhow::Result;
use rusqlite::{Connection, OpenFlags, params, types::Value as SqlValue};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

pub(super) type Contents = Vec<Vec<Vec<SqlValue>>>;

pub(super) struct Fixture {
    pub(super) directory: tempfile::TempDir,
    pub(super) source: PathBuf,
    pub(super) config: Config,
    pub(super) stores: Vec<Store>,
}

pub(super) fn directories(root: &Path) -> [PathBuf; 3] {
    let mut hosts =
        ["remote:a", "remote:b"].map(|host| format!("{:x}", Sha256::digest(host.as_bytes())));
    hosts.sort();
    [
        root.into(),
        root.join("remote-hosts").join(&hosts[0]),
        root.join("remote-hosts").join(&hosts[1]),
    ]
}

fn seed_sessions(database: &Connection, index: usize) -> Result<()> {
    database.execute(
        "INSERT INTO sessions
         (id,title,cwd,updated_at,seen_at,note,selected_state,version,input_revision,
          selection_turn,selection_users,selected_at,needs_review,native_state,
          native_section,runtime_pending_key)
         VALUES ('active','标题','/project',9,10,?,'to_archive',12,7,
          'turn','[\"first\",\"latest\"]',8,1,'to_archive','section-version','pending-phase')",
        [format!("主机 {index} 的最新备注")],
    )?;
    database.execute(
        "INSERT INTO sessions
         (id,title,cwd,updated_at,seen_at,note,selected_state,version,input_revision,archived)
         VALUES ('archived','已归档','/project',5,6,?,'to_archive',20,11,1)",
        [format!("主机 {index} 的归档墓碑")],
    )?;
    Ok(())
}

pub(super) fn populate(directory: &Path, index: usize) -> Result<Store> {
    let store = Store::open(directory, true)?;
    {
        let database = store.connection()?;
        database.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")?;
        seed_sessions(&database, index)?;
        database.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES ('active',7,'review',?)",
            [format!("主机 {index} 的历史备注")],
        )?;
        database.execute(
            "INSERT INTO title_updates(thread_id,version,state,requested_at,error)
             VALUES ('active',12,'to_archive',9,?)",
            [format!("主机 {index} 的标题待重试")],
        )?;
        database.execute(
            "INSERT INTO settings(key,value) VALUES ('legacy_remote_import',?)",
            params!["1"],
        )?;
    }
    Ok(store)
}

fn contents(database: &Connection) -> Result<Contents> {
    let mut tables = Vec::new();
    for table in [
        "sessions",
        "changes",
        "title_updates",
        "settings",
        "remote_sessions",
        "remote_projects",
    ] {
        let mut query = database.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
        let count = query.column_count();
        let rows = query
            .query_map([], |row| {
                (0..count)
                    .map(|column| row.get(column))
                    .collect::<rusqlite::Result<Vec<SqlValue>>>()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tables.push(rows);
    }
    Ok(tables)
}

pub(super) fn stored(directory: &Path) -> Result<Contents> {
    let database = Connection::open_with_flags(
        directory.join("status.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    contents(&database)
}

impl Fixture {
    pub(super) fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        let stores = directories(&source)
            .iter()
            .enumerate()
            .map(|(index, path)| populate(path, index))
            .collect::<Result<Vec<_>>>()?;
        for (index, host) in directories(&source).iter().enumerate().skip(1) {
            fs::write(
                host.join("source.json"),
                json!({"hostId":index,"command":["original"]}).to_string(),
            )?;
            fs::write(
                host.join("scan-progress.json"),
                json!({"cursor":"next","identifiers":["active"]}).to_string(),
            )?;
        }
        let config = Config {
            data_dir: directory.path().join("destination"),
            ..Config::default()
        };
        Ok(Self {
            directory,
            source,
            config,
            stores,
        })
    }

    pub(super) fn configuration(&self) -> PathBuf {
        self.directory.path().join("published/config.json")
    }

    pub(super) fn publish(&self) -> Result<bool> {
        super::super::publish(
            &self.configuration(),
            &self.config,
            &self.source.join("status.sqlite3"),
        )
    }

    pub(super) fn assert_preserved(&self) -> Result<()> {
        for ((source, target), store) in directories(&self.source)
            .iter()
            .zip(directories(&self.config.data_dir))
            .zip(&self.stores)
        {
            let database = store.connection()?;
            let expected = contents(&database)?;
            drop(database);
            assert_eq!(stored(source)?, expected);
            assert_eq!(stored(&target)?, expected);
            assert_eq!(
                fs::metadata(target.join("status.sqlite3"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert!(!target.join("status.sqlite3-wal").exists());
        }
        Ok(())
    }
}

pub(super) fn runtime_files(directory: &Path) -> Result<()> {
    for name in [
        "server.json",
        "launch.lock",
        "server.log",
        "credentials.json",
    ] {
        fs::write(directory.join(name), "must remain at source")?;
    }
    let socket = tempfile::tempdir()?;
    let path = socket.path().join("socket");
    drop(UnixListener::bind(&path)?);
    fs::rename(path, directory.join("desktop.sock"))?;
    Ok(())
}
