use super::check::rejection;
use super::fixture::{ID, load};
use crate::native_sections::Observation;
use crate::store::Store;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub(super) fn snapshot(store: &Store) -> Result<super::model_database::Snapshot> {
    let database = store.connection()?;
    Ok(super::model_database::snapshot(&database)?)
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
    rejection(operation(), reason)?;
    ensure!(snapshot(store)? == before, "拒绝操作改变了持久数据: {reason}");
    Ok(())
}
