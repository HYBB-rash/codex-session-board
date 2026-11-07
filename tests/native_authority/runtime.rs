use super::*;

fn placed(state: &str) -> Observation {
    Observation {
        state: state.into(),
        version: format!("{state}-id"),
    }
}

fn late_input_requires_review(store: &Store, mut turn: serde_json::Value) -> Result<()> {
    turn["userItems"] = json!(["new-user-input"]);
    store.observe(FIRST, &turn, None)?;
    assert_eq!(store.sessions()?[0]["followUp"], true);
    Ok(())
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

fn short_automatic_turn() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.native_sections = true;
    store.discover(&[json!({"id":FIRST})])?;
    store.cache_native(FIRST, &placed("review"))?;
    store
        .connection()?
        .execute("UPDATE sessions SET native_choice_at=100.5", [])?;
    let active = json!({"type":"active","activeFlags":[]});
    let turn = json!({"id":"short","status":"inProgress","startedAt":99});
    let change = store
        .prepare_native_runtime(FIRST, &active, &turn, 98.0)?
        .unwrap();
    store.commit_native_runtime(FIRST, &change, &placed("in_progress"))?;
    let done = json!({"id":"short","status":"completed","startedAt":99,"completedAt":100});
    let change = store
        .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, 98.0)?
        .unwrap();
    store.commit_native_runtime(FIRST, &change, &placed("review"))?;
    assert_eq!(store.sessions()?[0]["state"], "review");
    Ok(())
}

#[test]
fn new_stages_apply_once_and_external_moves_survive_repeated_observations() -> Result<()> {
    short_automatic_turn()?;
    let (_directory, store) = fixture()?;
    let since = codex_session_board::model::now();
    let active = json!({"type":"active","activeFlags":[]});
    let turn = json!({"id":"turn","status":"inProgress","startedAt":since+1.0});
    let change = store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .unwrap();
    store.commit_native_runtime(FIRST, &change, &placed("in_progress"))?;
    assert!(store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .is_none());
    store.cache_native(FIRST, &placed("to_archive"))?;
    assert!(store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .is_none());
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    missing_terminal_time_preserves_choice(&store)?;
    let done =
        json!({"id":"turn","status":"completed","startedAt":since+1.0,"completedAt":since+2.0});
    let change = store
        .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, since)?
        .unwrap();
    store.commit_native_runtime(FIRST, &change, &placed("review"))?;
    assert!(store
        .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, since)?
        .is_none());
    assert_eq!(store.note(FIRST)?, "备注不变");
    late_input_requires_review(&store, done)?;
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
    assert!(store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .is_some());
    store.cache_native(
        FIRST,
        &Observation {
            state: "to_archive".into(),
            version: "archive-id".into(),
        },
    )?;
    assert!(store
        .prepare_native_runtime(FIRST, &active, &turn, since)?
        .is_none());
    assert!(store
        .commit_native_runtime(
            FIRST,
            &pending,
            &Observation {
                state: "in_progress".into(),
                version: "active-id".into()
            }
        )
        .is_err());
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    assert_eq!(store.note(FIRST)?, "备注不变");
    Ok(())
}

#[test]
fn baseline_history_never_overwrites_an_existing_native_selection() -> Result<()> {
    input_before_native_refresh_still_requires_confirmation()?;
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), true)?;
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
    assert!(store.pending_titles(None)?.is_empty());
    assert!(store
        .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, 3.0)?
        .is_none());
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    store.connection()?.execute(
        "UPDATE sessions SET runtime_key='older-active-phase' WHERE id=?",
        [FIRST],
    )?;
    assert!(store
        .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, 3.0)?
        .is_none());
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    store.observe(
        FIRST,
        &json!({"id":"new-input","status":"completed","userItems":["late-input"]}),
        None,
    )?;
    assert_eq!(store.sessions()?[0]["followUp"], true);
    Ok(())
}

fn missing_terminal_time_preserves_choice(store: &Store) -> Result<()> {
    for completion in [None, Some(serde_json::Value::Null)] {
        store.connection()?.execute(
            "UPDATE sessions SET runtime_key='older-active-phase' WHERE id=?",
            [FIRST],
        )?;
        let mut done = json!({"id":"old-turn","status":"completed","startedAt":1});
        if let Some(completion) = completion {
            done["completedAt"] = completion;
        }
        assert!(store
            .prepare_native_runtime(FIRST, &json!({"type":"idle"}), &done, 3.0)?
            .is_none());
        assert_eq!(store.sessions()?[0]["state"], "to_archive");
    }
    Ok(())
}

fn input_before_native_refresh_still_requires_confirmation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    let mut native = NativeSurface {
        section: "review-id",
        fail: false,
    };
    app.store
        .observe(FIRST, &json!({"id":"turn","userItems":["u1"]}), None)?;
    app.read_native(&mut native)?;
    native.section = "archive-id";
    let input = json!({"id":"turn","userItems":["u1","u2"]});
    app.store.observe(FIRST, &input, None)?;
    assert_eq!(app.store.sessions()?[0]["followUp"], false);
    app.read_native(&mut native)?;
    app.store.observe(FIRST, &input, None)?;
    let row = &app.store.sessions()?[0];
    assert_eq!(row["followUp"], true);
    assert!(app
        .store
        .prepare_archive(
            FIRST,
            (
                row["version"].as_i64().unwrap(),
                row["inputVersion"].as_i64().unwrap()
            )
        )
        .unwrap_err()
        .to_string()
        .contains("新输入"));
    let restarted = native_app(directory.path())?;
    assert_eq!(restarted.store.sessions()?[0]["followUp"], true);
    Ok(())
}
