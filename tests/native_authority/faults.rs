use super::transport::{body, BlockingMove, Server};
use super::*;
use serde_json::Value;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

#[test]
fn remote_rejection_and_local_abort_do_not_commit_notes_or_replay_native_moves() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    server.surface.lock().unwrap().reject_move = Some(FIRST.into());
    assert!(app
        .request("/api/state", Some(&body(&app, "to_archive", "失败备注")?))
        .is_err());
    assert_eq!(app.store.note(FIRST)?, "");
    assert_eq!(server.surface.lock().unwrap().slots[FIRST], "review");
    server.surface.lock().unwrap().reject_move = None;
    app.request("/api/state", Some(&body(&app, "to_archive", "已确认")?))?;
    app.store.connection()?.execute_batch("CREATE TRIGGER reject_note BEFORE UPDATE ON sessions WHEN NEW.note='rejectcommit' BEGIN SELECT RAISE(ABORT,'injected commit failure'); END;")?;
    assert!(app
        .request("/api/state", Some(&body(&app, "review", "rejectcommit")?))
        .is_err());
    assert_eq!(app.store.note(FIRST)?, "已确认");
    assert_eq!(server.surface.lock().unwrap().slots[FIRST], "review");
    app.refresh_native()?;
    app.refresh_native()?;
    assert_eq!(app.snapshot()?["sessions"][0]["state"], "review");
    assert_eq!(app.store.note(FIRST)?, "已确认");
    Ok(())
}

#[test]
fn input_arriving_during_a_native_move_cannot_be_confirmed_by_the_old_save() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = Arc::new(server.app()?);
    app.request("/api/state", Some(&body(&app, "review", "原备注")?))?;
    let (started_sender, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    server.surface.lock().unwrap().blocked = Some(BlockingMove {
        identifier: FIRST.into(),
        started: started_sender,
        release: Arc::clone(&release),
    });
    let saving = Arc::clone(&app);
    let request = body(&app, "to_archive", "过期备注")?;
    let worker = std::thread::spawn(move || saving.request("/api/state", Some(&request)));
    let began = started.recv_timeout(Duration::from_secs(1));
    app.store.observe(FIRST,&json!({"id":"new-turn","status":"completed","startedAt":codex_session_board::model::now()+1.0,"userItems":["new-input"]}),None)?;
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    began?;
    assert!(worker.join().unwrap().is_err());
    app.refresh_native()?;
    let session = &app.snapshot()?["sessions"][0];
    assert_eq!(session["state"], "to_archive");
    assert_eq!(session["followUp"], true);
    assert_eq!(session["note"], "原备注");
    assert!(app
        .request("/api/archive", Some(&body(&app, "to_archive", "")?))
        .is_err());
    Ok(())
}

struct Runtime {
    since: f64,
}

struct DelayedRead<'app> {
    app: &'app App,
    old: Value,
}

impl Rpc for DelayedRead<'_> {
    fn call(&mut self, method: &str, _: Value) -> Result<Value> {
        anyhow::ensure!(method == "list_threads", "read only");
        self.app
            .request("/api/state", Some(&body(self.app, "to_archive", "新确认")?))?;
        Ok(self.old.clone())
    }
}

#[test]
fn a_late_read_snapshot_cannot_overwrite_a_confirmed_native_save() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    let old = server.surface.lock().unwrap().layout();
    app.read_native(&mut DelayedRead { app: &app, old })?;
    assert_eq!(app.snapshot()?["sessions"][0]["state"], "to_archive");
    assert_eq!(app.store.note(FIRST)?, "新确认");
    Ok(())
}

impl Rpc for Runtime {
    fn call(&mut self, method: &str, args: Value) -> Result<Value> {
        match method {
            "thread/list" => Ok(json!({"data":[
                {"id":FIRST,"updatedAt":1,"status":{"type":"active","activeFlags":[]}},
                {"id":"22222222-2222-4222-8222-222222222222","updatedAt":1,"status":{"type":"active","activeFlags":[]}}]})),
            "thread/turns/list" => Ok(
                json!({"data":[{"id":args["threadId"],"status":"inProgress","startedAt":self.since+1.0,"items":[]}]}),
            ),
            _ => anyhow::bail!("unexpected runtime read"),
        }
    }
}

#[test]
fn one_failed_native_stage_does_not_starve_a_later_chat() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    app.request("/api/state", Some(&body(&app, "review", "保留备注")?))?;
    let second = "22222222-2222-4222-8222-222222222222";
    app.discover(&[json!({"id":FIRST}), json!({"id":second})], true)?;
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(second.into(), "review".into());
    server.surface.lock().unwrap().reject_move = Some(FIRST.into());
    app.refresh_native()?;
    let since = codex_session_board::model::now();
    let mut revisions = std::collections::HashMap::new();
    codex_session_board::observer::scan_runtime(
        &app,
        &mut Runtime { since },
        &mut revisions,
        since,
    )?;
    assert_eq!(server.surface.lock().unwrap().slots[FIRST], "review");
    assert_eq!(server.surface.lock().unwrap().slots[second], "in_progress");
    assert!(!revisions.contains_key(FIRST));
    assert!(revisions.contains_key(second));
    assert!(app.snapshot()?["health"]["nativeStageError"].is_string());
    verify_failed_stage_after_external_choice(&app, &server, since, &mut revisions)?;
    Ok(())
}

fn native_move_count(server: &Server) -> usize {
    server
        .surface
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|method| method.as_str() == "move_thread_to_sidebar_section")
        .count()
}

fn verify_failed_stage_after_external_choice(
    app: &App,
    server: &Server,
    since: f64,
    revisions: &mut std::collections::HashMap<String, String>,
) -> Result<()> {
    let moves = native_move_count(server);
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "to_archive".into());
    codex_session_board::observer::scan_runtime(app, &mut Runtime { since }, revisions, since)?;
    assert_eq!(native_move_count(server), moves);
    app.refresh_native()?;
    codex_session_board::observer::scan_runtime(app, &mut Runtime { since }, revisions, since)?;
    assert_eq!(native_move_count(server), moves);
    let restarted = App::new(server.config.clone())?;
    restarted.refresh_native()?;
    codex_session_board::observer::scan_runtime(
        &restarted,
        &mut Runtime { since },
        &mut Default::default(),
        since,
    )?;
    assert_eq!(native_move_count(server), moves);
    assert_eq!(server.surface.lock().unwrap().slots[FIRST], "to_archive");
    assert_eq!(restarted.store.note(FIRST)?, "保留备注");
    Ok(())
}

#[test]
fn pinned_or_unmanaged_native_sections_are_not_moved_by_automatic_stages() -> Result<()> {
    for section in ["pinned", "custom-unmanaged-section"] {
        let directory = tempfile::tempdir()?;
        let app = native_app(directory.path())?;
        app.discover(&[json!({"id":FIRST})], true)?;
        app.store.cache_native(
            FIRST,
            &Observation {
                state: "unknown".into(),
                version: section.into(),
            },
        )?;
        let since = codex_session_board::model::now();
        let active = json!({"type":"active","activeFlags":[]});
        let turn = json!({"id":"turn","status":"inProgress","startedAt":since+1.0});
        app.observe_runtime(FIRST, &active, &turn, since)?;
        assert_eq!(app.store.native_version(FIRST)?, section);
        app.store.cache_native(
            FIRST,
            &Observation {
                state: "review".into(),
                version: "review-id".into(),
            },
        )?;
        app.observe_runtime(FIRST, &active, &turn, since)?;
        assert_eq!(app.snapshot()?["sessions"][0]["state"], "review");
    }
    Ok(())
}
