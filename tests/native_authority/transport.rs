use super::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::time::Duration;

pub(super) fn section_id(state: &str) -> String {
    let suffix = match state {
        "unknown" => 1,
        "in_progress" => 2,
        "review" => 3,
        "blocked" => 4,
        _ => 5,
    };
    format!("00000000-0000-4000-8000-{suffix:012}")
}

#[derive(Default)]
pub(super) struct Surface {
    pub latest_turn: Option<Value>,
    pub thread_status: Option<String>,
    pub slots: BTreeMap<String, String>,
    pub calls: Vec<String>,
    pub notes_during_move: Vec<String>,
    pub lose_receipt: bool,
    pub reject_move: Option<String>,
    pub blocked: Option<BlockingMove>,
    pub read_blocked: Option<BlockingMove>,
    pub remote_projects: Vec<Value>,
    pub remote_threads: Vec<Value>,
    pub remote_disconnected: bool,
    pub remote_blocked: Option<BlockingMove>,
}

#[derive(Clone)]
pub(super) struct BlockingMove {
    pub identifier: String,
    pub started: Sender<()>,
    pub release: Arc<(Mutex<bool>, Condvar)>,
}

pub(super) fn wait_for_release(blocked: Option<BlockingMove>) -> Result<()> {
    if let Some(blocked) = blocked {
        blocked.started.send(())?;
        let (lock, signal) = &*blocked.release;
        let _waiting = signal
            .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(10), |released| {
                !*released
            })
            .unwrap();
    }
    Ok(())
}

impl Surface {
    pub(super) fn catalog(&self) -> Value {
        let sections: Vec<_> = self.layout()["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|section| {
                json!({"id":format!("backend:{}",section["sectionId"].as_str().unwrap()),
                "name":section["name"],"appearance":null})
            })
            .collect();
        json!({"data":sections})
    }

    pub(super) fn backend_section(&self, identifier: &str) -> Value {
        self.slots
            .get(identifier)
            .map(|state| {
                let label = codex_session_board::model::board_states()
                    .find(|(value, _)| *value == state)
                    .unwrap()
                    .1;
                json!({"id":format!("backend:{}",section_id(state)),"name":label,"appearance":null})
            })
            .unwrap_or(Value::Null)
    }

    pub(super) fn layout(&self) -> Value {
        let sections: Vec<_> = codex_session_board::model::board_states()
            .map(|(state, label)| {
                let members: Vec<_> = self
                    .slots
                    .iter()
                    .filter(|(_, slot)| *slot == state)
                    .map(|(identifier, _)| format!("codex:thread:local:{identifier}"))
                    .collect();
                json!({"sectionId":section_id(state),"name":label,"itemKeys":members})
            })
            .collect();
        json!({"unavailableHosts":[],"sections":sections})
    }

    fn history(&self, args: &Value) -> Result<Option<Value>> {
        if self.remote_disconnected {
            return Ok(None);
        }
        assert_eq!(args["source"], "codex");
        assert_ne!(args["hostId"], "local");
        let threads: Vec<_> = self
            .remote_threads
            .iter()
            .filter(|thread| thread["hostId"] == args["hostId"])
            .cloned()
            .collect();
        Ok(Some(json!({"threads":threads,"nextCursor":null})))
    }

    fn call(
        &mut self,
        method: &str,
        args: &Value,
        database: &std::path::Path,
    ) -> Result<Option<Value>> {
        self.calls
            .push(if method == "list_threads" && args["mode"] == "history" {
                format!("history:{}", args["hostId"].as_str().unwrap())
            } else {
                method.into()
            });
        match method {
            "list_projects" => Ok(Some(json!({"projects":self.remote_projects}))),
            "list_threads" if args["mode"] == "history" => self.history(args),
            "list_threads" => Ok(Some(self.layout())),
            "move_thread_to_sidebar_section" => {
                assert_eq!(args["hostId"], "local");
                assert_eq!(args["source"], "codex");
                let identifier = args["threadId"].as_str().unwrap_or("");
                if self.reject_move.as_deref() == Some(identifier) {
                    return Ok(None);
                }
                let state = codex_session_board::model::board_states()
                    .find(|(state, _)| args["sectionId"] == section_id(state))
                    .ok_or_else(|| anyhow::anyhow!("unknown section"))?
                    .0;
                let connection = rusqlite::Connection::open(database)?;
                self.notes_during_move.push(connection.query_row(
                    "SELECT note FROM sessions WHERE id=?",
                    [identifier],
                    |row| row.get(0),
                )?);
                self.slots.insert(identifier.into(), state.into());
                if std::mem::take(&mut self.lose_receipt) {
                    return Ok(None);
                }
                Ok(Some(json!({})))
            }
            _ => anyhow::bail!("unexpected native method: {method}"),
        }
    }
}

pub(super) struct Server {
    pub surface: Arc<Mutex<Surface>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    pub config: Config,
    _engine: super::archive::Engine,
}

impl Server {
    pub fn new(directory: &std::path::Path) -> Result<Self> {
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
        let socket = directory.join("sidebar.sock");
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let config = Config {
            data_dir: directory.join("data"),
            native_sections: true,
            sync_titles: false,
            sidebar_socket: socket,
            sidebar_caller: FIRST.into(),
            sidebar_state_file: write_mapping(directory)?,
            status_sections: codex_session_board::model::board_states()
                .map(|(state, _)| (state.into(), section_id(state)))
                .collect(),
            rpc_timeout_seconds: 2,
            desktop_socket: directory.join("desktop.sock"),
            codex_command: vec!["/bin/false".into()],
            open_command: vec!["/bin/true".into()],
            ..Config::default()
        };
        config.validate()?;
        let surface = Arc::new(Mutex::new(Surface::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let engine = super::archive::Engine::new(&config.desktop_socket, Arc::clone(&surface))?;
        let worker = serve(
            listener,
            Arc::clone(&surface),
            Arc::clone(&stop),
            config.data_dir.join("status.sqlite3"),
        );
        Ok(Self {
            surface,
            stop,
            worker: Some(worker),
            config,
            _engine: engine,
        })
    }

    pub fn app(&self) -> Result<App> {
        let app = App::new(self.config.clone())?;
        app.discover(&[json!({"id":FIRST})], true)?;
        self.surface
            .lock()
            .unwrap()
            .slots
            .insert(FIRST.into(), "review".into());
        app.refresh_native()?;
        Ok(app)
    }
}

fn write_mapping(directory: &std::path::Path) -> Result<std::path::PathBuf> {
    let path = directory.join("aliases.json");
    let sections: Vec<_> = codex_session_board::model::board_states()
        .map(|(state, _)| {
            json!({"id":section_id(state),
            "hostSectionIds":{"local":format!("backend:{}",section_id(state))}})
        })
        .collect();
    std::fs::write(
        &path,
        json!({"electron-persisted-atom-state":{
            "sidebar-custom-sections-v3":{"account":{"sections":sections}}
        }})
        .to_string(),
    )?;
    Ok(path)
}

fn serve(
    listener: UnixListener,
    surface: Arc<Mutex<Surface>>,
    stop: Arc<AtomicBool>,
    database: std::path::PathBuf,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut clients = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            if let Ok((stream, _)) = listener.accept() {
                let surface = Arc::clone(&surface);
                let database = database.clone();
                clients.push(std::thread::spawn(move || {
                    connection(stream, surface, &database)
                }));
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        for client in clients {
            client.join().unwrap().unwrap();
        }
    })
}

fn blocking_request(surface: &Surface, params: &Value) -> Option<BlockingMove> {
    if params["tool"] == "list_projects" {
        return surface
            .remote_blocked
            .clone()
            .filter(|gate| gate.identifier == "projects");
    }
    if params["tool"] == "list_threads" && params["arguments"]["mode"] == "history" {
        return surface
            .remote_blocked
            .clone()
            .filter(|gate| params["arguments"]["hostId"] == gate.identifier);
    }
    match params["tool"].as_str().unwrap_or("") {
        "list_threads" => surface.read_blocked.clone(),
        "move_thread_to_sidebar_section" => surface
            .blocked
            .clone()
            .filter(|blocked| params["arguments"]["threadId"] == blocked.identifier),
        _ => None,
    }
}

fn connection(
    mut stream: UnixStream,
    surface: Arc<Mutex<Surface>>,
    database: &std::path::Path,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    loop {
        let mut size = [0u8; 4];
        if stream.read_exact(&mut size).is_err() {
            return Ok(());
        }
        let mut bytes = vec![0; u32::from_le_bytes(size) as usize];
        stream.read_exact(&mut bytes)?;
        let request: Value = serde_json::from_slice(&bytes)?;
        let params = &request["params"];
        let block = blocking_request(&surface.lock().unwrap(), params);
        wait_for_release(block)?;
        let result = surface.lock().unwrap().call(
            params["tool"].as_str().unwrap_or(""),
            &params["arguments"],
            database,
        )?;
        let Some(result) = result else { return Ok(()) };
        let reply = json!({"id":request["id"],"result":{"success":true,"contentItems":[{"type":"inputText","text":result.to_string()}]}});
        let bytes = serde_json::to_vec(&reply)?;
        stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
        stream.write_all(&bytes)?;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

pub(super) fn body(app: &App, state: &str, note: &str) -> Result<Value> {
    let snapshot = app.snapshot()?;
    let session = snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["id"] == FIRST)
        .unwrap();
    Ok(
        json!({"id":FIRST,"state":state,"note":note,"version":session["version"],"inputVersion":session["inputVersion"],"sectionVersion":session["sectionVersion"]}),
    )
}

#[test]
fn direct_transport_reads_shared_backend_positions_without_sidebar_projection() -> Result<()> {
    use codex_session_board::{
        native_sections::direct,
        rpc::{DesktopConnection, Role},
    };
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let _app = server.app()?;
    server.surface.lock().unwrap().calls.clear();
    let mut reader = DesktopConnection::connect(
        &server.config.desktop_socket,
        Duration::from_secs(2),
        Role::NativeReader,
    )?;
    assert_eq!(
        direct::read(&mut reader, &server.config, FIRST)?.version,
        section_id("review")
    );
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "to_archive".into());
    assert_eq!(
        direct::read(&mut reader, &server.config, FIRST)?.version,
        section_id("to_archive")
    );
    assert!(
        !server
            .surface
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|method| method == "list_threads")
    );
    Ok(())
}

#[test]
fn native_save_writes_remote_before_committing_notes_and_confirmation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    let request = body(&app, "to_archive", "新备注")?;
    app.request("/api/state", Some(&request))?;
    let surface = server.surface.lock().unwrap();
    assert_eq!(surface.slots[FIRST], "to_archive");
    assert_eq!(surface.notes_during_move, [""]);
    assert_eq!(app.store.note(FIRST)?, "新备注");
    assert_eq!(
        app.snapshot()?["sessions"][0]["sectionVersion"],
        section_id("to_archive")
    );
    Ok(())
}

#[test]
fn external_move_rejects_an_unobserved_stale_save_and_fresh_save_succeeds() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    let stale = body(&app, "review", "旧操作")?;
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "to_archive".into());
    assert!(app.request("/api/state", Some(&stale)).is_err());
    assert_eq!(app.store.note(FIRST)?, "");
    assert!(
        !server
            .surface
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|method| method == "move_thread_to_sidebar_section")
    );
    app.refresh_native()?;
    app.request("/api/state", Some(&body(&app, "review", "新操作")?))?;
    assert_eq!(server.surface.lock().unwrap().slots[FIRST], "review");
    assert_eq!(app.store.note(FIRST)?, "新操作");
    Ok(())
}

#[test]
fn lost_move_receipt_is_read_back_without_replaying_the_move() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server = Server::new(directory.path())?;
    let app = server.app()?;
    server.surface.lock().unwrap().lose_receipt = true;
    app.request("/api/state", Some(&body(&app, "to_archive", "回执已核对")?))?;
    let surface = server.surface.lock().unwrap();
    assert_eq!(
        surface
            .calls
            .iter()
            .filter(|method| method.as_str() == "move_thread_to_sidebar_section")
            .count(),
        1
    );
    assert_eq!(surface.slots[FIRST], "to_archive");
    assert_eq!(app.store.note(FIRST)?, "回执已核对");
    Ok(())
}
