use super::*;

fn placed(state: &str) -> Observation {
    Observation {
        state: state.into(),
        version: format!("{state}-id"),
    }
}

fn fixture() -> Result<(tempfile::TempDir, Store)> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.discover(&[json!({"id":FIRST})])?;
    store.set_state(FIRST, Some("unknown"), "备注不变", 0, Some(0))?;
    store.native_sections = true;
    store.cache_native(
        FIRST,
        &Observation {
            state: "review".into(),
            version: "review-id".into(),
        },
    )?;
    Ok((directory, store))
}

#[test]
fn new_stages_apply_once_and_external_moves_survive_repeated_observations() -> Result<()> {
    let (_directory, store) = fixture()?;
    let since = codex_session_board::model::now();
    let active = json!({"type":"active","activeFlags":[]});
    let turn = json!({"id":"turn","status":"inProgress","startedAt":since+1.0});
    let change = store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .unwrap();
    store.commit_native_runtime(FIRST, &change, &placed("in_progress"))?;
    assert!(
        store
            .prepare_native_runtime(FIRST, &active, &turn, since)?
            .is_none()
    );
    store.cache_native(FIRST, &placed("to_archive"))?;
    assert!(
        store
            .prepare_native_runtime(FIRST, &active, &turn, since)?
            .is_none()
    );
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    let done =
        json!({"id":"turn","status":"completed","startedAt":since+1.0,"completedAt":since+2.0});
    let change = store
        .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, since)?
        .unwrap();
    store.commit_native_runtime(FIRST, &change, &placed("review"))?;
    assert!(
        store
            .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, since)?
            .is_none()
    );
    assert_eq!(store.note(FIRST)?, "备注不变");
    Ok(())
}

#[test]
fn failed_stage_is_consumed_when_the_native_position_changes() -> Result<()> {
    let (_directory, store) = fixture()?;
    let since = codex_session_board::model::now();
    let active = json!({"type":"active","activeFlags":[]});
    let turn = json!({"id":"turn","status":"inProgress","startedAt":since+1.0});
    let pending = store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .unwrap();
    assert!(
        store
            .prepare_native_runtime(FIRST, &active, &turn, since)?
            .is_some()
    );
    store.cache_native(
        FIRST,
        &Observation {
            state: "to_archive".into(),
            version: "archive-id".into(),
        },
    )?;
    assert!(
        store
            .prepare_native_runtime(FIRST, &active, &turn, since)?
            .is_none()
    );
    assert!(
        store
            .commit_native_runtime(
                FIRST,
                &pending,
                &Observation {
                    state: "in_progress".into(),
                    version: "active-id".into()
                }
            )
            .is_err()
    );
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    assert_eq!(store.note(FIRST)?, "备注不变");
    Ok(())
}

#[test]
fn baseline_history_never_overwrites_an_existing_native_selection() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.native_sections = true;
    store.discover(&[json!({"id":FIRST})])?;
    store.cache_native(
        FIRST,
        &Observation {
            state: "to_archive".into(),
            version: "archive-id".into(),
        },
    )?;
    let done = json!({"id":"old-turn","status":"completed","startedAt":1,"completedAt":2});
    assert!(
        store
            .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, 3.0)?
            .is_none()
    );
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    Ok(())
}
