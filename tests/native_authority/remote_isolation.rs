use super::entrypoints::Live;
use super::transport::{BlockingMove, Server};
use super::*;
use codex_session_board::store::remote::identity;
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

const SECOND: &str = "22222222-2222-4222-8222-222222222222";
type Client = reqwest::blocking::Client;

#[path = "remote_fixture.rs"]
mod remote_fixture;
use remote_fixture::RemoteFixture;

#[path = "remote_backfill.rs"]
mod backfill;

#[path = "remote_archive_batches.rs"]
mod archive_batches;

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
        if managed_ready(&snapshot)
            && snapshot["health"]["remoteConnections"]
                .as_array()
                .is_some_and(|hosts| {
                    hosts.len() == 2
                        && hosts.iter().all(|host| {
                            host["scanComplete"] == complete && host["runtimeVerified"] == complete
                        })
                })
            && after.is_none_or(|previous| {
                snapshot["health"]["remoteLastAt"].as_f64().unwrap_or(0.0) > previous
            })
        {
            return Ok(snapshot);
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "remote scan did not reach expected state: {}",snapshot["health"]
        );
        live.post_path(client, "/api/refresh", &json!({}))?
            .error_for_status()?;
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn managed_ready(snapshot: &Value) -> bool {
    snapshot["health"]["remoteConnections"]
        .as_array()
        .is_some_and(|connections| {
            connections.len() == 2
                && ["mac", "other"].iter().all(|host| {
                    connections.iter().any(|connection| {
                        connection["hostId"] == *host
                            && connection["runtimeVerified"] == true
                            && connection["scanComplete"] == true
                            && connection["writeError"].is_null()
                    }) && [FIRST, SECOND].iter().all(|thread| {
                        snapshot["sessions"].as_array().unwrap().iter().any(|row| {
                            row["id"] == identity(host, thread) && row["readOnly"] == false
                        })
                    })
                })
        })
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

fn actions(live: &Live, client: &Client, before: &Value, remotes: &RemoteFixture) -> Result<Value> {
    let started = Instant::now();
    let _pause = remotes.pause()?;
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
    let rejected_at = Instant::now();
    let rejected = live.post_path(
        client,
        "/api/state",
        &update(row(before, &remote_id), "review", "unverified native write"),
    )?;
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);
    let rejection: Value = rejected.json()?;
    assert!(rejection.to_string().contains("主机"));
    let remote = rejected_at.elapsed().as_secs_f64() * 1000.0;
    let after = live.snapshot(client)?;
    check_after(live, client, before, &after)?;
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    eprintln!("REMOTE_ACTIONS state={state} archive={archive} rejection={remote} total={elapsed}");
    anyhow::ensure!(elapsed < 1000.0, "foreground requests took {elapsed} ms");
    Ok(json!({"localStateMs":state,"localArchiveMs":archive,
        "remoteRejectedMs":remote,"foregroundTotalMs":elapsed}))
}

fn check_after(live: &Live, client: &Client, before: &Value, after: &Value) -> Result<()> {
    let remote_id = identity("mac", FIRST);
    assert_eq!(row(after, FIRST)["note"], "local note");
    assert_eq!(row(after, FIRST)["state"], "to_archive");
    assert_eq!(row(after, &remote_id)["note"], "offline-safe note");
    assert_eq!(row(after, &remote_id)["state"], "blocked");
    assert_eq!(row(after, &remote_id), row(before, &remote_id));
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

fn launch(
    server: &mut Server,
    directory: &std::path::Path,
    remotes: &RemoteFixture,
    runtime_poll_seconds: u64,
) -> Result<Live> {
    server.config.rpc_timeout_seconds = 6;
    server.config.poll_seconds = 60;
    server.config.runtime_poll_seconds = runtime_poll_seconds;
    setup_remote(server);
    let app = server.app()?;
    for host in ["mac", "other"] {
        let threads = server
            .surface
            .lock()
            .unwrap()
            .remote_threads
            .iter()
            .filter(|thread| thread["hostId"] == host)
            .cloned()
            .collect::<Vec<_>>();
        app.store.discover_remote(host, host, &threads)?;
    }
    app.store
        .save_remote(&json!({"id":identity("mac",FIRST),"state":"blocked",
        "note":"offline-safe note","version":0,"inputVersion":0}))?;
    app.discover(&[json!({"id":FIRST}), json!({"id":SECOND})], true)?;
    app.refresh_native()?;
    remotes.seed_native_cache(&server.config, &app.store)?;
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
    }
    Ok(match mode {
        "scanning-projects" => Some(begin_gate(server, live, client, "projects")?),
        _ => None,
    })
}

fn confirm_remote_receipt(live: &Live, client: &Client, mode: &str) -> Result<()> {
    let before = wait_remote(live, client, true, None)?;
    if mode != "online" { return Ok(()); }
    let identifier = identity("mac", FIRST);
    let response: Value = live.post_path(client, "/api/state",
        &update(row(&before, &identifier), "blocked", "offline-safe note"))?
        .error_for_status()?.json()?;
    let after = live.snapshot(client)?;
    assert_eq!(&response["session"], row(&after, &identifier));
    assert_eq!(response["session"]["id"], identifier);
    assert_eq!(response["session"]["threadId"], FIRST);
    assert_eq!(response["session"]["hostId"], "mac");
    assert_eq!(response["session"]["remote"], true);
    assert_eq!(row(&after, &identity("other", FIRST)), row(&before, &identity("other", FIRST)));
    Ok(())
}

fn scenario(mode: &str) -> Result<Value> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    let remotes = RemoteFixture::new(&mut server, directory.path())?;
    let live = launch(&mut server, directory.path(), &remotes, 60)?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?;
    live.ready(&client)?;
    confirm_remote_receipt(&live, &client, mode)?;
    remotes.assert_managed(&server.config)?;
    let gate = prepare_mode(&server, &live, &client, mode)?;
    server.surface.lock().unwrap().calls.clear();
    let snapshot_started = Instant::now();
    let before = live.snapshot(&client)?;
    let snapshot_ms = snapshot_started.elapsed().as_secs_f64() * 1000.0;
    anyhow::ensure!(snapshot_ms < 1000.0, "snapshot waited for the remote scan");
    let mut report = actions(&live, &client, &before, &remotes)?;
    remotes.assert_isolated()?;
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
fn remote_page_started_before_a_save_cannot_overwrite_the_confirmed_position() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    let remotes = RemoteFixture::new(&mut server, directory.path())?;
    let live = launch(&mut server, directory.path(), &remotes, 1)?;
    let client = Client::builder().no_proxy().timeout(Duration::from_secs(4)).build()?;
    live.ready(&client)?;
    let before = wait_remote(&live, &client, true, None)?;
    let target = identity("mac", FIRST);
    let surface = remotes.surface("mac");
    let pause_page = || {
        let (started, received) = mpsc::channel();
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        surface.lock().unwrap().runtime_page_reply = Some(BlockingMove {
            identifier: FIRST.into(), started, release: Arc::clone(&release),
        });
        (Gate(release), received)
    };
    let (stale, started) = pause_page();
    started.recv_timeout(Duration::from_secs(5))?;
    live.post_path(&client, "/api/state", &update(row(&before, &target), "review", "新确认"))?
        .error_for_status()?;
    let (next, recovering) = pause_page();
    drop(stale);
    recovering.recv_timeout(Duration::from_secs(6))?;
    let after = live.snapshot(&client)?;
    assert_eq!(row(&after, &target)["state"], "review");
    assert_eq!(row(&after, &target)["note"], "新确认");
    drop(next);
    Ok(())
}

#[test]
fn lost_remote_move_and_readback_preserve_the_note_until_refresh_without_replay() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    let remotes = RemoteFixture::new(&mut server, directory.path())?;
    let live = launch(&mut server, directory.path(), &remotes, 1)?;
    let client = Client::builder().no_proxy().timeout(Duration::from_secs(10)).build()?;
    live.ready(&client)?;
    let before = wait_remote(&live, &client, true, None)?;
    let remote_id = identity("mac", FIRST);
    let surface = remotes.surface("mac");
    let moves = || surface.lock().unwrap().calls.iter()
        .filter(|method| *method == "move_thread_to_sidebar_section").count();
    let initial = moves();
    {
        let mut surface = surface.lock().unwrap();
        surface.lose_receipt = true;
        surface.unreadable_section = Some("review".into());
    }
    let reply = live.post_path(&client, "/api/state",
        &update(row(&before, &remote_id), "review", "不能报告已保存"))?;
    assert_eq!(reply.status(), reqwest::StatusCode::BAD_REQUEST);
    let error = reply.text()?;
    assert!(error.contains("原生移动结果未确认"), "{error}");
    assert_eq!(surface.lock().unwrap().slots[FIRST], "review");
    let uncertain = live.snapshot(&client)?;
    assert_eq!(row(&uncertain, &remote_id)["note"], "offline-safe note");
    assert_eq!(row(&uncertain, &remote_id)["state"], "blocked");
    surface.lock().unwrap().unreadable_section = None;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let snapshot = live.snapshot(&client)?;
        let recovered = row(&snapshot, &remote_id);
        if recovered["state"] == "review" {
            assert_eq!(recovered["note"], "offline-safe note");
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "远程移动恢复读回超时");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(moves(), initial + 1);
    Ok(())
}

#[test]
fn local_save_commits_while_same_uuid_remote_move_waits() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    let remotes = RemoteFixture::new(&mut server, directory.path())?;
    let live = launch(&mut server, directory.path(), &remotes, 60)?;
    let client = Client::builder().no_proxy().timeout(Duration::from_secs(4)).build()?;
    live.ready(&client)?;
    let before = wait_remote(&live, &client, true, None)?;
    let remote_id = identity("mac", FIRST);
    let remote = update(row(&before, &remote_id), "review", "远端");
    let local = update(row(&before, FIRST), "to_archive", "本机");
    let (started, received) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    remotes.surface("mac").lock().unwrap().blocked = Some(BlockingMove {
        identifier: FIRST.into(), started, release: Arc::clone(&release),
    });
    std::thread::scope(|scope| -> Result<()> {
        let gate = Gate(release);
        let saving = scope.spawn(|| live.post_path(&client, "/api/state", &remote));
        received.recv_timeout(Duration::from_secs(3))?;
        live.post_path(&client, "/api/state", &local)?.error_for_status()?;
        assert_eq!(row(&live.snapshot(&client)?, FIRST)["note"], "本机");
        assert_eq!(remotes.surface("mac").lock().unwrap().slots[FIRST], "blocked");
        drop(gate);
        saving.join().unwrap()?.error_for_status()?;
        Ok(())
    })?;
    let after = live.snapshot(&client)?;
    assert_eq!(row(&after, &remote_id)["note"], "远端");
    assert_eq!(row(&after, FIRST)["note"], "本机");
    Ok(())
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
