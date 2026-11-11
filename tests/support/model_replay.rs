use super::model_database::{self, Rows};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const TABLES: [&str; 6] = [
    "title_updates", "changes", "remote_sessions", "remote_projects", "sessions", "settings",
];

pub struct TraceDatabase {
    path: PathBuf,
    schema: Rows,
}

impl TraceDatabase {
    pub fn capture(database: &Connection, root: &Path) -> Result<Self> {
        let path = PathBuf::from(database.path().context("重放需要磁盘数据库")?).canonicalize()?;
        ensure!(path.starts_with(root.canonicalize()?), "数据库不在本次临时目录中");
        let initial = model_database::snapshot(database)?;
        let actual: BTreeSet<_> = initial.tables.keys().map(String::as_str).collect();
        ensure!(actual == TABLES.into_iter().collect(), "重放数据库出现未声明的表");
        Ok(Self { path, schema: initial.schema })
    }

    pub fn reset(&self) -> Result<()> {
        let mut database = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        ensure!(model_database::schema(&database)? == self.schema, "重放改变了schema");
        let transaction = database.transaction()?;
        for table in TABLES {
            transaction.execute(&format!("DELETE FROM {table}"), [])?;
        }
        transaction.commit()?;
        drop(database);
        let database = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let cleared = model_database::snapshot(&database)?;
        ensure!(cleared.schema == self.schema, "清理改变了schema");
        ensure!(cleared.tables.values().all(Vec::is_empty), "重放数据库仍有上条路径的数据");
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct ReplayCounts {
    pub traces: usize,
    pub steps: usize,
    pub restarts: usize,
    pub views: usize,
}

impl ReplayCounts {
    pub fn record(&mut self, event: &str) {
        self.steps += 1;
        self.restarts += usize::from(event == "restart");
        self.views += usize::from(event == "view");
    }
}

impl std::ops::AddAssign for ReplayCounts {
    fn add_assign(&mut self, other: Self) {
        self.traces += other.traces;
        self.steps += other.steps;
        self.restarts += other.restarts;
        self.views += other.views;
    }
}
