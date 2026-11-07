use super::{
    archive::Engine,
    entrypoints::Live,
    transport::{BlockingMove, Server, body},
    *,
};
use codex_session_board::auto_archive::sweep;
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

fn prepared() -> Result<(tempfile::TempDir, Server, Engine, App)> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.auto_archive_seconds = Some(1);
    server.config.desktop_socket = directory.path().join("auto-engine.sock");
    let engine = Engine::new(&server.config.desktop_socket, Arc::clone(&server.surface))?;
    let app = server.app()?;
    app.request("/api/state", Some(&body(&app, "to_archive", "保留备注")?))?;
    Ok((directory, server, engine, app))
}

fn remote_candidate(app: &App) -> Result<Value> {
    app.store.discover_remote(
        "other",
        "remote",
        &[json!({"id":FIRST,"hostId":"other","title":"remote","updatedAt":1})],
    )?;
    let identifier = codex_session_board::store::remote::identity("other", FIRST);
    app.store.save_remote(&json!({
        "id":identifier,"state":"to_archive","note":"远程备注","version":0,"inputVersion":0
    }))?;
    app.store.remote_session(&identifier)
}

#[test]
fn sweep_verifies_live_input_and_running_status_without_confirming_or_archiving_remote()
-> Result<()> {
    let (_directory, server, engine, app) = prepared()?;
    let remote = remote_candidate(&app)?;
    let old = json!({"id":"turn","status":"completed","userItems":["old"]});
    app.store.observe(FIRST, &old, None)?;
    app.request("/api/state", Some(&body(&app, "to_archive", "保留备注")?))?;
    for (status, turn) in [
        (
            "active",
            json!({"id":"turn","status":"completed","items":[{"id":"old","type":"userMessage"}]}),
        ),
        (
            "idle",
            json!({"id":"new-turn","status":"completed","items":[{"id":"new","type":"userMessage"}]}),
        ),
        (
            "idle",
            json!({"id":"turn","status":"completed","items":[{"id":"old","type":"userMessage"},{"id":"new","type":"userMessage"}]}),
        ),
    ] {
        {
            let mut surface = server.surface.lock().unwrap();
            surface.thread_status = Some(status.into());
            surface.latest_turn = Some(turn);
        }
        let report = sweep(&app)?;
        assert_eq!(report["deferred"].as_array().unwrap().len(), 1);
        assert!(engine.archived.lock().unwrap().is_empty());
        assert_eq!(app.store.sessions()?[0]["followUp"], false);
    }
    server.surface.lock().unwrap().latest_turn = Some(json!({
        "id":"turn","status":"completed","items":[{"id":"old","type":"userMessage"}]
    }));
    assert_eq!(sweep(&app)?["archived"], json!([FIRST]));
    assert!(engine.archived.lock().unwrap().contains(FIRST));
    assert!(app.store.sessions()?.is_empty());
    assert_eq!(
        app.store.remote_session(remote["id"].as_str().unwrap())?,
        remote
    );
    assert!(sweep(&app)?["archived"].as_array().unwrap().is_empty());
    Ok(())
}

#[test]
fn sweep_rechecks_native_position_after_selecting_the_candidate() -> Result<()> {
    let (_directory, server, engine, app) = prepared()?;
    let app = Arc::new(app);
    let (sender, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    server.surface.lock().unwrap().read_blocked = Some(BlockingMove {
        identifier: FIRST.into(),
        started: sender,
        release: Arc::clone(&release),
    });
    let archiving = Arc::clone(&app);
    let worker = std::thread::spawn(move || sweep(&archiving));
    let began = started.recv_timeout(Duration::from_secs(2));
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "review".into());
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    began?;
    let report = worker.join().unwrap()?;
    assert_eq!(report["deferred"].as_array().unwrap().len(), 1);
    assert!(engine.archived.lock().unwrap().is_empty());
    assert_eq!(app.store.note(FIRST)?, "保留备注");
    assert_eq!(app.store.sessions()?.len(), 1);
    Ok(())
}

fn wait_for_timer(live: &Live, client: &reqwest::blocking::Client) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let snapshot = live.snapshot(client)?;
        let last = &snapshot["health"]["autoArchiveLastRun"];
        if last["archived"]
            .as_array()
            .is_some_and(|ids| ids.contains(&json!(FIRST)))
        {
            return Ok(snapshot);
        }
        anyhow::ensure!(Instant::now() < deadline, "timer did not archive");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn built_service_archives_on_its_timer_and_publishes_the_result_without_an_archive_request()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.auto_archive_seconds = Some(1);
    server.config.runtime_poll_seconds = 1;
    drop(server.app()?);
    let live = Live::new(&mut server.config, directory.path())?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?;
    let ready = live.ready(&client)?;
    let session = &ready["sessions"][0];
    live.post_path(
        &client,
        "/api/state",
        &json!({
            "id":FIRST,"state":"to_archive","note":"定时归档保留",
            "version":session["version"],"inputVersion":session["inputVersion"],
            "sectionVersion":session["sectionVersion"]
        }),
    )?
    .error_for_status()?;
    let snapshot = wait_for_timer(&live, &client)?;
    assert!(snapshot["sessions"].as_array().unwrap().is_empty());
    assert_eq!(snapshot["health"]["autoArchiveSeconds"], 1);
    let persisted: Value = serde_json::from_slice(&std::fs::read(
        server.config.data_dir.join("auto-archive.json"),
    )?)?;
    assert!(persisted["at"].is_number());
    let store = Store::open(&server.config.data_dir, false)?;
    assert_eq!(store.note(FIRST)?, "定时归档保留");
    assert!(store.sessions()?.is_empty());
    Ok(())
}
