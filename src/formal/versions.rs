use super::fixture::{ID, load};
use super::records::{observe, position, rejected, revisions, select, snapshot, start};
use crate::store::Store;
use anyhow::Result;
use serde_json::json;

#[test]
fn position_and_manual_writes_increment_versions_without_false_conflicts() -> Result<()> {
    let store = start()?;
    assert_eq!(revisions(&store)?, (0, 0));
    store.cache_native(ID, &position("review"))?;
    assert_eq!(revisions(&store)?, (1, 0));
    let before = snapshot(&store)?;
    store.cache_native(ID, &position("review"))?;
    assert_eq!(snapshot(&store)?, before);
    let mut moved = position("review");
    moved.version = "another-review-section".into();
    store.cache_native(ID, &moved)?;
    assert_eq!(revisions(&store)?, (2, 0));
    store.cache_native(ID, &position("to_archive"))?;
    assert_eq!(revisions(&store)?, (3, 0));
    select(&store, "to_archive")?;
    assert_eq!(revisions(&store)?, (4, 0));
    select(&store, "to_archive")?;
    assert_eq!(revisions(&store)?, (5, 0));
    assert_eq!(store.note(ID)?, "保留备注");
    Ok(())
}

#[test]
fn input_revisions_count_new_batches_and_preserve_confirmation() -> Result<()> {
    let store = start()?;
    store.cache_native(ID, &position("review"))?;
    for (turn, users, revision) in [
        ("first", vec!["u1"], 1), ("first", vec!["u1"], 1),
        ("first", vec!["u1", "u2", "u3"], 2),
        ("first", vec!["u3", "u1", "u2"], 2),
        ("second", vec!["u1", "u2", "u3"], 3), ("empty", vec![], 3),
    ] {
        observe(&store, turn, &users)?;
        assert_eq!(revisions(&store)?, (1, revision));
    }
    select(&store, "review")?;
    observe(&store, "empty", &["u4"])?;
    assert_eq!(revisions(&store)?, (2, 4));
    assert_eq!(load(&store)?.review, 1);
    observe(&store, "empty", &["u4"])?;
    assert_eq!(revisions(&store)?, (2, 4));
    select(&store, "review")?;
    assert_eq!(load(&store)?.review, 0);
    assert_eq!(revisions(&store)?, (3, 4));
    Ok(())
}

#[test]
fn stale_edits_reject_each_revision_without_persistent_side_effects() -> Result<()> {
    let store = start()?;
    store.cache_native(ID, &position("review"))?;
    let original = revisions(&store)?;
    select(&store, "to_archive")?;
    reject_edit(&store, original, "状态已更新")?;
    let before_input = revisions(&store)?;
    observe(&store, "new", &["u1"])?;
    reject_edit(&store, before_input, "新输入")?;
    select(&store, "to_archive")?;
    assert_eq!(load(&store)?.review, 0);
    reject_edit(&store, (revisions(&store)?.0, before_input.1), "新输入")?;
    select(&store, "review")?;
    assert_eq!(load(&store)?.native_state.as_deref(), Some("review"));
    Ok(())
}

fn reject_edit(store: &Store, version: (i64, i64), reason: &str) -> Result<()> {
    rejected(store, reason, || store.validate_state(ID, Some("review"), "新备注", version))?;
    rejected(store, reason, || {
        store.set_native_state(ID, Some("review"), "新备注", version, &position("review"))
    })
}

#[test]
fn runtime_results_and_same_column_stages_preserve_revision_contracts() -> Result<()> {
    let store = start()?;
    store.cache_native(ID, &position("review"))?;
    select(&store, "review")?;
    for (kind, phase, flags, state) in [
        ("active", "inProgress", vec![], "in_progress"),
        ("active", "inProgress", vec!["waitingOnApproval"], "blocked"),
        ("active", "inProgress", vec!["waitingOnUserInput"], "blocked"),
        ("systemError", "inProgress", vec![], "blocked"),
        ("idle", "completed", vec![], "review"),
    ] {
        runtime_step(&store, kind, phase, &flags, state)?;
    }
    Ok(())
}

fn runtime_step(store: &Store, kind: &str, phase: &str, flags: &[&str], state: &str) -> Result<()> {
    let status = json!({"type":kind,"activeFlags":flags});
    let body = json!({"id":"same-turn","status":phase,"startedAt":1.0e12,"completedAt":1.0e12});
    let before = load(store)?;
    let change = store.prepare_native_runtime(ID, &status, &body, 0.0)?.unwrap();
    assert_eq!(change.state, state);
    assert_eq!(change.version, before.version);
    assert_eq!(Some(&change.section_version), before.native_section.as_ref());
    assert_eq!(revisions(store)?, (before.version, before.input_revision));
    store.commit_native_runtime(ID, &change, &position(state))?;
    assert_eq!(revisions(store)?, (before.version + 1, before.input_revision));
    assert_eq!(store.note(ID)?, "保留备注");
    let after = snapshot(store)?;
    assert!(store.prepare_native_runtime(ID, &status, &body, 0.0)?.is_none());
    assert_eq!(snapshot(store)?, after);
    Ok(())
}
