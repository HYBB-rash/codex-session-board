use super::*;
use crate::config::Config;

struct Turns(Value);

impl Rpc for Turns {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        assert_eq!(method, "thread/turns/list");
        assert_eq!(params["threadId"], "session");
        assert_eq!(params["sortDirection"], "desc");
        assert_eq!(params["itemsView"], "full");
        Ok(self.0.clone())
    }
}

fn app(directory: &tempfile::TempDir, four_columns: bool) -> Result<App> {
    App::new(Config {
        data_dir: directory.path().into(),
        sync_titles: false,
        four_columns,
        ..Config::default()
    })
}

fn observe(app: &App, revision: i64, status: &str, response: Value) -> Result<()> {
    let rows = [json!({
        "id": "session", "status": {"type":status}, "updatedAt":revision,
    })];
    app.store.discover(&rows)?;
    observe_runtime_threads(app, &mut Turns(response), &mut HashMap::new(), 0.0, &rows)
}

fn turn(identifier: &str, status: &str, started: f64, finished: Option<f64>) -> Value {
    json!({"data":[{
        "id":identifier, "status":status, "startedAt":started, "completedAt":finished,
        "items":[{"type":"userMessage","id":"input"},{"type":"imageGeneration","id":"image"}],
    }]})
}

#[test]
fn history_projections_preserve_running_and_explicit_terminal_events_change_the_phase() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(&directory, true)?;
    observe(&app, 1, "active", turn("one", "inProgress", 1.0, None))?;
    for (revision, status) in [(2, "interrupted"), (3, "completed"), (4, "failed")] {
        observe(&app, revision, "idle", turn("one", status, 1.0, None))?;
        assert_eq!(app.store.sessions()?[0]["state"], "in_progress");
    }
    observe(&app, 5, "active", turn("one", "interrupted", 1.0, Some(5.0)))?;
    assert_eq!(app.store.sessions()?[0]["state"], "in_progress");
    observe(&app, 5, "idle", turn("one", "interrupted", 1.0, Some(5.0)))?;
    assert_eq!(app.store.sessions()?[0]["state"], "blocked");

    observe(&app, 6, "active", turn("two", "inProgress", 6.0, None))?;
    observe(&app, 7, "notLoaded", turn("two", "completed", 6.0, Some(7.0)))?;
    let row = &app.store.sessions()?[0];
    assert_eq!(row["state"], "review");
    assert_eq!(row["source"], "runtime");
    Ok(())
}

#[test]
fn a_terminal_event_older_than_the_manual_choice_keeps_the_card() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(&directory, true)?;
    observe(&app, 1, "active", turn("one", "inProgress", 1.0, None))?;
    let row = &app.store.sessions()?[0];
    app.store.set_state(
        "session",
        Some("to_archive"),
        "保留人工位置",
        row["version"].as_i64().unwrap(),
        row["inputVersion"].as_i64(),
    )?;
    observe(&app, 2, "idle", turn("one", "interrupted", 1.0, Some(2.0)))?;
    let row = &app.store.sessions()?[0];
    assert_eq!(row["state"], "to_archive");
    assert_eq!(row["note"], "保留人工位置");
    Ok(())
}

#[test]
fn empty_requires_complete_response_and_an_inactive_verified_backend() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(&directory, true)?;
    for (revision, status, response) in [
        (1,"idle",json!({"data":null})),
        (2,"idle",json!({"data":[],"nextCursor":"more"})),
        (3,"active",json!({"data":[],"nextCursor":null})),
        (4,"idle",json!({"data":[],"nextCursor":null})),
    ] {
        observe(&app, revision, status, response)?;
        assert_eq!(app.store.sessions()?[0]["confirmedEmpty"], revision == 4);
    }
    let legacy_directory = tempfile::tempdir()?;
    let legacy = self::app(&legacy_directory, false)?;
    observe(&legacy, 1, "idle", json!({"data":[],"nextCursor":null}))?;
    assert!(legacy.store.sessions()?[0].get("confirmedEmpty").is_none());
    Ok(())
}

#[test]
fn standalone_history_discovery_does_not_replace_verified_turns_in_four_column_mode() -> Result<()> {
    struct StaleHistory;

    impl Rpc for StaleHistory {
        fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
            assert_eq!(method, "thread/list");
            Ok(json!({"data":[
                {"id":"session","updatedAt":2},
                {"id":"session-newly-discovered","updatedAt":3},
            ]}))
        }
    }

    let directory = tempfile::tempdir()?;
    let app = app(&directory, true)?;
    observe(&app, 1, "idle", json!({"data":[]}))?;
    scan_history(&app, &mut StaleHistory)?;
    let sessions = app.store.sessions()?;
    let row = sessions.iter().find(|row| row["id"] == "session").unwrap();
    assert_eq!(row["updatedAt"], 1);
    assert_eq!(row["confirmedEmpty"], true);
    let discovered = sessions.iter().find(|row| row["id"] == "session-newly-discovered").unwrap();
    assert_eq!(discovered["confirmedEmpty"], false);
    Ok(())
}
#[test]
fn warnings_require_two_complete_observations_and_thirty_seconds() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(&directory, true)?;
    app.discover(&[json!({"id":"session","updatedAt":1})], true)?;
    app.request("/api/state", Some(&json!({"id":"session","state":"to_archive",
        "note":"保留人工备注","version":0,"inputVersion":0})))?;
    let start = Instant::now();
    let mut revisions = RuntimeRevisions::new();
    for (seconds, reliable, warning) in [
        (0, false, false), (29, false, false), (30, false, true),
        (31, true, false),
        (60, false, false), (89, false, false), (90, false, true),
        (91, true, false),
        (120, false, false), (150, false, true),
    ] {
        let status = if reliable { "idle" } else { "active" };
        let threads = [json!({"id":"session","updatedAt":1,"status":{"type":status}})];
        let response = turn("old", "completed", 1.0, reliable.then_some(2.0));
        observe_runtime_threads_at(&app, &mut Turns(response), &mut revisions, 0.0,
            &threads, start + Duration::from_secs(seconds))?;
        let snapshot = app.snapshot()?;
        let error = snapshot["health"].get("runtimeError").unwrap();
        if warning {
            assert!(error.as_str().is_some_and(|text| text.contains("持续无法确认执行状态")),
                "第 {seconds} 秒应提示持续缺证，实际为 {error}");
        } else {
            assert!(error.is_null(), "第 {seconds} 秒出现意外告警 {error}");
        }
        let row = &snapshot["sessions"][0];
        assert_eq!(row["id"], "session");
        assert_eq!(row["state"], "to_archive", "第 {seconds} 秒");
        assert_eq!(row["manual"], true);
        assert_eq!(row["note"], "保留人工备注");
    }
    Ok(())
}
