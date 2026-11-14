use super::*;
use std::os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}};
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

struct Backend {
    socket: PathBuf,
    phase: Arc<Mutex<(String, String, u64)>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<()>>>,
}

impl Backend {
    fn start(root: &Path) -> Result<Self> {
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        let socket = root.join("shared.sock");
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let phase = Arc::new(Mutex::new(("active".into(), "inProgress".into(), 1)));
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::clone(&phase);
        let stopping = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            let mut clients = Vec::new();
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&state);
                        clients.push(std::thread::spawn(move || connection(stream, state)));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            for client in clients {
                client.join().map_err(|_| anyhow::anyhow!("测试后台连接线程失败"))??;
            }
            Ok(())
        });
        Ok(Self { socket, phase, stop, worker: Some(worker) })
    }

    fn plugin(&self, profile: &Profile) -> Result<Plugin> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-session-board"));
        command.env("CODEX_APP_SERVER_WS_URL", format!("ws+unix://localhost{}:/rpc", self.socket.display()));
        Plugin::launch(command, Some(&profile.path), None)
    }

    fn set(&self, kind: &str, phase: &str) {
        let mut state = self.phase.lock().unwrap();
        *state = (kind.into(), phase.into(), state.2 + 1);
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap().unwrap();
        }
    }
}

fn connection(stream: UnixStream, phase: Arc<Mutex<(String, String, u64)>>) -> Result<()> {
    stream.set_nonblocking(false)?;
    let mut socket = tungstenite::accept(stream)?;
    while let Ok(message) = socket.read() {
        if message.is_close() { break; }
        let request: Value = serde_json::from_slice(&message.into_data())?;
        if request["id"].is_null() { continue; }
        let state = phase.lock().unwrap().clone();
        let started = codex_session_board::model::now() + 1.0;
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => json!({}),
            "thread/list" => json!({"data":[{"id":THREAD,"name":"new task","updatedAt":state.2,
                "status":{"type":state.0}}],"nextCursor":null}),
            "thread/turns/list" => json!({"data":[{"id":"turn-1","status":state.1,
                "startedAt":started,"completedAt":null,"items":[]}],"nextCursor":null}),
            method => anyhow::bail!("unexpected method {method}"),
        };
        socket.send(tungstenite::Message::Text(
            json!({"id":request["id"],"result":result}).to_string().into()))?;
    }
    Ok(())
}

fn until(plugin: &mut Plugin, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let snapshot = plugin.snapshot()?;
        if predicate(&snapshot) { return Ok(snapshot); }
        ensure!(Instant::now() < deadline, "未观察到目标状态：{snapshot}");
        std::thread::sleep(Duration::from_millis(80));
    }
}

#[test]
fn inherited_shared_backend_drives_new_tasks_and_unloaded_projection_cannot_block_them() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut profile = Profile::new(directory.path())?;
    profile.config.runtime_poll_seconds = 1;
    profile.config.desktop_socket = directory.path().join("stale-daemon.sock");
    std::fs::write(&profile.path, serde_json::to_vec(&profile.config)?)?;
    let backend = Backend::start(directory.path())?;
    let mut plugin = backend.plugin(&profile)?;
    let active = until(&mut plugin, |snapshot| snapshot["sessions"][0]["state"] == "in_progress")?;
    assert_eq!(active["health"]["runtimeVerified"], true);
    assert_eq!(active["runtime"]["authority"]["pid"], std::process::id());
    backend.set("notLoaded", "interrupted");
    let unloaded = until(&mut plugin, |snapshot| snapshot["sessions"][0]["turnStatus"] == "interrupted")?;
    assert_eq!(unloaded["sessions"][0]["state"], "in_progress");
    backend.set("idle", "completed");
    until(&mut plugin, |snapshot| snapshot["sessions"][0]["state"] == "review")?;
    backend.set("idle", "interrupted");
    until(&mut plugin, |snapshot| snapshot["sessions"][0]["state"] == "blocked")?;
    plugin.close()
}

#[test]
fn a_different_client_parent_cannot_reuse_an_existing_board_owner() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = Profile::new(directory.path())?;
    let mut owner = Plugin::start(Some(&profile.path), None)?;
    owner.snapshot()?;
    let mut command = Command::new("python3");
    command.args(["-B", "-c", "import subprocess,sys\nchild=subprocess.Popen(sys.argv[1:])\nsys.exit(child.wait())"])
        .arg(env!("CARGO_BIN_EXE_codex-session-board"))
        .env_remove("CODEX_APP_SERVER_WS_URL");
    let mut other = Plugin::launch(command, Some(&profile.path), None)?;
    let response = other.call("board_snapshot", json!({}))?;
    assert_eq!(response["isError"], true);
    assert!(response["content"][0]["text"].as_str().unwrap_or("").contains("其他客户端后台"), "{response}");
    other.close()?;
    owner.close()
}
