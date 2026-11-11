use super::fixture::{ID, load};
use crate::native_sections::Observation;
use crate::store::Store;
use anyhow::{Result, bail, ensure};
use rusqlite::types::Value as SqlValue;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) type DatabaseSnapshot = BTreeMap<&'static str, Vec<Vec<SqlValue>>>;

pub(super) fn snapshot(store: &Store) -> Result<DatabaseSnapshot> {
    let database = store.connection()?;
    ["sessions", "changes", "title_updates", "settings", "remote_sessions", "remote_projects"]
        .into_iter()
        .map(|table| {
            let mut query = database.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
            let count = query.column_count();
            let rows = query.query_map([], |row| (0..count).map(|index| row.get(index)).collect())?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((table, rows))
        })
        .collect::<rusqlite::Result<_>>().map_err(Into::into)
}

pub(super) fn start() -> Result<Store> {
    let mut store = Store::open_memory(true)?;
    store.native_sections = true;
    store.discover(&[json!({"id":ID,"name":"原生契约","updatedAt":1})])?;
    Ok(store)
}

pub(super) fn position(state: &str) -> Observation {
    Observation { state: state.into(), version: format!("{state}-id") }
}

pub(super) fn revisions(store: &Store) -> Result<(i64, i64)> {
    let row = load(store)?;
    Ok((row.version, row.input_revision))
}

pub(super) fn select(store: &Store, state: &str) -> Result<()> {
    store.set_native_state(ID, Some(state), "保留备注", revisions(store)?, &position(state))
}

pub(super) fn observe(store: &Store, turn: &str, users: &[&str]) -> Result<Value> {
    let body = json!({"id":turn,"status":"completed","startedAt":1.0e12,"userItems":users});
    store.observe(ID, &body, None)?;
    Ok(body)
}

pub(super) fn rejected<Returned>(
    store: &Store, reason: &str, operation: impl FnOnce() -> Result<Returned>,
) -> Result<()> {
    let before = snapshot(store)?;
    match operation() {
        Ok(_) => bail!("应拒绝的操作返回成功: {reason}"),
        Err(error) => ensure!(error.to_string().contains(reason), "错误原因不匹配: {error}"),
    }
    ensure!(snapshot(store)? == before, "拒绝操作改变了持久数据: {reason}");
    Ok(())
}
