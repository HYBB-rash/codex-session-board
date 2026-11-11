use super::fixture::{ID, load};
use super::records::{observe, position, revisions, select, snapshot};
use crate::store::Store;
use anyhow::Result;
use serde_json::json;
use std::path::Path;

fn open(directory: &Path) -> Result<Store> {
    let mut store = Store::open(directory, true)?;
    store.native_sections = true;
    assert!(store.title_epoch.is_some());
    Ok(store)
}

fn reopen(store: Store, directory: &Path) -> Result<Store> {
    let before = snapshot(&store)?;
    let epoch = store.title_epoch;
    super::model_database::mark_restart(&*store.connection()?)?;
    drop(store);
    let store = open(directory)?;
    assert!(!super::model_database::restart_marker_exists(&*store.connection()?)?);
    assert_eq!(store.title_epoch, epoch);
    assert_eq!(snapshot(&store)?, before);
    Ok(store)
}

#[test]
fn native_manual_input_pending_and_title_failure_survive_real_reopen() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = open(directory.path())?;
    store.discover(&[json!({"id":ID,"name":"持久化","updatedAt":1})])?;
    store.cache_native(ID, &position("review"))?;
    observe(&store, "history", &["u1"])?;
    select(&store, "to_archive")?;
    fail_title(&store)?;
    let store = reopen(store, directory.path())?;
    assert_eq!(load(&store)?.review, 0);
    observe(&store, "history", &["u1", "u2"])?;
    let before_empty = revisions(&store)?;
    observe(&store, "empty", &[])?;
    assert_eq!(revisions(&store)?, before_empty);
    assert_eq!(load(&store)?.review, 1);
    let status = json!({"type":"active"});
    let turn = json!({"id":"pending","status":"inProgress","startedAt":1.0e12});
    assert!(store.prepare_native_runtime(ID, &status, &turn, 0.0)?.is_some());
    let store = reopen(store, directory.path())?;
    assert_eq!(load(&store)?.review, 1);
    let version = revisions(&store)?;
    let change = store.prepare_native_runtime(ID, &status, &turn, 1.0e12 + 1.0)?.unwrap();
    assert_eq!(change.state, "in_progress");
    store.commit_native_runtime(ID, &change, &position("in_progress"))?;
    assert_eq!(revisions(&store)?, (version.0 + 1, version.1));
    assert_eq!(load(&store)?.review, 1);
    assert_eq!(store.note(ID)?, "保留备注");
    assert!(!store.has_pending_native_runtime(ID)?);
    let store = reopen(store, directory.path())?;
    let before = snapshot(&store)?;
    assert!(store.prepare_native_runtime(ID, &status, &turn, 1.0e12 + 1.0)?.is_none());
    assert_eq!(snapshot(&store)?, before);
    archive_restore(store, directory.path())
}

fn fail_title(store: &Store) -> Result<()> {
    let version = revisions(store)?.0;
    store.title_failed(ID, version, "暂时断开")?;
    let failure: (i64, Option<String>, bool) = store.connection()?.query_row(
        "SELECT version,error,attempted_at IS NOT NULL FROM title_updates WHERE thread_id=?",
        [ID], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(failure, (version, Some("暂时断开".into()), true));
    Ok(())
}

fn archive_restore(store: Store, directory: &Path) -> Result<()> {
    select(&store, "to_archive")?;
    let version = revisions(&store)?;
    store.confirm_archive(ID, version)?;
    assert_eq!(revisions(&store)?, (version.0 + 1, version.1));
    let store = reopen(store, directory)?;
    assert_eq!(store.archived_ids()?, vec![ID]);
    assert!(store.sessions()?.is_empty());
    store.restore_native(ID, &position("review"))?;
    assert_eq!(revisions(&store)?, (version.0 + 2, version.1));
    assert_eq!(load(&store)?.archived, 0);
    assert_eq!(store.note(ID)?, "保留备注");
    assert_eq!(store.sessions()?.len(), 1);
    let store = reopen(store, directory)?;
    let before = snapshot(&store)?;
    store.restore_native(ID, &position("review"))?;
    assert_eq!(snapshot(&store)?, before);
    Ok(())
}
