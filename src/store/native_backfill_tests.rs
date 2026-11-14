use super::*;

fn setup() -> Result<(Store, Config, String, Value)> {
    let store = Store::open_memory(true)?;
    let config = Config {status_sections:crate::model::board_states()
        .map(|(state,_)| (state.into(),uuid::Uuid::new_v4().to_string())).collect(),
        ..Config::default()};
    let identifier = uuid::Uuid::new_v4().to_string();
    store.discover(&[json!({"id":identifier,"name":"业务会话","updatedAt":10})])?;
    Ok((store,config,identifier,json!({"owner":"account-a","host":"remote:a"})))
}

fn default_position() -> Observation {
    Observation {state:"unknown".into(),version:"unsectioned".into()}
}

fn target(change: &BackfillChange) -> Observation {
    Observation {state:change.state.clone(),version:change.target.clone()}
}

#[test]
fn backfill_preserves_manual_notes_input_and_phase_anchors_without_bulk_title_jobs() -> Result<()> {
    let (store, config, id, scope) = setup()?;
    store.observe(&id, &json!({"id":"old","userItems":["u1"],"status":"completed","hasFinal":true}), None)?;
    store.set_state(&id, Some("blocked"), "原来的阻塞原因", 0, Some(1))?;
    store.observe(&id, &json!({"id":"next","userItems":["u2"],"status":"inProgress"}), None)?;
    store.connection()?.execute("UPDATE sessions SET runtime_key='old-phase',
        runtime_pending_key='[\"old\",\"in_progress\",[]]' WHERE id=?", [&id])?;
    let before = get_row(&*store.connection()?, &id)?;
    assert!(store.cache_native_backfill(&id, &default_position(), &scope)?);
    let change = store.prepare_native_backfill(&id, &config,
        &json!({"type":"active"}), &scope)?.unwrap();
    assert_eq!(change.state, "blocked");
    store.finish_native_backfill(&id, &change, &target(&change))?;
    let after = get_row(&*store.connection()?, &id)?;
    for key in ["selected_state","note","selection_turn","selection_users","selected_at",
        "needs_review","input_revision","runtime_key","runtime_pending_key","native_choice_at"] {
        assert_eq!(before[key], after[key], "{key}");
    }
    assert_eq!(store.pending_titles(None)?.len(), 1);
    assert_eq!(store.pending_titles(None)?[0]["version"], after["version"]);
    assert!(!store.native_backfill_pending(&id)?);
    Ok(())
}

#[test]
fn backfill_new_input_rejects_commit_and_lost_confirmation_settles_without_replay() -> Result<()> {
    let (store, config, id, scope) = setup()?;
    store.cache_native_backfill(&id, &default_position(), &scope)?;
    let change = store.prepare_native_backfill(&id, &config, &json!({}), &scope)?.unwrap();
    store.observe(&id, &json!({"id":"new","userItems":["new-input"],"status":"inProgress"}), None)?;
    assert!(store.finish_native_backfill(&id, &change, &target(&change)).is_err());
    assert!(store.validate_native_backfill(&id, &change).is_err());
    let before = get_row(&*store.connection()?, &id)?;
    store.cache_native_backfill(&id, &target(&change), &scope)?;
    assert!(!store.native_backfill_pending(&id)?);
    let after = get_row(&*store.connection()?, &id)?;
    assert_eq!(before["input_revision"], after["input_revision"]);
    assert_eq!(before["needs_review"], after["needs_review"]);
    assert_eq!(store.pending_titles(None)?.len(), 0);
    Ok(())
}

#[test]
fn backfill_respects_existing_native_custom_pinned_and_managed_positions_once() -> Result<()> {
    for section in ["pinned","custom-section","managed-section"] {
        let (store, config, id, scope) = setup()?;
        let native = Observation {state:"unknown".into(),version:section.into()};
        assert!(store.cache_native_backfill(&id, &native, &scope)?);
        assert!(!store.cache_native_backfill(&id, &default_position(), &json!({"new":"scope"}))?);
        assert!(store.prepare_native_backfill(&id, &config, &json!({}), &scope)?.is_none());
        assert_eq!(store.pending_titles(None)?.len(), 0);
    }
    Ok(())
}

#[test]
fn backfill_uses_live_active_and_guarded_history_without_promoting_old_failures() -> Result<()> {
    for (status, turn, expected) in [
        (json!({"type":"idle"}),json!({"status":"failed"}),"unknown"),
        (json!({"type":"notLoaded"}),json!({"status":"completed","hasFinal":true}),"review"),
        (json!({"type":"idle"}),json!({"status":"completed","hasFinal":false}),"unknown"),
        (json!({"type":"active"}),json!({"status":"inProgress"}),"in_progress"),
        (json!({"type":"active","activeFlags":["waitingOnUserInput"]}),
            json!({"status":"inProgress"}),"blocked"),
    ] {
        let (store, config, id, scope) = setup()?;
        store.observe(&id, &turn, None)?;
        store.cache_native_backfill(&id, &default_position(), &scope)?;
        let change = store.prepare_native_backfill(&id, &config, &status, &scope)?.unwrap();
        assert_eq!(change.state, expected);
        store.finish_native_backfill(&id, &change, &target(&change))?;
        assert!(store.pending_titles(None)?.is_empty());
    }
    Ok(())
}

#[test]
fn backfill_baselines_old_phase_but_allows_a_new_terminal_after_initialization() -> Result<()> {
    let (store, config, id, scope) = setup()?;
    let since = now();
    let turn = json!({"id":"old-turn","status":"completed","completedAt":since-100.0});
    store.connection()?.execute("UPDATE sessions SET runtime_key=?,runtime_pending_key=? WHERE id=?",
        params![json!(["old-turn","in_progress",[]]).to_string(),
            json!(["old-turn","review",[]]).to_string(),id])?;
    let actual = Observation {state:"to_archive".into(),version:"managed-archive".into()};
    store.cache_native_backfill(&id,&actual,&scope)?;
    assert!(store.baseline_native_backfill(&id,&json!({"type":"notLoaded"}),&turn,since)?);
    assert!(store.prepare_native_runtime(&id,&json!({"type":"notLoaded"}),&turn,since)?.is_none());
    assert!(store.pending_titles(None)?.is_empty());
    assert_eq!(get_row(&*store.connection()?,&id)?["native_state"],"to_archive");
    let next = json!({"id":"new-turn","status":"completed","completedAt":since+100.0});
    assert!(!store.baseline_native_backfill(&id,&json!({"type":"idle"}),&next,since)?);
    assert!(store.prepare_native_runtime(&id,&json!({"type":"idle"}),&next,since)?.is_some());
    assert!(store.prepare_native_backfill(&id,&config,&json!({}),&scope)?.is_none());
    Ok(())
}

#[test]
fn backfill_keeps_archive_input_confirmation_with_already_cached_identical_input() -> Result<()> {
    let (mut store, _, id, scope) = setup()?;
    store.native_sections = true;
    let turn = json!({"id":"old","userItems":["u1"],"status":"completed"});
    store.observe(&id,&turn,None)?;
    store.connection()?.execute("UPDATE sessions SET needs_review=0,native_state='to_archive',
        native_section='managed-archive' WHERE id=?", [&id])?;
    let actual = Observation {state:"to_archive".into(),version:"managed-archive".into()};
    store.cache_native_backfill(&id,&actual,&scope)?;
    store.observe(&id,&turn,None)?;
    let row = get_row(&*store.connection()?,&id)?;
    assert_eq!(row["needs_review"],1);
    assert!(store.prepare_archive(&id,(row["version"].as_i64().unwrap(),
        row["input_revision"].as_i64().unwrap())).is_err());
    Ok(())
}

#[test]
fn backfill_restart_between_move_and_baseline_keeps_a_new_terminal_event() -> Result<()> {
    for lost_receipt in [false,true] {
        let (_,config,id,scope) = setup()?;
        let directory = tempfile::tempdir()?;
        let mut store = Store::open(directory.path(),true)?;
        store.native_sections = true;
        store.discover(&[json!({"id":id,"name":"业务会话","updatedAt":10})])?;
        store.observe(&id,&json!({"id":"same","status":"inProgress","startedAt":now()-10.0}),None)?;
        store.cache_native_backfill(&id,&default_position(),&scope)?;
        let change = store.prepare_native_backfill(&id,&config,
            &json!({"type":"active"}),&scope)?.unwrap();
        assert_eq!(change.state,"in_progress");
        if lost_receipt { store.cache_native_backfill(&id,&target(&change),&scope)?; }
        else { store.finish_native_backfill(&id,&change,&target(&change))?; }
        let cutoff = store.native_backfill_since(&id,0.0)?;
        drop(store);
        let mut store = Store::open(directory.path(),true)?;
        store.native_sections = true;
        let since = store.native_backfill_since(&id,cutoff+1000.0)?;
        assert_eq!(since,cutoff);
        let turn = json!({"id":"same","status":"completed","completedAt":cutoff+1.0});
        store.observe(&id,&turn,None)?;
        assert!(!store.baseline_native_backfill(&id,&json!({"type":"idle"}),&turn,since)?);
        let next = store.prepare_native_runtime(&id,&json!({"type":"idle"}),&turn,since)?.unwrap();
        assert_eq!(next.state,"review");
        assert!(!store.native_backfill_pending(&id)?);
        assert!(store.pending_titles(None)?.is_empty());
    }
    Ok(())
}
