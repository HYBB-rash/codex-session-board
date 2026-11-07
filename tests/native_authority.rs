use anyhow::Result;
use codex_session_board::{native_sections::Observation, store::Store};
use serde_json::json;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
#[path = "native_authority/runtime.rs"]
mod runtime;

#[test]
fn native_cache_wins_over_local_choices_without_erasing_notes_or_confirmation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.discover(&[json!({"id":FIRST})])?;
    store.set_state(FIRST, Some("to_archive"), "保留备注", 0, Some(0))?;
    store.native_sections = true;
    assert_eq!(store.sessions()?[0]["state"], "unknown");
    store.cache_native(
        FIRST,
        &Observation {
            state: "review".into(),
            version: "native-review".into(),
        },
    )?;
    let session = &store.sessions()?[0];
    assert_eq!(session["state"], "review");
    assert_eq!(session["note"], "保留备注");
    assert_eq!(session["manual"], true);
    store.cache_native(
        FIRST,
        &Observation {
            state: "blocked".into(),
            version: "native-blocked".into(),
        },
    )?;
    drop(store);
    let mut reopened = Store::open(directory.path(), false)?;
    reopened.native_sections = true;
    assert_eq!(reopened.sessions()?[0]["state"], "blocked");
    assert_eq!(reopened.note(FIRST)?, "保留备注");
    Ok(())
}

#[test]
fn native_confirmations_commit_notes_and_reject_stale_or_invalid_edits() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.native_sections = true;
    store.discover(&[json!({"id":FIRST})])?;
    let before = Observation {
        state: "review".into(),
        version: "native-review".into(),
    };
    store.cache_native(FIRST, &before)?;
    assert!(
        store
            .validate_state(FIRST, Some("blocked"), "", (1, 0))
            .is_err()
    );
    assert!(
        store
            .validate_state(FIRST, Some("review"), "备注", (0, 0))
            .is_err()
    );
    assert!(
        store
            .validate_state(FIRST, Some("review"), "备注", (1, 1))
            .is_err()
    );
    let after = Observation {
        state: "to_archive".into(),
        version: "native-archive".into(),
    };
    store.set_native_state(FIRST, Some("to_archive"), "已确认", (1, 0), &after)?;
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    assert_eq!(store.sessions()?[0]["sectionVersion"], "native-archive");
    assert_eq!(store.note(FIRST)?, "已确认");
    assert!(
        store
            .set_native_state(FIRST, Some("review"), "过期", (1, 0), &before)
            .is_err()
    );
    assert_eq!(store.note(FIRST)?, "已确认");
    Ok(())
}
