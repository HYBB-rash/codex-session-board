use super::transport::{BlockingMove, Server, Surface, body, section_id, wait_for_release};
use super::*;
use anyhow::Context;
use serde_json::Value;
use std::collections::HashSet;
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

const SECOND: &str = "22222222-2222-4222-8222-222222222222";

#[test]
fn archive_batch_recovers_lost_receipt_after_reopen_without_replay() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated_archive_batch(directory.path(), "archive_batch_recovers_lost_receipt_after_reopen_without_replay")? {
        return Ok(());
    }
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    app.request("/api/state", Some(&body(&app, "to_archive", "保留备注")?))?;
    {
        let mut native = server.surface.lock().unwrap();
        native.archive_account = Some("original".into());
        native.lose_archive_receipt = true;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut first = body(&app, "to_archive", "")?;
    first["title"] = json!("丢失回执");
    let second = json!({"id":SECOND,"title":"尚未提交","version":0,"inputVersion":0});
    let manifest = json!({"id":id,"items":[first,second]});
    let target = json!({"id":FIRST, "batchId":id});
    app.request("/api/archive-batches", Some(&manifest))?;
    assert!(app.request("/api/archive-batches", Some(&json!({"id":id,"items":[manifest["items"][0]]}))).is_err());
    assert_eq!(app.request("/api/archive", Some(&target))?["item"]["status"], "pending");
    assert_eq!(app.request("/api/archive", Some(&target))?["item"]["status"], "pending");
    assert!(app.store.contains(FIRST)?);
    drop(app);
    assert_batch_recovery(&server, &id)
}

fn assert_batch_recovery(server: &Server, id: &str) -> Result<()> {
    let reopened = App::new(server.config.clone())?;
    reopened.refresh_native()?;
    reopened.discover(&[], true)?;
    assert!(!reopened.snapshot()?["sessions"].as_array().unwrap().iter().any(|row| row["id"] == FIRST));
    let check = |path: &str| reopened.request(path, Some(&json!({"id":id})));
    let items = |value: &Value, identifier: &str| value["batch"]["items"].as_array().unwrap()
        .iter().find(|item| item["id"] == identifier).unwrap()["status"].clone();
    let history = reopened.request("/api/archive-batches", None)?;
    assert_eq!(history["batches"][0]["id"], id);
    assert_eq!(items(&json!({"batch":history["batches"][0]}), SECOND), "awaiting");
    assert_eq!(items(&check("/api/archive-batches/finish")?, FIRST), "pending");
    assert_eq!(reopened.request("/api/archive", Some(&json!({"id":SECOND,"batchId":id})))?["item"]["status"], "awaiting");

    server.surface.lock().unwrap().archive_account = Some("different".into());
    assert_eq!(items(&check("/api/archive-batches/check")?, FIRST), "pending");
    server.surface.lock().unwrap().archive_account = Some("original".into());
    assert_eq!(items(&check("/api/archive-batches/check")?, FIRST), "verified");
    assert_eq!(items(&check("/api/archive-batches/check")?, SECOND), "awaiting");
    assert!(!reopened.store.contains(FIRST)?);
    assert_eq!(reopened.store.note(FIRST)?, "保留备注");
    assert_eq!(server.surface.lock().unwrap().calls.iter().filter(|method| *method == "thread/archive").count(), 1);
    Ok(())
}

#[test]
fn archive_batch_persists_new_input_refusal_without_native_write() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated_archive_batch(directory.path(), "archive_batch_persists_new_input_refusal_without_native_write")? {
        return Ok(());
    }
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    app.request("/api/state", Some(&body(&app, "to_archive", "")?))?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut target = body(&app, "to_archive", "")?;
    target["title"] = json!("有新输入");
    let unknown = uuid::Uuid::new_v4().to_string();
    let register = |batch: &str| app.request("/api/archive-batches",
        Some(&json!({"id":batch,"items":[target]})));
    register(&unknown)?;
    let unverified = app.request("/api/archive", Some(&json!({"batchId":unknown,"id":FIRST})))?;
    assert_eq!(unverified["item"]["status"], "rejected");
    register(&id)?;
    {
        let mut native = server.surface.lock().unwrap();
        native.archive_account = Some("original".into());
        native.latest_turn = Some(json!({
            "id":"new","status":"completed","items":[{"id":"input","type":"userMessage"}],
        }));
    }
    let submitted = app.request("/api/archive", Some(&json!({"batchId": id,"id":FIRST})))?;
    assert_eq!(submitted["item"]["status"], "newInput");
    assert!(app.store.contains(FIRST)?);
    let checked = app.request("/api/archive-batches/check", Some(&json!({"id":id})))?;
    assert_eq!(checked["batch"]["items"][0]["status"], "newInput");
    assert!(!server.surface.lock().unwrap().calls.iter().any(|method| method == "thread/archive"));
    Ok(())
}

fn isolated_archive_batch(home: &std::path::Path, test: &str) -> Result<bool> {
    if std::env::var_os("CSB_ARCHIVE_BATCH_FIXTURE").is_some() {
        return Ok(false);
    }
    let result = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", &format!("archive::{test}"), "--nocapture"])
        .env("CSB_ARCHIVE_BATCH_FIXTURE", "1")
        .env("CODEX_HOME", home).output()?;
    anyhow::ensure!(result.status.success(), "隔离归档批次测试失败: {test}\n{}\n{}",
        String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    Ok(true)
}

pub(super) struct Engine {
    pub archived: Arc<Mutex<HashSet<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    pub blocked: Arc<Mutex<Option<BlockingMove>>>,
    discovery: Arc<(Mutex<bool>, Condvar)>,
}

pub(super) struct DiscoveryPause(Arc<(Mutex<bool>, Condvar)>);

impl Drop for DiscoveryPause {
    fn drop(&mut self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

impl Engine {
    pub(super) fn pause_discovery_after_archive(&self) -> DiscoveryPause {
        *self.discovery.0.lock().unwrap() = false;
        DiscoveryPause(Arc::clone(&self.discovery))
    }

    pub fn new(path: &std::path::Path, surface: Arc<Mutex<Surface>>) -> Result<Self> {
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let archived = Arc::new(Mutex::new(HashSet::new()));
        let flag = Arc::clone(&stop);
        let state = Arc::clone(&archived);
        let blocked = Arc::new(Mutex::new(None));
        let gate = Arc::clone(&blocked);
        let discovery = Arc::new((Mutex::new(true), Condvar::new()));
        let scans = Arc::clone(&discovery);
        let worker = std::thread::spawn(move || {
            let mut clients = Vec::new();
            while !flag.load(Ordering::Relaxed) {
                if let Ok((stream, _)) = listener.accept() {
                    let state = Arc::clone(&state);
                    let gate = Arc::clone(&gate);
                    let surface = Arc::clone(&surface);
                    let discovery = Arc::clone(&scans);
                    clients.push(std::thread::spawn(move || {
                        engine_connection(stream, state, gate, surface, discovery)
                    }));
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            for client in clients {
                client.join().unwrap().unwrap();
            }
        });
        Ok(Self {
            archived,
            stop,
            worker: Some(worker),
            blocked,
            discovery,
        })
    }
}

fn engine_connection(
    stream: UnixStream,
    archived: Arc<Mutex<HashSet<String>>>,
    blocked: Arc<Mutex<Option<BlockingMove>>>,
    surface: Arc<Mutex<Surface>>,
    discovery: Arc<(Mutex<bool>, Condvar)>,
) -> Result<()> {
    if !super::transport::prepare_native_stream(&stream, Duration::from_secs(10))? {
        return Ok(());
    }
    let mut socket = tungstenite::accept(stream)?;
    while let Ok(message) = socket.read() {
        if message.is_close() {
            break;
        }
        let request: Value = serde_json::from_str(message.to_text()?)?;
        if request["id"].is_null() {
            continue;
        }
        if request["method"] == "thread/list" && !archived.lock().unwrap().is_empty() {
            let _released = discovery
                .1
                .wait_while(discovery.0.lock().unwrap(), |released| !*released)
                .unwrap();
        }
        let identifier = request["params"]["threadId"].as_str().unwrap_or(FIRST);
        let block = blocked.lock().unwrap().clone().filter(|block| {
            request["method"] == "thread/archive" && block.identifier == identifier
        });
        wait_for_release(block)?;
        if request["method"] == "thread/read" {
            let block = surface.lock().unwrap().read_blocked.clone()
                .filter(|block| block.identifier == identifier);
            wait_for_release(block)?;
        }
        let Some(response) = engine_response(&request, &archived, &mut surface.lock().unwrap()) else {
            return Ok(());
        };
        if request["method"] == "thread/list" && request["params"]["currentAccountOnly"] == false
            && request["params"]["archived"] == false
        {
            let page = surface.lock().unwrap().runtime_page_reply.take();
            wait_for_release(page)?;
        }
        socket.send(tungstenite::Message::Text(
            response.to_string().into(),
        ))?;
    }
    Ok(())
}

fn engine_response(
    request: &Value,
    archived: &Mutex<HashSet<String>>,
    surface: &mut Surface,
) -> Option<Value> {
    let response = match engine_reply(request, archived, surface) {
        Ok(result) => json!({"id":request["id"],"result":result}),
        Err(error) => json!({"id":request["id"],
            "error":{"code":-32601,"message":error.to_string()}}),
    };
    if request["method"] == "thread/archive"
        && std::mem::take(&mut surface.lose_archive_receipt) {
        return None;
    }
    Some(response)
}

fn engine_reply(
    request: &Value,
    archived: &Mutex<HashSet<String>>,
    surface: &mut Surface,
) -> Result<Value> {
    let identifier = request["params"]["threadId"].as_str().unwrap_or(FIRST);
    let method = request["method"].as_str().unwrap_or("");
    surface.calls.push(method.into());
    Ok(match method {
        "thread/list" => {
            if let Some(unreadable) = &surface.unreadable_section {
                anyhow::ensure!(!surface.slots.values().any(|slot| slot == unreadable),
                    "fixture: native list unavailable");
            }
            let desired = request["params"]["archived"] == true;
            let values: Vec<_> = surface
                .slots
                .keys()
                .filter(|identifier| archived.lock().unwrap().contains(*identifier) == desired)
                .map(|identifier| {
                    json!({"id":identifier,"status":{"type":"idle"},"updatedAt":1,
                    "section":surface.backend_section(identifier)})
                })
                .collect();
            json!({"data":values})
        }
        "threadSection/list" => surface.catalog(),
        "thread/read" => {
            if let Some(unreadable) = &surface.unreadable_section {
                anyhow::ensure!(surface.slots.get(identifier) != Some(unreadable),
                    "fixture: native position unavailable");
            }
            let mut thread = surface.probe_threads.get(identifier).cloned().unwrap_or(
                json!({"id":identifier,"status":{"type":surface.thread_status.as_deref().unwrap_or("idle")}}));
            thread["section"] = surface.backend_section(identifier);
            json!({"thread":thread})
        }
        "thread/start" | "thread/name/set" => probe_reply(request,surface)?,
        "thread/turns/list" => turns_list(&request["params"], identifier, surface),
        "thread/archive" => {
            surface.archive_writes.push(identifier.into());
            archived.lock().unwrap().insert(identifier.into());
            json!({})
        }
        _ => return archive_identity_reply(method, surface),
    })
}

fn turns_list(params: &Value, identifier: &str, surface: &Surface) -> Value {
    if params["itemsView"] == "notLoaded" && let Some(revision) = &surface.preview_revision {
        return json!({"data":[revision]});
    }
    if params["itemsView"] != "summary" {
        assert_eq!(params["itemsView"], "full");
    }
    let turns = surface.probe_threads.get(identifier)
        .map(|thread| thread["turns"].as_array().unwrap().clone())
        .unwrap_or_else(|| surface.latest_turn.iter().cloned().collect::<Vec<_>>());
    json!({"data":turns})
}

fn archive_identity_reply(method: &str, surface: &Surface) -> Result<Value> {
    match method {
        "initialize" => {
            let home = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from);
            Ok(json!({"codexHome":home.and_then(|home| home.canonicalize().ok())}))
        }
        "account/read" if surface.archive_account.is_some() =>
            Ok(json!({"userId":surface.archive_account,"workspaceRouting":{"chatgptAccountId":"fixture"}})),
        _ => anyhow::bail!("unexpected engine method {method}"),
    }
}

fn probe_reply(request: &Value, surface: &mut Surface) -> Result<Value> {
    if request["method"] == "thread/start" {
        let id = uuid::Uuid::new_v4().to_string();
        let thread = json!({"id":id,"name":null,"cwd":request["params"]["cwd"],
            "historyMode":request["params"]["historyMode"],"turns":[],"status":{"type":"notLoaded"}});
        surface.slots.insert(id.clone(),"unsectioned".into());
        surface.probe_threads.insert(id,thread.clone());
        Ok(json!({"thread":thread}))
    } else {
        let identifier = request["params"]["threadId"].as_str().context("missing probe")?;
        surface.probe_threads.get_mut(identifier).context("unknown probe")?["name"] =
            request["params"]["name"].clone();
        Ok(json!({}))
    }
}

#[test]
fn unsupported_engine_methods_return_errors_without_disconnecting() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("engine.sock");
    let _engine = Engine::new(&path, Arc::new(Mutex::new(Surface::default())))?;
    let stream = UnixStream::connect(&path)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let (mut client, _) = tungstenite::client("ws://localhost", stream)?;
    client.send(tungstenite::Message::Text(
        json!({"id":1,"method":"account/read","params":{"refreshToken":false}})
            .to_string().into(),
    ))?;
    let rejected: Value = serde_json::from_str(client.read()?.to_text()?)?;
    assert_eq!(rejected["id"], 1);
    assert_eq!(rejected["error"]["code"], -32601);
    assert!(rejected.get("result").is_none());
    client.send(tungstenite::Message::Text(
        json!({"id":2,"method":"thread/list","params":{"archived":false}})
            .to_string().into(),
    ))?;
    let accepted: Value = serde_json::from_str(client.read()?.to_text()?)?;
    assert_eq!(accepted["id"], 2);
    assert_eq!(accepted["result"]["data"], json!([]));
    assert!(accepted.get("error").is_none());
    client.close(None)?;
    Ok(())
}

#[test]
fn native_archive_keeps_its_configured_budget_during_slow_section_verification() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.rpc_timeout_seconds = 6;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let engine = Engine::new(&server.config.desktop_socket, Arc::clone(&server.surface))?;
    let app = Arc::new(server.app()?);
    app.request("/api/state", Some(&body(&app, "to_archive", "保留备注")?))?;
    let (sender, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    server.surface.lock().unwrap().read_blocked = Some(BlockingMove {
        identifier: FIRST.into(),
        started: sender,
        release: Arc::clone(&release),
    });
    let request = body(&app, "to_archive", "保留备注")?;
    let archiving = Arc::clone(&app);
    let worker = std::thread::spawn(move || archiving.request("/api/archive", Some(&request)));
    started.recv_timeout(Duration::from_secs(1))?;
    std::thread::sleep(Duration::from_millis(3200));
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    worker.join().unwrap()?;
    assert!(engine.archived.lock().unwrap().contains(FIRST));
    assert_eq!(app.store.note(FIRST)?, "保留备注");
    assert!(app.snapshot()?["sessions"].as_array().unwrap().is_empty());
    Ok(())
}

#[test]
fn a_slow_native_archive_does_not_hold_the_database_lock_for_other_chats() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let engine = Engine::new(&server.config.desktop_socket, Arc::clone(&server.surface))?;
    let app = Arc::new(server.app()?);
    app.discover(&[json!({"id":FIRST}), json!({"id":SECOND})], true)?;
    {
        let mut surface = server.surface.lock().unwrap();
        surface.slots.insert(FIRST.into(), "to_archive".into());
        surface.slots.insert(SECOND.into(), "review".into());
    }
    app.refresh_native()?;
    let (started_sender, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    *engine.blocked.lock().unwrap() = Some(BlockingMove {
        identifier: FIRST.into(),
        started: started_sender,
        release: Arc::clone(&release),
    });
    let archiving = Arc::clone(&app);
    let request = body(&app, "to_archive", "")?;
    let first = std::thread::spawn(move || archiving.request("/api/archive", Some(&request)));
    let began = started.recv_timeout(Duration::from_secs(1));
    let (finished_sender, finished) = mpsc::channel();
    let saving = Arc::clone(&app);
    let second = std::thread::spawn(move || {
        finished_sender.send(saving.request("/api/state",Some(&json!({"id":SECOND,"state":"to_archive","note":"独立保存","version":1,"inputVersion":0,"sectionVersion":section_id("review")})))).unwrap()
    });
    let result = finished.recv_timeout(Duration::from_secs(1));
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    began?;
    first.join().unwrap()?;
    second.join().unwrap();
    result??;
    assert_eq!(app.store.note(SECOND)?, "独立保存");
    assert!(engine.archived.lock().unwrap().contains(FIRST));
    Ok(())
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

#[test]
fn native_archive_rejects_external_moves_before_archiving_and_accepts_fresh_confirmation()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let engine = Engine::new(&server.config.desktop_socket, Arc::clone(&server.surface))?;
    let app = server.app()?;
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "to_archive".into());
    app.refresh_native()?;
    let stale = body(&app, "to_archive", "")?;
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "review".into());
    assert!(app.request("/api/archive", Some(&stale)).is_err());
    assert!(engine.archived.lock().unwrap().is_empty());
    assert_eq!(app.snapshot()?["sessions"][0]["id"], FIRST);
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "to_archive".into());
    app.refresh_native()?;
    app.request("/api/archive", Some(&body(&app, "to_archive", "")?))?;
    assert!(engine.archived.lock().unwrap().contains(FIRST));
    assert!(app.snapshot()?["sessions"].as_array().unwrap().is_empty());
    Ok(())
}

#[test]
fn a_blocked_move_in_another_chat_does_not_block_native_archive() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.rpc_timeout_seconds = 6;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let engine = Engine::new(&server.config.desktop_socket, Arc::clone(&server.surface))?;
    let app = Arc::new(server.app()?);
    app.discover(&[json!({"id":FIRST}), json!({"id":SECOND})], true)?;
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(SECOND.into(), "to_archive".into());
    app.refresh_native()?;
    let (started_sender, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    server.surface.lock().unwrap().blocked = Some(BlockingMove {
        identifier: FIRST.into(),
        started: started_sender,
        release: Arc::clone(&release),
    });
    let saving = Arc::clone(&app);
    let move_body = body(&app, "to_archive", "正在保存")?;
    let first = std::thread::spawn(move || saving.request("/api/state", Some(&move_body)));
    started.recv_timeout(Duration::from_secs(1))?;
    let (finished_sender, finished) = mpsc::channel();
    let archiving = Arc::clone(&app);
    let second = std::thread::spawn(move || {
        finished_sender.send(archiving.request("/api/archive",Some(&json!({"id":SECOND,"version":1,"inputVersion":0,"sectionVersion":section_id("to_archive")})))).unwrap()
    });
    let result = finished.recv_timeout(Duration::from_secs(1));
    std::thread::sleep(Duration::from_millis(3200));
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    first.join().unwrap()?;
    second.join().unwrap();
    result??;
    assert!(engine.archived.lock().unwrap().contains(SECOND));
    assert!(!engine.archived.lock().unwrap().contains(FIRST));
    Ok(())
}
