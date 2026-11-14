use super::{State, test_support::*, view};
use anyhow::Result;
use serde_json::{Value, json};

#[test]
fn cached_snapshot_requires_live_reverification_after_restart() -> Result<()> {
    let (_directory, app) = app()?;
    let snapshot = snapshot();
    snapshot.validate(HOST)?;
    app.store.save_catalog_value(
        "catalog_snapshot_v1",
        Some(&serde_json::to_value(snapshot)?),
    )?;
    let reopened = crate::app::App::new(app.config.clone())?;
    let view = view::read(&reopened, &[])?;
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0]["presence"], "cachedUnknown");
    assert_eq!(view.rows[0]["catalogCurrent"], true);
    assert_eq!(view.rows[0]["readOnly"], true);
    assert_eq!(view.health["complete"], false);
    assert_eq!(view.health["cached"], true);
    install(&app, super::test_support::snapshot(), true);
    app.deactivate()?;
    let disabled = view::read(&app, &[])?;
    assert_eq!(disabled.rows[0]["presence"], "cachedUnknown");
    assert_eq!(disabled.rows[0]["readOnly"], true);
    assert_eq!(disabled.health["complete"], false);
    Ok(())
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
    app.store.discover(&[thread(FIRST, false, true)])?;
    app.store
        .set_state(FIRST, Some("review"), "保留本机备注", 0, Some(0))?;
    install(&app, snapshot(), true);
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
