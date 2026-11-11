use super::fixture::{ID, load};
use super::records::{observe, position, rejected, revisions, select, snapshot, start};
use crate::store::Store;
use anyhow::Result;
use serde_json::json;

fn reject_archive(store: &Store, version: (i64, i64), reason: &str) -> Result<()> {
    rejected(store, reason, || store.prepare_archive(ID, version))?;
    rejected(store, reason, || store.confirm_archive(ID, version))
}

#[test]
fn archive_guards_independently_reject_versions_review_and_wrong_columns() -> Result<()> {
    let store = start()?;
    store.cache_native(ID, &position("review"))?;
    reject_archive(&store, revisions(&store)?, "只能归档待归档会话")?;
    select(&store, "to_archive")?;
    let original = revisions(&store)?;
    select(&store, "to_archive")?;
    reject_archive(&store, original, "状态已更新，请刷新后重试")?;
    observe(&store, "new", &["u1"])?;
    select(&store, "to_archive")?;
    assert_eq!(load(&store)?.review, 0);
    let input_conflict = "会话有新输入，请刷新并核对后重试";
    reject_archive(&store, (revisions(&store)?.0, original.1), input_conflict)?;
    observe(&store, "new", &["u1", "u2"])?;
    let needs_review = "会话有新输入，请核对后点击卡片上的“确认待归档”";
    reject_archive(&store, revisions(&store)?, needs_review)?;
    select(&store, "to_archive")?;
    store.confirm_archive(ID, revisions(&store)?)?;
    reject_archive(&store, revisions(&store)?, "会话已归档，请刷新看板")
}

#[test]
fn archive_rechecks_prepared_versions_and_commits_only_fresh_confirmation() -> Result<()> {
    let store = start()?;
    store.cache_native(ID, &position("review"))?;
    let expected = observe(&store, "new", &["u1"])?;
    select(&store, "to_archive")?;
    let prepared = revisions(&store)?;
    let before = snapshot(&store)?;
    assert_eq!(store.prepare_archive(ID, prepared)?, expected);
    assert_eq!(snapshot(&store)?, before);
    select(&store, "to_archive")?;
    rejected(&store, "状态已更新，请刷新后重试", || store.confirm_archive(ID, prepared))?;
    let prepared = revisions(&store)?;
    assert_eq!(store.prepare_archive(ID, prepared)?, expected);
    observe(&store, "new", &["u1", "u2"])?;
    rejected(&store, "会话有新输入，请刷新并核对后重试", || store.confirm_archive(ID, prepared))?;
    select(&store, "to_archive")?;
    let active = json!({"type":"active"});
    let running = json!({"id":"pending","status":"inProgress","startedAt":1.0e12});
    assert!(store.prepare_native_runtime(ID, &active, &running, 0.0)?.is_some());
    let version = revisions(&store)?;
    let before = snapshot(&store)?;
    store.confirm_archive(ID, version)?;
    let row = load(&store)?;
    assert_eq!(revisions(&store)?, (version.0 + 1, version.1));
    assert_eq!(row.archived, 1);
    assert!(row.pending_key.is_none());
    assert_eq!(store.note(ID)?, "保留备注");
    let after = snapshot(&store)?;
    assert_eq!(after.tables["changes"], before.tables["changes"]);
    assert!(after.tables["title_updates"].is_empty());
    assert!(store.sessions()?.is_empty());
    reject_archive(&store, revisions(&store)?, "会话已归档，请刷新看板")
}
