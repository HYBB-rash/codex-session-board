use super::{State, test_support::*, view};
use anyhow::Result;
use serde_json::{Value, json};

#[test]
fn four_column_board_keeps_a_confirmed_empty_archived_chat_in_the_live_catalog() -> Result<()> {
    use crate::{app::App, config::Config, native_sections::Observation, store::remote::identity};
    use std::sync::Arc;

    let directory = tempfile::tempdir()?;
    let config = Config {
        data_dir: directory.path().join("root"), four_columns: true,
        native_sections: true, sync_titles: false, ..Config::default()
    };
    let root = App::new(config.clone())?;
    let remote = Arc::new(App::new(Config {
        data_dir: directory.path().join("remote"), host_id: HOST.into(), ..config
    })?);
    remote.store.discover(&[thread(FIRST, false, true)])?;
    remote.store.observe(FIRST, &json!({"confirmedEmpty":true}), None)?;
    remote.store.cache_native(FIRST, &Observation { state: "unknown".into(), version: "chats".into() })?;
    root.remotes.lock().unwrap().insert(HOST.into(), remote.clone());
    let mut catalog = snapshot();
    catalog.active.push(thread(SECOND, false, true));
    catalog.validate(HOST)?;
    install(&remote, catalog.clone(), true);
    let active = root.request("/api/sessions", None)?;
    let active = active["sessions"].as_array().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0]["id"], identity(HOST, SECOND));
    assert_eq!(active[0].get("state"), Some(&Value::Null));

    catalog.active.remove(0);
    catalog.archived.push(thread(FIRST, true, true));
    catalog.validate(HOST)?;
    install(&remote, catalog, true);
    let archived = root.request("/api/sessions", None)?;
    let rows = archived["sessions"].as_array().unwrap();
    let row = rows.iter().find(|row| row["id"] == identity(HOST, FIRST))
        .expect("已归档的空会话仍应出现在根看板快照");
    assert_eq!(row["presence"], "archived");
    assert_eq!(row["archived"], true);
    assert_eq!(row.get("state"), Some(&Value::Null));
    assert_eq!(row["placementReason"], "原生侧栏默认区");
    Ok(())
}

#[test]
fn cached_snapshot_requires_live_reverification_after_restart() -> Result<()> {
    use crate::{app::App, config::Config, store::remote::identity};
    use std::sync::Arc;

    let directory = tempfile::tempdir()?;
    let config = Config {
        data_dir: directory.path().join("root"), four_columns: true, sync_titles: false,
        ..Config::default()
    };
    let root = App::new(config.clone())?;
    let remote_config = Config { data_dir: directory.path().join("remote"), host_id: HOST.into(), ..config };
    let snapshot = snapshot();
    snapshot.validate(HOST)?;
    App::new(remote_config.clone())?.store.save_catalog_value(
        "catalog_snapshot_v1",
        Some(&serde_json::to_value(snapshot)?),
    )?;
    let remote = Arc::new(App::new(remote_config)?);
    root.remotes.lock().unwrap().insert(HOST.into(), remote.clone());
    let check = |live: bool| -> Result<()> {
        let board = root.request("/api/sessions", None)?;
        let rows = board["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["id"], identity(HOST, FIRST));
        assert_eq!(row.get("state"), Some(&Value::Null));
        assert_eq!(row["presence"], if live { "active" } else { "cachedUnknown" });
        assert_eq!(row["catalogCurrent"], true);
        assert_eq!(row["canOpen"], live);
        assert_eq!(row["readOnly"], !live);
        let catalog = &board["health"]["remoteConnections"][0]["catalog"];
        assert_eq!(catalog["complete"], live);
        assert_eq!(catalog["cached"], !live);
        Ok(())
    };
    check(false)?;
    install(&remote, super::test_support::snapshot(), true);
    check(true)?;
    remote.deactivate()?;
    check(false)
}

#[test]
fn old_cached_session_needs_positive_runtime_evidence_to_be_current() -> Result<()> {
    let (_directory, app) = app()?;
    app.store.discover(&[thread(FIRST, false, true)])?;
    let history = view::read(&app, &[])?.rows;
    assert_eq!(history[0]["catalogCurrent"], false);
    assert_eq!(history[0]["readOnly"], true);
    app.discover(&[thread(FIRST, false, true)], true)?;
    app.update_health(json!({"runtimeVerified":true}))?;
    let current = view::read(&app, &[])?.rows;
    assert_eq!(current[0]["presence"], "active");
    assert_eq!(current[0]["catalogEvidence"], "runtime");
    assert_eq!(current[0]["ordinary"], true);
    assert_eq!(current[0]["readOnly"], false);
    assert_eq!(current[0]["catalogProjectId"], Value::Null);
    assert!(current[0]["projectIssue"].is_string());
    app.update_health(json!({"runtimeVerified":false}))?;
    let disconnected = view::read(&app, &[])?.rows;
    assert_eq!(disconnected[0]["presence"], "cachedUnknown");
    assert_eq!(disconnected[0]["readOnly"], true);
    app.update_health(json!({"runtimeVerified":true}))?;
    app.discover(&[], true)?;
    assert_eq!(view::read(&app, &[])?.rows[0]["catalogCurrent"], false);
    Ok(())
}

#[test]
fn owned_pending_draft_cannot_resurrect_a_previously_published_session() -> Result<()> {
    let (_directory, app) = app()?;
    app.store.discover(&[thread(FIRST, false, true)])?;
    app.update_health(json!({"runtimeVerified":true}))?;
    let drafts = vec![json!({"threadId":FIRST,"project":"remote:owned-project"})];
    let pending = view::read(&app, &drafts)?.rows;
    assert_eq!(pending[0]["catalogEvidence"], "pendingDraft");
    assert_eq!(pending[0]["ordinary"], false);
    assert_eq!(pending[0]["readOnly"], false);
    app.store.catalog_mark_published(&[FIRST.into()])?;
    let history = view::read(&app, &drafts)?.rows;
    assert_eq!(history[0]["catalogEvidence"], "cache");
    assert_eq!(history[0]["catalogCurrent"], false);
    assert_eq!(history[0]["readOnly"], true);
    Ok(())
}

#[test]
fn catalog_metadata_preserves_notes_and_explicit_projectless_membership() -> Result<()> {
    let (_directory, app) = app()?;
    install(&app, snapshot(), true);
    assert_eq!(view::read(&app, &[])?.rows[0]["readOnly"], true);
    app.store.discover(&[thread(FIRST, false, true)])?;
    app.store
        .set_state(FIRST, Some("review"), "保留本机备注", 0, Some(0))?;
    let rows = view::read(&app, &[])?.rows;
    assert_eq!(rows[0]["title"], "远程目录标题");
    assert_eq!(rows[0]["note"], "保留本机备注");
    assert_eq!(rows[0]["state"], "review");
    assert_eq!(rows[0]["catalogProjectId"], Value::Null);
    assert_eq!(rows[0]["membership"]["state"], "projectless");
    assert_eq!(rows[0]["projectIssue"], Value::Null);
    assert_eq!(rows[0]["readOnly"], false);
    Ok(())
}

#[test]
fn archive_tombstone_wins_over_an_active_catalog_row() -> Result<()> {
    let (_directory, app) = app()?;
    app.store.discover(&[thread(FIRST, false, true)])?;
    app.store
        .set_state(FIRST, Some("to_archive"), "", 0, Some(0))?;
    let version = app.store.sessions()?[0]["version"].as_i64().unwrap();
    app.store.archive(FIRST, version, 0, |_| Ok(()))?;
    install(&app, snapshot(), true);
    let rows = view::read(&app, &[])?.rows;
    assert_eq!(rows[0]["archived"], true);
    assert_eq!(rows[0]["presence"], "archived");
    assert_eq!(rows[0]["readOnly"], true);
    assert_eq!(rows[0]["canArchive"], false);
    assert!(!app.store.contains(FIRST)?);
    Ok(())
}

#[test]
fn empty_coverage_and_foreign_project_cache_are_never_complete() -> Result<()> {
    let mut snapshot = snapshot();
    snapshot.coverage = json!({});
    assert!(snapshot.validate(HOST).is_err());
    let state = State {
        snapshot: Some(snapshot),
        live: true,
        ..State::default()
    };
    assert_eq!(state.health()["complete"], false);
    let mut snapshot = super::test_support::snapshot();
    snapshot.projects.push(
        json!({"id":"target:[\"remote:other\",\"targetServer\",\"p\"]",
        "origin":"targetServer","hostId":HOST,"canCreate":false,
        "canMove":false,"canOpenProject":false}),
    );
    assert!(snapshot.validate(HOST).is_err());
    Ok(())
}
