use super::fixture::{ID, load};
use super::records::{position, revisions, select, snapshot, start};
use crate::store::Store;
use anyhow::Result;
use serde_json::{Value, json};

fn running(store: &Store, identifier: &str, time: f64, since: f64) -> Result<()> {
    let status = json!({"type":"active"});
    let turn = json!({"id":identifier,"status":"inProgress","startedAt":time});
    let change = store.prepare_native_runtime(ID, &status, &turn, since)?.unwrap();
    store.commit_native_runtime(ID, &change, &position("in_progress"))
}

#[test]
fn first_phase_distinguishes_older_equal_and_newer_observation_times() -> Result<()> {
    for offset in [-1.0, 0.0, 1.0] {
        let store = start()?;
        store.cache_native(ID, &position("review"))?;
        let status = json!({"type":"active"});
        let turn = json!({"id":"first","status":"inProgress","startedAt":100.0 + offset});
        let change = store.prepare_native_runtime(ID, &status, &turn, 100.0)?;
        assert_eq!(change.is_some(), offset >= 0.0);
        if let Some(change) = change {
            assert_eq!(change.state, "in_progress");
            store.commit_native_runtime(ID, &change, &position("in_progress"))?;
        }
        let row = load(&store)?;
        assert_eq!(row.native_state.as_deref(), Some(if offset < 0.0 { "review" } else { "in_progress" }));
        assert_eq!(revisions(&store)?, (1 + i64::from(offset >= 0.0), 0));
        assert!(row.runtime_key.is_some() && row.pending_key.is_none());
        let before = snapshot(&store)?;
        assert!(store.prepare_native_runtime(ID, &status, &turn, 100.0)?.is_none());
        assert_eq!(snapshot(&store)?, before);
    }
    Ok(())
}

#[test]
fn terminal_completion_time_respects_both_sides_and_equality_of_manual_choice() -> Result<()> {
    for (outcome, state) in [("completed", "review"), ("failed", "blocked"), ("interrupted", "blocked")] {
        for offset in [-1.0, 0.0, 1.0] {
            let store = start()?;
            store.cache_native(ID, &position("review"))?;
            running(&store, "same-turn", 1.0, 0.0)?;
            select(&store, "to_archive")?;
            let chosen = load(&store)?.choice_at.unwrap();
            let version = revisions(&store)?;
            let turn = json!({"id":"same-turn","status":outcome,"startedAt":1.0,"completedAt":chosen + offset});
            let status = json!({"type":"idle"});
            terminal(&store, &status, &turn, (offset > 0.0).then_some(state))?;
            let row = load(&store)?;
            assert_eq!(revisions(&store)?, (version.0 + i64::from(offset > 0.0), version.1));
            assert_eq!(row.native_state.as_deref(), Some(if offset > 0.0 { state } else { "to_archive" }));
            assert_eq!(store.note(ID)?, "保留备注");
            let before = snapshot(&store)?;
            assert!(store.prepare_native_runtime(ID, &status, &turn, 0.0)?.is_none());
            assert_eq!(snapshot(&store)?, before);
        }
    }
    Ok(())
}

fn terminal(store: &Store, status: &Value, turn: &Value, state: Option<&str>) -> Result<()> {
    let change = store.prepare_native_runtime(ID, status, turn, 0.0)?;
    assert_eq!(change.as_ref().map(|change| change.state), state);
    if let Some(change) = change {
        store.commit_native_runtime(ID, &change, &position(state.unwrap()))?;
    }
    Ok(())
}

#[test]
fn pending_turn_retry_bypasses_old_observation_and_manual_choice_boundaries() -> Result<()> {
    for confirmed in [false, true] {
        retry_before_both_boundaries(confirmed)?;
    }
    Ok(())
}

fn retry_before_both_boundaries(confirmed: bool) -> Result<()> {
    let store = start()?;
    store.cache_native(ID, &position("review"))?;
    if confirmed {
        running(&store, "confirmed", 1.0, 0.0)?;
    }
    select(&store, "to_archive")?;
    let chosen = load(&store)?.choice_at.unwrap();
    let status = json!({"type":"active"});
    let turn = json!({"id":"pending","status":"inProgress","startedAt":chosen - 2.0});
    assert!(store.prepare_native_runtime(ID, &status, &turn, chosen - 3.0)?.is_some());
    let version = revisions(&store)?;
    let turn = json!({"id":"pending","status":"completed","startedAt":chosen - 2.0,"completedAt":chosen - 1.0});
    let change = store.prepare_native_runtime(ID, &json!({"type":"idle"}), &turn, chosen + 1.0)?.unwrap();
    assert_eq!(change.state, "review");
    store.commit_native_runtime(ID, &change, &position("review"))?;
    assert_eq!(revisions(&store)?, (version.0 + 1, version.1));
    assert!(load(&store)?.choice_at.is_none());
    assert!(!store.has_pending_native_runtime(ID)?);
    assert_eq!(store.note(ID)?, "保留备注");
    Ok(())
}
