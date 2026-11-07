use super::transport::{BlockingMove, Server, body, section_id, wait_for_release};
use super::*;
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

pub(super) struct Engine {
    pub archived: Arc<Mutex<HashSet<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    pub blocked: Arc<Mutex<Option<BlockingMove>>>,
}

impl Engine {
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let archived = Arc::new(Mutex::new(HashSet::new()));
        let flag = Arc::clone(&stop);
        let state = Arc::clone(&archived);
        let blocked = Arc::new(Mutex::new(None));
        let gate = Arc::clone(&blocked);
        let worker = std::thread::spawn(move || {
            let mut clients = Vec::new();
            while !flag.load(Ordering::Relaxed) {
                if let Ok((stream, _)) = listener.accept() {
                    let state = Arc::clone(&state);
                    let gate = Arc::clone(&gate);
                    clients.push(std::thread::spawn(move || {
                        engine_connection(stream, state, gate)
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
        })
    }
}

fn engine_connection(
    stream: UnixStream,
    archived: Arc<Mutex<HashSet<String>>>,
    blocked: Arc<Mutex<Option<BlockingMove>>>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut socket = tungstenite::accept(stream)?;
    while let Ok(message) = socket.read() {
        if message.is_close() {
            break;
        }
        let request: Value = serde_json::from_str(message.to_text()?)?;
        if request["id"].is_null() {
            continue;
        }
        let identifier = request["params"]["threadId"].as_str().unwrap_or(FIRST);
        let block = blocked.lock().unwrap().clone().filter(|block| {
            request["method"] == "thread/archive" && block.identifier == identifier
        });
        wait_for_release(block)?;
        let result = engine_reply(&request, &archived)?;
        socket.send(tungstenite::Message::Text(
            json!({"id":request["id"],"result":result})
                .to_string()
                .into(),
        ))?;
    }
    Ok(())
}

fn engine_reply(request: &Value, archived: &Mutex<HashSet<String>>) -> Result<Value> {
    let identifier = request["params"]["threadId"].as_str().unwrap_or(FIRST);
    Ok(match request["method"].as_str().unwrap_or("") {
        "initialize" => json!({}),
        "thread/list" => {
            let desired = request["params"]["archived"] == true;
            let values: Vec<_> = [FIRST, SECOND]
                .into_iter()
                .filter(|identifier| archived.lock().unwrap().contains(*identifier) == desired)
                .map(|identifier| json!({"id":identifier,"status":{"type":"idle"},"updatedAt":1}))
                .collect();
            json!({"data":values})
        }
        "thread/read" => json!({"thread":{"id":identifier,"status":{"type":"idle"}}}),
        "thread/turns/list" => {
            if request["params"]["itemsView"] != "summary" {
                assert_eq!(request["params"]["itemsView"], "full");
            }
            json!({"data":[]})
        }
        "thread/archive" => {
            archived.lock().unwrap().insert(identifier.into());
            json!({})
        }
        method => anyhow::bail!("unexpected engine method {method}"),
    })
}

#[test]
fn a_slow_native_archive_does_not_hold_the_database_lock_for_other_chats() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let engine = Engine::new(&server.config.desktop_socket)?;
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
    let engine = Engine::new(&server.config.desktop_socket)?;
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
    server.config.desktop_socket = directory.path().join("engine.sock");
    let engine = Engine::new(&server.config.desktop_socket)?;
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
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    first.join().unwrap()?;
    second.join().unwrap();
    result??;
    assert!(engine.archived.lock().unwrap().contains(SECOND));
    assert!(!engine.archived.lock().unwrap().contains(FIRST));
    Ok(())
}
