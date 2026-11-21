use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use std::os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}};
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

pub struct Backend {
    pub socket: PathBuf,
    phase: Arc<Mutex<(String, String, u64)>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<()>>>,
}

impl Backend {
    pub fn start(root: &Path, thread_id: &'static str) -> Result<Self> {
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
                        clients.push(std::thread::spawn(move || connection(stream, state, thread_id)));
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

    pub fn set(&self, kind: &str, phase: &str) -> Result<()> {
        let mut state = self.phase.lock().map_err(|_| anyhow::anyhow!("测试状态锁已损坏"))?;
        *state = (kind.into(), phase.into(), state.2 + 1);
        Ok(())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let result = worker.join().map_err(|_| anyhow::anyhow!("测试后台线程失败"))
                .and_then(|result| result);
            assert!(result.is_ok(), "{result:?}");
        }
    }
}

fn connection(stream: UnixStream, phase: Arc<Mutex<(String, String, u64)>>, thread_id: &str) -> Result<()> {
    stream.set_nonblocking(false)?;
    let mut socket = tungstenite::accept(stream)?;
    while let Ok(message) = socket.read() {
        if message.is_close() { break; }
        let request: Value = serde_json::from_slice(&message.into_data())?;
        if request["id"].is_null() { continue; }
        let state = phase.lock().map_err(|_| anyhow::anyhow!("测试状态锁已损坏"))?.clone();
        let started = codex_session_board::model::now() + 1.0;
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => json!({}),
            "thread/list" => json!({"data":[{"id":thread_id,"name":"new task","updatedAt":state.2,
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
