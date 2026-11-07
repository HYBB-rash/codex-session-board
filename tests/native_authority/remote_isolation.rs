use super::entrypoints::Live;
use super::transport::{BlockingMove, Server};
use super::*;
use codex_session_board::store::remote::identity;
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

const SECOND: &str = "22222222-2222-4222-8222-222222222222";
type Client = reqwest::blocking::Client;

struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Drop for Gate {
    fn drop(&mut self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

fn row<'snapshot>(snapshot: &'snapshot Value, identifier: &str) -> &'snapshot Value {
    snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["id"] == identifier)
        .unwrap()
}

fn update(row: &Value, state: &str, note: &str) -> Value {
    json!({"id":row["id"],"state":state,"note":note,"version":row["version"],
        "inputVersion":row["inputVersion"],"sectionVersion":row["sectionVersion"]})
}

fn wait_remote(live: &Live, client: &Client, complete: bool, after: Option<f64>) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = live.snapshot(client)?;
        if snapshot["health"]["remoteHosts"]
            .as_array()
            .is_some_and(|hosts| {
                hosts.len() == 2 && hosts.iter().all(|host| host["scanComplete"] == complete)
            })
            && after.is_none_or(|previous| {
                snapshot["health"]["remoteLastAt"].as_f64().unwrap_or(0.0) > previous
            })
        {
            return Ok(snapshot);
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "remote scan did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn setup_remote(server: &Server) {
    let mut surface = server.surface.lock().unwrap();
    for host in ["mac", "other"] {
        surface.remote_projects.push(json!({
            "projectKind":"remote","hostId":host,"projectId":"same","path":"/remote",
            "label":"Project","hostDisplayName":host
        }));
        for identifier in [FIRST, SECOND] {
            surface.remote_threads.push(json!({
                "id":identifier,"hostId":host,"title":"remote","projectId":"same","updatedAt":1
            }));
        }
    }
    surface.slots.insert(SECOND.into(), "to_archive".into());
}

fn begin_gate(server: &Server, live: &Live, client: &Client, target: &str) -> Result<Gate> {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let guard = Gate(Arc::clone(&release));
    let (sender, received) = mpsc::channel();
    server.surface.lock().unwrap().remote_blocked = Some(BlockingMove {
        identifier: target.into(),
        started: sender,
        release,
    });
    live.post_path(client, "/api/refresh", &json!({}))?
        .error_for_status()?;
    received.recv_timeout(Duration::from_secs(2))?;
    Ok(guard)
}

fn timed_post(live: &Live, client: &Client, path: &str, body: &Value) -> Result<f64> {
    let started = Instant::now();
    let reply: Value = live
        .post_path(client, path, body)?
        .error_for_status()?
        .json()?;
    assert_eq!(reply["ok"], true);
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

fn actions(live: &Live, client: &Client, before: &Value) -> Result<Value> {
    let started = Instant::now();
    let state = timed_post(
        live,
        client,
        "/api/state",
        &update(row(before, FIRST), "to_archive", "local note"),
    )?;
    let archive = timed_post(
        live,
        client,
        "/api/archive",
        &update(row(before, SECOND), "to_archive", ""),
    )?;
    let remote_id = identity("mac", FIRST);
    let remote = timed_post(
        live,
        client,
        "/api/state",
        &update(row(before, &remote_id), "blocked", "offline-safe note"),
    )?;
    let after = live.snapshot(client)?;
    check_after(live, client, before, &after)?;
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    anyhow::ensure!(elapsed < 1000.0, "foreground requests took {elapsed} ms");
    Ok(json!({"localStateMs":state,"localArchiveMs":archive,
        "remoteSaveMs":remote,"foregroundTotalMs":elapsed}))
}

fn check_after(live: &Live, client: &Client, before: &Value, after: &Value) -> Result<()> {
    let remote_id = identity("mac", FIRST);
    assert_eq!(row(after, FIRST)["note"], "local note");
    assert_eq!(row(after, FIRST)["state"], "to_archive");
    assert_eq!(row(after, &remote_id)["note"], "offline-safe note");
    assert_eq!(row(after, &remote_id)["state"], "blocked");
    assert_eq!(row(after, &identity("other", FIRST))["version"], 0);
    assert_eq!(row(after, &identity("other", FIRST))["note"], "");
    assert_eq!(row(after, &identity("mac", SECOND))["state"], "unknown");
    assert!(
        !after["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["id"] == SECOND)
    );
    assert!(
        !live
            .post_path(
                client,
                "/api/state",
                &update(row(before, &remote_id), "review", "stale")
            )?
            .status()
            .is_success()
    );
    assert!(
        !live
            .post_path(
                client,
                "/api/archive",
                &update(row(after, &remote_id), "to_archive", "")
            )?
            .status()
            .is_success()
    );
    Ok(())
}

fn restart_offline(
    server: &mut Server,
    directory: &std::path::Path,
    client: &Client,
) -> Result<()> {
    let restarted = Live::new(&mut server.config, directory)?;
    restarted.ready(client)?;
    let snapshot = restarted.snapshot(client)?;
    assert_eq!(
        row(&snapshot, &identity("mac", FIRST))["note"],
        "offline-safe note"
    );
    assert_eq!(row(&snapshot, &identity("mac", FIRST))["state"], "blocked");
    server.surface.lock().unwrap().remote_disconnected = false;
    restarted
        .post_path(client, "/api/refresh", &json!({}))?
        .error_for_status()?;
    let snapshot = wait_remote(&restarted, client, true, None)?;
    assert_eq!(
        row(&snapshot, &identity("mac", FIRST))["note"],
        "offline-safe note"
    );
    assert_eq!(row(&snapshot, &identity("mac", FIRST))["state"], "blocked");
    Ok(())
}

fn launch(server: &mut Server, directory: &std::path::Path) -> Result<Live> {
    server.config.rpc_timeout_seconds = 6;
    server.config.poll_seconds = 60;
    server.config.runtime_poll_seconds = 60;
    setup_remote(server);
    let app = server.app()?;
    app.discover(&[json!({"id":FIRST}), json!({"id":SECOND})], true)?;
    app.refresh_native()?;
    drop(app);
    Live::new(&mut server.config, directory)
}

fn finish_scan(
    gate: Option<Gate>,
    server: &Server,
    live: &Live,
    client: &Client,
    before: &Value,
) -> Result<()> {
    if gate.is_some() {
        drop(gate);
        server.surface.lock().unwrap().remote_blocked = None;
        let after = wait_remote(
            live,
            client,
            true,
            before["health"]["remoteLastAt"].as_f64(),
        )?;
        assert_eq!(
            row(&after, &identity("mac", FIRST))["note"],
            "offline-safe note"
        );
        assert_eq!(row(&after, &identity("mac", FIRST))["state"], "blocked");
    }
    Ok(())
}

fn prepare_mode(server: &Server, live: &Live, client: &Client, mode: &str) -> Result<Option<Gate>> {
    if mode == "disconnected" {
        server.surface.lock().unwrap().remote_disconnected = true;
        live.post_path(client, "/api/refresh", &json!({}))?
            .error_for_status()?;
        wait_remote(live, client, false, None)?;
    }
    Ok(match mode {
        "scanning-projects" => Some(begin_gate(server, live, client, "projects")?),
        "scanning-history" => Some(begin_gate(server, live, client, "mac")?),
        _ => None,
    })
}

fn scenario(mode: &str) -> Result<Value> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    let live = launch(&mut server, directory.path())?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?;
    live.ready(&client)?;
    wait_remote(&live, &client, true, None)?;
    let gate = prepare_mode(&server, &live, &client, mode)?;
    server.surface.lock().unwrap().calls.clear();
    let snapshot_started = Instant::now();
    let before = live.snapshot(&client)?;
    let snapshot_ms = snapshot_started.elapsed().as_secs_f64() * 1000.0;
    anyhow::ensure!(snapshot_ms < 1000.0, "snapshot waited for the remote scan");
    let mut report = actions(&live, &client, &before)?;
    report["scenario"] = json!(mode);
    report["remoteBudgetMs"] = json!(6000);
    report["snapshotMs"] = json!(snapshot_ms);
    report["finishedBeforeRemoteReleased"] =
        json!(gate.as_ref().is_some_and(|gate| !*gate.0.0.lock().unwrap()));
    assert!(
        !server
            .surface
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|method| method == "list_threads")
    );
    finish_scan(gate, &server, &live, &client, &before)?;
    drop(live);
    if mode == "disconnected" {
        restart_offline(&mut server, directory.path(), &client)?;
    }
    Ok(report)
}

#[test]
fn remote_online_disconnected_and_scanning_do_not_delay_local_http_mutations() -> Result<()> {
    let mut reports = Vec::new();
    for mode in [
        "online",
        "disconnected",
        "scanning-projects",
        "scanning-history",
    ] {
        reports.push(scenario(mode)?);
    }
    if let Ok(path) = std::env::var("NATIVE_LATENCY_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports)?)?;
    }
    eprintln!("REMOTE_ISOLATION {}", serde_json::to_string(&reports)?);
    Ok(())
}
