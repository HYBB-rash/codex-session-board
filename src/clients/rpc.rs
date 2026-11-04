use super::{ExternalError, external, required};
use crate::common::command;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

const FRAME_LIMIT: usize = 8 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub(super) struct RpcFailure {
    pub response: Value,
}

impl std::fmt::Display for RpcFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "RPC error {}", self.response["error"]["code"])
    }
}

impl std::error::Error for RpcFailure {}

#[derive(Clone)]
enum Endpoint {
    Process(Vec<String>),
    WebSocket(PathBuf),
}

#[derive(Default)]
struct Inbox {
    responses: HashMap<String, Value>,
    response_history: Vec<Value>,
    notifications: Vec<Value>,
    attention: Option<Value>,
    failure: Option<String>,
    stderr: Vec<u8>,
}

type Shared = Arc<(Mutex<Inbox>, Condvar)>;

#[derive(Clone)]
enum Sink {
    Rpc(Shared),
    Bridge(mpsc::Sender<WireEvent>),
}

enum WireEvent {
    Frame(Vec<u8>),
    End(Option<String>),
}

impl Sink {
    fn frame(&self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Rpc(shared) => accept_frame(shared, bytes),
            Self::Bridge(sender) => {
                sender.send(WireEvent::Frame(bytes.to_vec()))?;
                Ok(())
            }
        }
    }

    fn end(&self, kind: Option<&str>) {
        match self {
            Self::Rpc(shared) => {
                let mut inbox = shared.0.lock().unwrap();
                if inbox.failure.is_none() {
                    inbox.failure = Some(kind.unwrap_or("connection_closed").to_owned());
                }
                shared.1.notify_all();
            }
            Self::Bridge(sender) => {
                let _ = sender.send(WireEvent::End(kind.map(str::to_owned)));
            }
        }
    }
}

struct Packet {
    bytes: Vec<u8>,
    acknowledgement: mpsc::Sender<std::result::Result<(), String>>,
}

struct Connection {
    writer: mpsc::SyncSender<Packet>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    child: Option<Child>,
    socket: Option<UnixStream>,
}

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(socket) = &self.socket {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(child) = &mut self.child {
            if let Ok(group) = i32::try_from(child.id()) {
                unsafe {
                    kill(-group, 15);
                }
                let until = Instant::now() + Duration::from_millis(100);
                while Instant::now() < until && matches!(child.try_wait(), Ok(None)) {
                    thread::sleep(Duration::from_millis(5));
                }
                unsafe {
                    kill(-group, 9);
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        let until = Instant::now() + Duration::from_millis(200);
        for worker in self.threads.drain(..) {
            while !worker.is_finished() && Instant::now() < until {
                thread::sleep(Duration::from_millis(2));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

fn seconds(value: f64) -> Result<Duration> {
    ensure!(
        value.is_finite() && value > 0.0,
        "Timeout must be positive and finite"
    );
    Duration::try_from_secs_f64(value).context("Timeout is too large")
}

fn deadline(duration: Duration) -> Result<Instant> {
    Instant::now()
        .checked_add(duration)
        .context("Timeout is too large")
}

fn remaining(until: Instant) -> Result<Duration> {
    until
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .context("Deadline elapsed")
}

fn accept_frame(shared: &Shared, bytes: &[u8]) -> Result<()> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }
    let message: Value = serde_json::from_slice(bytes)
        .map_err(|_| external("receive", "invalid_utf8_json", false, false))?;
    ensure_protocol(message.is_object(), "frame_not_object")?;
    ensure_protocol(
        message.get("jsonrpc").is_none() || message["jsonrpc"] == "2.0",
        "unsupported_jsonrpc",
    )?;
    if let Some(identifier) = message.get("id") {
        ensure_protocol(
            identifier.is_string() || identifier.is_i64() || identifier.is_u64(),
            "invalid_rpc_id",
        )?;
    }
    let mut inbox = shared.0.lock().unwrap();
    if let Some(method) = message.get("method") {
        ensure_protocol(
            method.as_str().is_some_and(|text| !text.is_empty()),
            "invalid_rpc_method",
        )?;
        if message.get("id").is_some() && inbox.attention.is_none() {
            inbox.attention = Some(message.clone());
        }
        inbox.notifications.push(message);
    } else if message.get("id").is_some()
        && (message.get("result").is_some() != message.get("error").is_some())
    {
        let key = serde_json::to_string(&message["id"])?;
        inbox.responses.insert(key, message.clone());
        inbox.response_history.push(message);
    } else {
        return Err(external("receive", "invalid_rpc_frame", false, false));
    }
    shared.1.notify_all();
    Ok(())
}

fn ensure_protocol(condition: bool, kind: &str) -> Result<()> {
    if !condition {
        return Err(external("receive", kind, false, false));
    }
    Ok(())
}

fn read_lines(reader: impl Read, limit: usize, sink: Sink) {
    let mut reader = io::BufReader::new(reader);
    let mut pending = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok(bytes) => bytes,
            Err(_) => {
                sink.end(Some("connection_read"));
                return;
            }
        };
        if available.is_empty() {
            sink.end(Some(
                if pending.iter().any(|byte: &u8| !byte.is_ascii_whitespace()) {
                    "protocol_partial_eof"
                } else {
                    "connection_closed"
                },
            ));
            return;
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let complete = available[count - 1] == b'\n';
        let payload_count = count - usize::from(complete);
        if pending.len().saturating_add(payload_count) > limit {
            sink.end(Some("protocol_frame_limit"));
            return;
        }
        pending.extend_from_slice(&available[..payload_count]);
        reader.consume(count);
        if complete {
            if sink.frame(&pending).is_err() {
                sink.end(Some("protocol_invalid_frame"));
                return;
            }
            pending.clear();
        }
    }
}

fn process_connection(
    arguments: &[String],
    shared: Shared,
    limit: usize,
    stderr_limit: usize,
) -> Result<Connection> {
    let mut child = Command::new(&arguments[0])
        .args(&arguments[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|_| external("connect", "connection_spawn", false, true))?;
    let mut input = child.stdin.take().context("Missing child stdin")?;
    let output = child.stdout.take().context("Missing child stdout")?;
    let mut errors = child.stderr.take().context("Missing child stderr")?;
    let stop = Arc::new(AtomicBool::new(false));
    let (writer, packets) = mpsc::sync_channel::<Packet>(1);
    let writer_stop = stop.clone();
    let writer_sink = Sink::Rpc(shared.clone());
    let writer_thread = thread::spawn(move || {
        while !writer_stop.load(Ordering::Acquire) {
            match packets.recv_timeout(Duration::from_millis(10)) {
                Ok(packet) => {
                    let result = input
                        .write_all(&packet.bytes)
                        .and_then(|()| input.flush())
                        .map_err(|_| "connection_write".to_owned());
                    let failed = result.is_err();
                    let _ = packet.acknowledgement.send(result);
                    if failed {
                        writer_sink.end(Some("connection_write"));
                        return;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    let output_sink = Sink::Rpc(shared.clone());
    let output_thread = thread::spawn(move || read_lines(output, limit, output_sink));
    let error_thread = thread::spawn(move || {
        let mut buffer = [0; 8192];
        loop {
            match errors.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(count) => {
                    let mut inbox = shared.0.lock().unwrap();
                    inbox.stderr.extend_from_slice(&buffer[..count]);
                    let excess = inbox.stderr.len().saturating_sub(stderr_limit);
                    inbox.stderr.drain(..excess);
                }
            }
        }
    });
    Ok(Connection {
        writer,
        stop,
        threads: vec![writer_thread, output_thread, error_thread],
        child: Some(child),
        socket: None,
    })
}

fn would_block(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(error) if
        matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

fn websocket_connection(
    path: &Path,
    limit: usize,
    timeout: Duration,
    sink: Sink,
) -> Result<Connection> {
    let socket = UnixStream::connect(path)
        .map_err(|_| external("connect", "connection_socket", false, true))?;
    socket.set_read_timeout(Some(timeout))?;
    socket.set_write_timeout(Some(timeout))?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(limit))
        .max_frame_size(Some(limit))
        .write_buffer_size(0);
    let (mut websocket, _) =
        tungstenite::client::client_with_config("ws://localhost/", socket, Some(config))
            .map_err(|_| external("connect", "connection_handshake", false, true))?;
    websocket.get_mut().set_nonblocking(true)?;
    let socket = websocket.get_mut().try_clone()?;
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let (writer, packets) = mpsc::sync_channel::<Packet>(1);
    let worker = thread::spawn(move || websocket_loop(&mut websocket, packets, worker_stop, sink));
    Ok(Connection {
        writer,
        stop,
        threads: vec![worker],
        child: None,
        socket: Some(socket),
    })
}

fn websocket_loop(
    websocket: &mut WebSocket<UnixStream>,
    packets: mpsc::Receiver<Packet>,
    stop: Arc<AtomicBool>,
    sink: Sink,
) {
    let mut acknowledgement: Option<mpsc::Sender<std::result::Result<(), String>>> = None;
    while !stop.load(Ordering::Acquire) {
        if acknowledgement.is_none() {
            match packets.try_recv() {
                Ok(packet) => {
                    let text = match String::from_utf8(packet.bytes) {
                        Ok(text) => text.trim_end_matches(['\r', '\n']).to_owned(),
                        Err(_) => {
                            let _ = packet.acknowledgement.send(Err("invalid_utf8".to_owned()));
                            sink.end(Some("protocol_invalid_utf8"));
                            return;
                        }
                    };
                    match websocket.write(Message::Text(text.into())) {
                        Ok(()) => acknowledgement = Some(packet.acknowledgement),
                        Err(error) if would_block(&error) => {
                            acknowledgement = Some(packet.acknowledgement);
                        }
                        Err(_) => {
                            let _ = packet
                                .acknowledgement
                                .send(Err("connection_write".to_owned()));
                            sink.end(Some("connection_write"));
                            return;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        match websocket.flush() {
            Ok(()) => {
                if let Some(sender) = acknowledgement.take() {
                    let _ = sender.send(Ok(()));
                }
            }
            Err(error) if would_block(&error) => {}
            Err(_) => {
                sink.end(Some("connection_write"));
                return;
            }
        }
        for _ in 0..128 {
            match websocket.read() {
                Ok(Message::Text(text)) => {
                    if sink.frame(text.as_bytes()).is_err() {
                        sink.end(Some("protocol_invalid_frame"));
                        return;
                    }
                }
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
                Ok(Message::Close(_)) => {
                    let _ = websocket.flush();
                    sink.end(Some("connection_closed"));
                    return;
                }
                Ok(_) => {
                    sink.end(Some("protocol_nontext_frame"));
                    return;
                }
                Err(error) if would_block(&error) => break,
                Err(_) => {
                    sink.end(Some("connection_read"));
                    return;
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    let _ = websocket.close(None);
    let _ = websocket.flush();
}

pub struct RpcClient {
    endpoint: Endpoint,
    mcp: bool,
    timeout: Duration,
    frame_limit: usize,
    stderr_limit: usize,
    initialize_params: Value,
    initialized: Option<Value>,
    connection: Option<Connection>,
    shared: Shared,
    next_id: u64,
    halted: bool,
    pub last_thread: Option<Value>,
    pub last_turn_result: Option<Value>,
}

impl RpcClient {
    pub fn from_config(config: &Value) -> Result<Self> {
        let timeout = seconds(match config.get("request_timeout_seconds") {
            Some(value) => value.as_f64().context("RPC timeout must be a number")?,
            None => 120.0,
        })?;
        let endpoint = if let Some(path) = config["daemon_socket"].as_str() {
            ensure!(!path.is_empty(), "Daemon socket cannot be empty");
            Endpoint::WebSocket(PathBuf::from(path))
        } else if !config["codex_command"].is_null() {
            let arguments = command(&config["codex_command"])?;
            if arguments.iter().any(|item| {
                Path::new(item)
                    .file_name()
                    .is_some_and(|name| name == "daemon_bridge.py")
            }) {
                let position = arguments
                    .iter()
                    .position(|item| item == "--socket")
                    .context("Legacy daemon bridge command requires --socket")?;
                let path = arguments
                    .get(position + 1)
                    .context("Missing daemon socket")?;
                ensure!(!path.is_empty(), "Daemon socket cannot be empty");
                Endpoint::WebSocket(PathBuf::from(path))
            } else {
                Endpoint::Process(arguments)
            }
        } else {
            Endpoint::Process(vec!["codex".into(), "app-server".into(), "proxy".into()])
        };
        let frame_limit = integer_limit(config, "max_frame_bytes", FRAME_LIMIT)?;
        let stderr_limit = integer_limit(config, "stderr_limit_bytes", STDERR_LIMIT)?;
        Ok(Self {
            endpoint,
            mcp: false,
            timeout,
            frame_limit,
            stderr_limit,
            initialize_params: json!({
                "clientInfo": {"name": "notion_program", "version": "0.1.0"},
                "capabilities": {"experimentalApi": true}
            }),
            initialized: None,
            connection: None,
            shared: Arc::new((Mutex::new(Inbox::default()), Condvar::new())),
            next_id: 1,
            halted: false,
            last_thread: None,
            last_turn_result: None,
        })
    }

    pub(super) fn desktop(config: &Value) -> Result<Self> {
        let arguments = command(&config["desktop_tools_command"])?;
        let mut client = Self::from_config(&json!({
            "codex_command": arguments,
            "request_timeout_seconds": config["desktop_request_timeout_seconds"].as_f64().unwrap_or(90.0),
            "max_frame_bytes": config["max_frame_bytes"].as_u64().unwrap_or(FRAME_LIMIT as u64)
        }))?;
        client.mcp = true;
        client.initialize_params = json!({
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "notion-program", "version": "0.1.0"}
        });
        Ok(client)
    }

    pub fn notifications(&self) -> Vec<Value> {
        self.shared.0.lock().unwrap().notifications.clone()
    }

    pub fn response_history(&self) -> Vec<Value> {
        self.shared.0.lock().unwrap().response_history.clone()
    }

    pub fn stderr_tail(&self) -> String {
        String::from_utf8_lossy(&self.shared.0.lock().unwrap().stderr).into_owned()
    }

    pub fn connect(&mut self) -> Result<Value> {
        self.connect_until(deadline(self.timeout)?)
    }

    fn connect_until(&mut self, until: Instant) -> Result<Value> {
        self.check("connect", false)?;
        if let Some(initialized) = &self.initialized {
            return Ok(initialized.clone());
        }
        if self.connection.is_none() {
            self.connection = Some(match &self.endpoint {
                Endpoint::Process(arguments) => process_connection(
                    arguments,
                    self.shared.clone(),
                    self.frame_limit,
                    self.stderr_limit,
                )?,
                Endpoint::WebSocket(path) => websocket_connection(
                    path,
                    self.frame_limit,
                    remaining(until)?.min(Duration::from_secs(15)),
                    Sink::Rpc(self.shared.clone()),
                )?,
            });
        }
        let initialized = self.raw_request("initialize", &self.initialize_params.clone(), until);
        let result = (|| -> Result<Value> {
            let initialized = initialized?;
            if !initialized.is_object()
                || self.mcp
                    && initialized["protocolVersion"]
                        .as_str()
                        .is_none_or(str::is_empty)
            {
                return Err(external("initialize", "invalid_handshake", false, false));
            }
            let method = if self.mcp {
                "notifications/initialized"
            } else {
                "initialized"
            };
            self.write_message(&json!({"method": method, "params": {}}), until, false)?;
            self.initialized = Some(initialized.clone());
            Ok(initialized)
        })();
        if result.is_err() {
            self.connection.take();
            self.halted = true;
        }
        result
    }

    fn check(&self, operation: &str, mutation: bool) -> Result<()> {
        let inbox = self.shared.0.lock().unwrap();
        if inbox.attention.is_some() {
            return Err(external(operation, "needs_attention", mutation, false));
        }
        if self.halted {
            return Err(external(
                operation,
                "timeout_connection_disabled",
                mutation,
                true,
            ));
        }
        if let Some(kind) = &inbox.failure {
            return Err(external(
                operation,
                kind,
                mutation,
                kind.starts_with("connection"),
            ));
        }
        Ok(())
    }

    fn timeout(&mut self, operation: &str, mutation: bool) -> anyhow::Error {
        self.halted = true;
        external(operation, "timeout", mutation, true)
    }

    fn write_message(&mut self, message: &Value, until: Instant, mutation: bool) -> Result<()> {
        let mut message = message.clone();
        if self.mcp {
            message["jsonrpc"] = json!("2.0");
        }
        let mut bytes = serde_json::to_vec(&message)?;
        ensure!(
            bytes.len() <= self.frame_limit,
            "Outgoing RPC frame exceeds limit"
        );
        bytes.push(b'\n');
        let operation = message["method"].as_str().unwrap_or("write").to_owned();
        self.check(&operation, false)?;
        let (sender, receiver) = mpsc::channel();
        self.connection
            .as_ref()
            .context("RPC is not connected")?
            .writer
            .try_send(Packet {
                bytes,
                acknowledgement: sender,
            })
            .map_err(|_| external(&operation, "connection_write", false, true))?;
        let duration = remaining(until).map_err(|_| self.timeout(&operation, mutation))?;
        match receiver.recv_timeout(duration) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(kind)) => Err(external(&operation, &kind, mutation, true)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(self.timeout(&operation, mutation)),
            Err(_) => Err(external(&operation, "connection_write", mutation, true)),
        }
    }

    fn raw_request(&mut self, method: &str, params: &Value, until: Instant) -> Result<Value> {
        let mutation = !read_only(method, params);
        self.check(method, false)?;
        let identifier = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .context("RPC identifiers exhausted")?;
        let mut message = json!({"id": identifier, "method": method});
        if !params.is_null() {
            message["params"] = params.clone();
        }
        self.write_message(&message, until, mutation)?;
        let key = identifier.to_string();
        let shared = self.shared.clone();
        let mut inbox = shared.0.lock().unwrap();
        loop {
            if inbox.attention.is_some() {
                return Err(external(method, "needs_attention", mutation, false));
            }
            if let Some(response) = inbox.responses.remove(&key) {
                if response.get("error").is_some() {
                    return Err(anyhow::Error::new(RpcFailure {
                        response: response.clone(),
                    })
                    .context(ExternalError {
                        operation: method.to_owned(),
                        outcome_unknown: mutation,
                        reconnectable: false,
                        kind: "rpc_error".to_owned(),
                    }));
                }
                return Ok(response["result"].clone());
            }
            if let Some(kind) = &inbox.failure {
                return Err(external(
                    method,
                    kind,
                    mutation,
                    kind.starts_with("connection"),
                ));
            }
            let duration = match remaining(until) {
                Ok(duration) => duration,
                Err(_) => return Err(self.timeout(method, mutation)),
            };
            inbox = shared.1.wait_timeout(inbox, duration).unwrap().0;
        }
    }

    pub fn request(&mut self, method: &str, params: &Value) -> Result<Value> {
        ensure!(!method.is_empty(), "RPC method cannot be empty");
        ensure!(
            !matches!(
                method,
                "initialize" | "initialized" | "notifications/initialized"
            ),
            "Use connect for initialization"
        );
        ensure!(
            params.is_null() || params.is_object(),
            "RPC parameters must be an object"
        );
        let until = deadline(self.timeout)?;
        self.connect_until(until)?;
        self.raw_request(method, params, until)
    }

    pub fn start_thread(
        &mut self,
        model: &str,
        cwd: &str,
        title: &str,
        sandbox: &str,
        reviewer: &str,
    ) -> Result<Value> {
        ensure!(!model.is_empty(), "Model cannot be empty");
        ensure!(
            Path::new(cwd).is_absolute(),
            "Execution cwd must be absolute"
        );
        ensure!(
            title.is_empty() || !title.trim().is_empty(),
            "Title cannot contain only whitespace"
        );
        let mut params = json!({
            "model": model, "cwd": cwd, "ephemeral": false,
            "allowProviderModelFallback": false
        });
        add_settings(&mut params, sandbox, reviewer)?;
        let until = deadline(self.timeout)?;
        self.connect_until(until)?;
        let response = self.raw_request("thread/start", &params, until)?;
        let thread = object_with_id(&response, "thread")?;
        self.last_thread = Some(thread.clone());
        if !title.is_empty() {
            self.raw_request(
                "thread/name/set",
                &json!({"threadId": thread["id"], "name": title}),
                until,
            )?;
            let response = self.raw_request(
                "thread/read",
                &json!({"threadId": thread["id"], "includeTurns": false}),
                until,
            )?;
            let named = object_with_id(&response, "thread")?;
            ensure!(
                named["id"] == thread["id"],
                "Thread identity changed after naming"
            );
            self.last_thread = Some(named.clone());
            return Ok(named);
        }
        Ok(thread)
    }

    pub fn resume_thread(
        &mut self,
        thread: &str,
        model: &str,
        sandbox: &str,
        reviewer: &str,
    ) -> Result<Value> {
        ensure!(!thread.is_empty(), "Thread cannot be empty");
        let mut params = json!({"threadId": thread, "excludeTurns": true});
        if !model.is_empty() {
            params["model"] = json!(model);
        }
        add_settings(&mut params, sandbox, reviewer)?;
        let result = self.request("thread/resume", &params)?;
        let resumed = object_with_id(&result, "thread")?;
        ensure!(
            resumed["id"] == thread,
            "Resume returned a different thread"
        );
        self.last_thread = Some(resumed.clone());
        Ok(resumed)
    }

    pub fn start_turn(&mut self, thread: &str, prompt: &str) -> Result<Value> {
        self.turn(thread, prompt, None)
    }

    pub fn run_turn(&mut self, thread: &str, prompt: &str, timeout: f64) -> Result<Value> {
        self.turn(thread, prompt, Some(seconds(timeout)?))
    }

    fn turn(&mut self, thread: &str, prompt: &str, wait: Option<Duration>) -> Result<Value> {
        ensure!(!thread.is_empty(), "Thread cannot be empty");
        let start = self.notifications().len();
        let until = deadline(wait.unwrap_or(self.timeout))?;
        let mut turn = Value::Null;
        let mut acknowledged = false;
        let execution = (|| -> Result<String> {
            self.connect_until(until)?;
            let response = self.raw_request(
                "turn/start",
                &json!({"threadId": thread, "input": [{"type": "text", "text": prompt}]}),
                until,
            )?;
            turn = object_with_id(&response, "turn")?;
            acknowledged = true;
            if wait.is_none() {
                return match turn["status"].as_str() {
                    Some("inProgress") => Ok("in_progress".to_owned()),
                    Some(status @ ("completed" | "failed" | "interrupted")) => {
                        Ok(status.to_owned())
                    }
                    _ => Err(external("turn/start", "invalid_turn_status", true, false)),
                };
            }
            if let Some(status @ ("completed" | "failed" | "interrupted")) = turn["status"].as_str()
            {
                return Ok(status.to_owned());
            }
            let shared = self.shared.clone();
            let mut inbox = shared.0.lock().unwrap();
            let mut cursor = start;
            loop {
                if inbox.attention.is_some() {
                    return Ok("needs_attention".to_owned());
                }
                for event in &inbox.notifications[cursor..] {
                    if event["method"] == "turn/completed"
                        && event["params"]["threadId"] == thread
                        && event["params"]["turn"]["id"] == turn["id"]
                    {
                        let completed = &event["params"]["turn"];
                        let status = required(completed, "status")?;
                        if !matches!(status, "completed" | "failed" | "interrupted") {
                            return Err(external(
                                "turn/completed",
                                "invalid_turn_status",
                                true,
                                false,
                            ));
                        }
                        turn = completed.clone();
                        return Ok(status.to_owned());
                    }
                }
                cursor = inbox.notifications.len();
                if let Some(kind) = &inbox.failure {
                    return Err(external("turn/completed", kind, true, false));
                }
                let duration = match remaining(until) {
                    Ok(duration) => duration,
                    Err(_) => {
                        self.halted = true;
                        return Ok("timeout".to_owned());
                    }
                };
                inbox = shared.1.wait_timeout(inbox, duration).unwrap().0;
            }
        })();
        let status = match execution {
            Ok(status) => status,
            Err(error) => {
                let kind = error
                    .downcast_ref::<ExternalError>()
                    .map(|error| error.kind.as_str());
                match kind {
                    Some("needs_attention") => "needs_attention".to_owned(),
                    Some("timeout" | "timeout_connection_disabled") => "timeout".to_owned(),
                    _ => return Err(error),
                }
            }
        };
        let events = self.notifications()[start..].to_vec();
        if turn.is_null() {
            for event in &events {
                if matches!(
                    event["method"].as_str(),
                    Some("turn/started" | "turn/completed")
                ) && event["params"]["threadId"] == thread
                    && event["params"]["turn"].is_object()
                {
                    turn = event["params"]["turn"].clone();
                }
            }
        }
        let result = json!({
            "turn": turn, "status": status,
            "text": turn_text(thread, &turn, &events),
            "notifications": events, "start_response_received": acknowledged
        });
        self.last_turn_result = Some(result.clone());
        Ok(result)
    }
}

fn integer_limit(config: &Value, key: &str, default: usize) -> Result<usize> {
    if config[key].is_null() {
        return Ok(default);
    }
    let value = config[key]
        .as_u64()
        .context("Frame limits must be positive integers")?;
    ensure!(value > 0, "Frame limits must be positive");
    usize::try_from(value).context("Frame limit is too large")
}

pub(super) fn add_settings(params: &mut Value, sandbox: &str, reviewer: &str) -> Result<()> {
    ensure!(
        matches!(sandbox, "" | "read-only" | "workspace-write"),
        "Unsupported execution sandbox"
    );
    ensure!(
        matches!(reviewer, "" | "user" | "auto_review"),
        "Unsupported approvals reviewer"
    );
    if !sandbox.is_empty() {
        params["sandbox"] = json!(sandbox);
    }
    if !reviewer.is_empty() {
        params["approvalsReviewer"] = json!(reviewer);
    }
    Ok(())
}

pub(super) fn object_with_id(result: &Value, key: &str) -> Result<Value> {
    required(&result[key], "id").map_err(|_| external(key, "missing_identity", false, false))?;
    Ok(result[key].clone())
}

fn read_only(method: &str, params: &Value) -> bool {
    matches!(
        method,
        "initialize" | "initialized" | "notifications/initialized"
    ) || method.ends_with("/read")
        || method.ends_with("/list")
        || method == "tools/list"
        || method == "mcpServer/tool/call"
            && params["tool"].as_str().is_some_and(|tool| {
                tool.ends_with(".fetch")
                    || tool.ends_with(".query-data-sources")
                    || tool.contains("get_tool_access")
            })
        || method == "tools/call" && params["name"] == "read_thread"
}

fn turn_text(thread: &str, turn: &Value, events: &[Value]) -> String {
    let mut messages: Vec<(String, String, Option<String>, bool)> = Vec::new();
    let mut store = |item: &Value, delta: bool| {
        let (Some(identifier), Some(text)) = (
            item["id"].as_str(),
            item[if delta { "delta" } else { "text" }].as_str(),
        ) else {
            return;
        };
        let position = messages.iter().position(|entry| entry.0 == identifier);
        let position = position.unwrap_or_else(|| {
            messages.push((identifier.to_owned(), String::new(), None, false));
            messages.len() - 1
        });
        let entry = &mut messages[position];
        if delta {
            if !entry.3 {
                entry.1.push_str(text);
            }
        } else {
            entry.1 = text.to_owned();
            entry.2 = item["phase"].as_str().map(str::to_owned);
            entry.3 = true;
        }
    };
    if !turn.is_null() {
        for event in events {
            let params = &event["params"];
            if params["threadId"] != thread || params["turnId"] != turn["id"] {
                continue;
            }
            if event["method"] == "item/agentMessage/delta" {
                store(
                    &json!({"id": params["itemId"], "delta": params["delta"]}),
                    true,
                );
            } else if event["method"] == "item/completed"
                && params["item"]["type"] == "agentMessage"
            {
                store(&params["item"], false);
            }
        }
        if let Some(items) = turn["items"].as_array() {
            for item in items {
                if item["type"] == "agentMessage" {
                    store(item, false);
                }
            }
        }
    }
    let finals = messages
        .iter()
        .any(|entry| entry.2.as_deref() == Some("final_answer"));
    messages
        .iter()
        .filter(|entry| !finals || entry.2.as_deref() == Some("final_answer"))
        .map(|entry| entry.1.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn bridge(socket: &Path, max_frame_bytes: usize) -> Result<()> {
    bridge_streams(socket, max_frame_bytes, io::stdin(), io::stdout())
}

fn bridge_streams(
    socket: &Path,
    max_frame_bytes: usize,
    input: impl Read + Send + 'static,
    mut output: impl Write,
) -> Result<()> {
    ensure!(max_frame_bytes > 0, "Frame limit must be positive");
    let (events, incoming) = mpsc::channel();
    let connection = websocket_connection(
        socket,
        max_frame_bytes,
        Duration::from_secs(15),
        Sink::Bridge(events.clone()),
    )?;
    let writer = connection.writer.clone();
    let input_thread = thread::spawn(move || {
        struct Forward {
            writer: mpsc::SyncSender<Packet>,
        }
        let mut reader = io::BufReader::new(input);
        let mut pending = Vec::new();
        let forward = Forward { writer };
        let result = (|| -> Result<()> {
            loop {
                let available = reader.fill_buf()?;
                if available.is_empty() {
                    if !pending.is_empty() {
                        forward.send(&pending)?;
                    }
                    return Ok(());
                }
                let count = available
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(available.len(), |position| position + 1);
                ensure!(
                    pending.len().saturating_add(count) <= max_frame_bytes,
                    "RPC frame exceeds configured limit"
                );
                let complete = available[count - 1] == b'\n';
                pending.extend_from_slice(&available[..count]);
                reader.consume(count);
                if complete {
                    forward.send(&pending)?;
                    pending.clear();
                }
            }
        })();
        let _ = events.send(WireEvent::End(
            result.err().map(|_| "bridge_input_failed".to_owned()),
        ));
        impl Forward {
            fn send(&self, bytes: &[u8]) -> Result<()> {
                if bytes.iter().all(u8::is_ascii_whitespace) {
                    return Ok(());
                }
                std::str::from_utf8(bytes)?;
                let (sender, receiver) = mpsc::channel();
                self.writer.send(Packet {
                    bytes: bytes.to_vec(),
                    acknowledgement: sender,
                })?;
                receiver
                    .recv()?
                    .map_err(|_| external("bridge", "connection_write", true, false))
            }
        }
    });
    let result = loop {
        match incoming.recv() {
            Ok(WireEvent::Frame(bytes)) => {
                output.write_all(&bytes)?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
            Ok(WireEvent::End(None)) => break Ok(()),
            Ok(WireEvent::End(Some(kind))) => {
                break Err(external("bridge", &kind, false, false));
            }
            Err(_) => break Err(external("bridge", "connection_closed", false, false)),
        }
    };
    drop(connection);
    if input_thread.is_finished() {
        let _ = input_thread.join();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::test_support::{THREAD, config};
    use std::os::unix::net::UnixListener;

    #[test]
    fn handshake_raw_results_and_interleaved_frames() -> Result<()> {
        let mut client = RpcClient::from_config(&config(json!({})))?;
        assert_eq!(client.connect()?["userAgent"], "offline-fake");
        assert_eq!(client.request("test/null", &Value::Null)?, Value::Null);
        assert_eq!(client.request("test/scalar", &json!({}))?, 42);
        assert_eq!(client.request("test/chunks", &json!({}))?["text"], "你好🙂");
        assert_eq!(client.request("test/interleaved", &json!({}))?["ok"], true);
        let history = client.request("test/history", &json!({}))?;
        let messages = history.as_array().unwrap();
        assert_eq!(messages[0]["method"], "initialize");
        assert_eq!(messages[1]["method"], "initialized");
        assert!(messages[0].get("jsonrpc").is_none());
        assert_eq!(
            messages
                .iter()
                .filter(|message| message["method"] == "initialize")
                .count(),
            1
        );
        assert!(
            client
                .response_history()
                .iter()
                .any(|response| response["id"] == "unrelated")
        );
        assert!(
            client
                .notifications()
                .iter()
                .any(|event| event["method"] == "unrelated/notice")
        );
        Ok(())
    }

    #[test]
    fn title_failure_preserves_created_thread_and_does_not_create_again() -> Result<()> {
        let mut client = RpcClient::from_config(&config(json!({"title_error": true})))?;
        let error = client.start_thread("model", "/tmp", "task", "workspace-write", "auto_review");
        assert!(error.is_err());
        assert_eq!(client.last_thread.as_ref().unwrap()["id"], THREAD);
        let history = client.request("test/history", &json!({}))?;
        assert_eq!(
            history
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["method"] == "thread/start")
                .count(),
            1
        );
        let created = history
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["method"] == "thread/start")
            .unwrap();
        assert_eq!(created["params"]["ephemeral"], false);
        assert_eq!(created["params"]["allowProviderModelFallback"], false);
        assert_eq!(created["params"]["approvalsReviewer"], "auto_review");
        Ok(())
    }

    #[test]
    fn resume_only_changes_explicit_settings() -> Result<()> {
        let mut client = RpcClient::from_config(&config(json!({})))?;
        client.resume_thread(THREAD, "", "", "")?;
        let history = client.request("test/history", &json!({}))?;
        let resumed = &history.as_array().unwrap()[2];
        assert_eq!(
            resumed["params"],
            json!({"threadId": THREAD, "excludeTurns": true})
        );
        Ok(())
    }

    #[test]
    fn completion_before_start_response_and_final_item_replacement() -> Result<()> {
        for mode in ["completed", "early", "failed", "interrupted"] {
            let mut client = RpcClient::from_config(&config(json!({"turn_mode": mode})))?;
            let outcome = client.run_turn(THREAD, "prompt", 2.0)?;
            assert_eq!(outcome["text"], "完整结果");
            assert_eq!(outcome["turn"]["custom"], "terminal");
            assert_eq!(outcome["start_response_received"], true);
            assert_eq!(
                outcome["status"],
                if matches!(mode, "failed" | "interrupted") {
                    mode
                } else {
                    "completed"
                }
            );
        }
        Ok(())
    }

    #[test]
    fn start_turn_has_no_execution_lifetime_timeout() -> Result<()> {
        let mut client = RpcClient::from_config(&config(json!({"turn_mode": "active"})))?;
        assert_eq!(
            client.start_turn(THREAD, "prompt")?["status"],
            "in_progress"
        );
        assert_eq!(
            client.request("test/echo", &json!({"still": "connected"}))?["still"],
            "connected"
        );
        let history = client.request("test/history", &json!({}))?;
        assert!(
            !history
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["method"] == "turn/interrupt")
        );
        Ok(())
    }

    #[test]
    fn every_server_request_latches_attention_even_with_same_response_id() -> Result<()> {
        let mut client = RpcClient::from_config(&config(json!({})))?;
        let error = client.request("test/attention", &json!({})).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ExternalError>().unwrap().kind,
            "needs_attention"
        );
        assert!(client.request("test/echo", &json!({})).is_err());
        assert!(
            client
                .notifications()
                .iter()
                .any(|message| message.get("id").is_some())
        );
        let mut client = RpcClient::from_config(&config(json!({"turn_mode": "no_ack"})))?;
        let outcome = client.start_turn(THREAD, "prompt")?;
        assert_eq!(outcome["status"], "needs_attention");
        assert_eq!(outcome["start_response_received"], false);
        assert_eq!(outcome["turn"]["id"], "turn-1");
        Ok(())
    }

    #[test]
    fn timeout_disables_connection_and_preserves_partial_turn() -> Result<()> {
        let mut client = RpcClient::from_config(&config(json!({"turn_mode": "active"})))?;
        let result = client.run_turn(THREAD, "prompt", 0.2)?;
        assert_eq!(result["status"], "timeout");
        assert_eq!(result["start_response_received"], true);
        assert_eq!(result["turn"]["id"], "turn-1");
        assert!(client.request("test/echo", &json!({})).is_err());
        Ok(())
    }

    #[test]
    fn blocked_pipe_and_partial_frames_obey_deadline() -> Result<()> {
        for blocked in [false, true] {
            let mut settings = config(json!({}));
            settings["request_timeout_seconds"] = json!(0.2);
            let mut client = RpcClient::from_config(&settings)?;
            client.connect()?;
            if blocked {
                client.request("test/block", &json!({}))?;
            }
            let started = Instant::now();
            let error = if blocked {
                client.request("test/echo", &json!({"data": "x".repeat(2 * 1024 * 1024)}))
            } else {
                client.request("test/partial", &json!({}))
            }
            .unwrap_err();
            assert!(started.elapsed() < Duration::from_secs(2));
            assert_eq!(
                error.downcast_ref::<ExternalError>().unwrap().kind,
                "timeout"
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_frames_and_exit_are_not_reported_as_timeouts() -> Result<()> {
        for bytes in [
            "[]",
            "not-json",
            "{\"jsonrpc\":\"1.0\",\"id\":3,\"result\":{}}",
            "{\"id\":true,\"result\":{}}",
        ] {
            let mut client = RpcClient::from_config(&config(json!({"bad": bytes})))?;
            let error = client.request("test/bad", &json!({})).unwrap_err();
            assert!(
                error
                    .downcast_ref::<ExternalError>()
                    .unwrap()
                    .kind
                    .starts_with("protocol")
            );
        }
        let mut client = RpcClient::from_config(&config(json!({})))?;
        assert!(
            client
                .request("test/exit", &json!({}))
                .unwrap_err()
                .to_string()
                .contains("connection")
        );
        Ok(())
    }

    #[test]
    fn stderr_is_drained_and_rpc_errors_keep_raw_data_without_displaying_it() -> Result<()> {
        let mut settings = config(json!({"stderr": true}));
        settings["stderr_limit_bytes"] = json!(1024);
        let mut client = RpcClient::from_config(&settings)?;
        client.connect()?;
        let error = client.request("test/error", &json!({})).unwrap_err();
        assert_eq!(
            error.downcast_ref::<RpcFailure>().unwrap().response["error"]["data"]["keep"],
            true
        );
        assert!(!format!("{error:#}").contains("private-secret"));
        assert_eq!(
            client.request("test/echo", &json!({"ok": true}))?["ok"],
            true
        );
        assert!(client.stderr_tail().ends_with("�TAIL"));
        assert!(client.shared.0.lock().unwrap().stderr.len() <= 1024);
        Ok(())
    }

    #[test]
    fn invalid_configuration_fails_without_spawning() {
        for timeout in [0.0, -1.0] {
            assert!(RpcClient::from_config(&json!({"request_timeout_seconds": timeout})).is_err());
        }
        for settings in [
            json!({"codex_command": []}),
            json!({"codex_command": "codex"}),
            json!({"max_frame_bytes": 0}),
        ] {
            assert!(RpcClient::from_config(&settings).is_err());
        }
    }

    #[test]
    fn native_websocket_monitor_disconnect_preserves_daemon_execution() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&path)?;
        let methods = Arc::new(Mutex::new(Vec::new()));
        let recorded = methods.clone();
        let daemon = thread::spawn(move || {
            let mut active = false;
            for connection in listener.incoming().take(2) {
                let mut websocket = tungstenite::accept(connection.unwrap()).unwrap();
                loop {
                    let message = match websocket.read() {
                        Ok(Message::Text(message)) => message,
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => continue,
                    };
                    let request: Value = serde_json::from_str(&message).unwrap();
                    let method = request["method"].as_str().unwrap();
                    recorded.lock().unwrap().push(method.to_owned());
                    if request.get("id").is_none() {
                        continue;
                    }
                    let result = match method {
                        "initialize" => json!({"userAgent": "fake-daemon"}),
                        "turn/start" => {
                            active = true;
                            json!({"turn": {"id": "long-task", "status": "inProgress"}})
                        }
                        "thread/read" => json!({"active": active}),
                        _ => Value::Null,
                    };
                    websocket
                        .send(Message::Text(
                            json!({"id": request["id"], "result": result})
                                .to_string()
                                .into(),
                        ))
                        .unwrap();
                }
            }
        });
        let settings = json!({"daemon_socket": path, "request_timeout_seconds": 2});
        {
            let mut first = RpcClient::from_config(&settings)?;
            assert_eq!(first.start_turn(THREAD, "prompt")?["status"], "in_progress");
        }
        {
            let mut second = RpcClient::from_config(&settings)?;
            assert_eq!(
                second.request("thread/read", &json!({"threadId": THREAD}))?["active"],
                true
            );
        }
        daemon.join().unwrap();
        let methods = methods.lock().unwrap();
        assert_eq!(
            methods
                .iter()
                .filter(|method| *method == "turn/start")
                .count(),
            1
        );
        assert!(!methods.iter().any(|method| method == "turn/interrupt"));
        Ok(())
    }

    #[test]
    fn legacy_python_bridge_configuration_uses_native_socket() -> Result<()> {
        let client = RpcClient::from_config(&json!({
            "codex_command": ["/missing/python", "/missing/daemon_bridge.py", "--socket", "/tmp/fake.sock"]
        }))?;
        assert!(
            matches!(client.endpoint, Endpoint::WebSocket(path) if path == Path::new("/tmp/fake.sock"))
        );
        Ok(())
    }

    #[test]
    fn bridge_forwards_text_frames_and_stops_only_its_connection_on_input_eof() -> Result<()> {
        struct Output(mpsc::Sender<Vec<u8>>);
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0
                    .send(bytes.to_vec())
                    .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Output closed"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("bridge.sock");
        let listener = UnixListener::bind(&path)?;
        let daemon = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut websocket = tungstenite::accept(socket).unwrap();
            let message = websocket.read().unwrap();
            assert_eq!(message.into_text().unwrap(), "{\"id\":1}");
            websocket.send(Message::Ping(vec![1, 2].into())).unwrap();
            websocket
                .send(Message::Text("{\"id\":1,\"result\":\"你好\"}".into()))
                .unwrap();
            loop {
                match websocket.read() {
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });
        let (input, mut writer) = UnixStream::pair()?;
        let (sender, receiver) = mpsc::channel();
        let forwarding = thread::spawn(move || bridge_streams(&path, 4096, input, Output(sender)));
        writer.write_all(b"\n{\"id\":1}\r\n")?;
        let body = receiver.recv_timeout(Duration::from_secs(2))?;
        assert_eq!(String::from_utf8(body)?, "{\"id\":1,\"result\":\"你好\"}");
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2))?, b"\n");
        drop(writer);
        forwarding.join().unwrap()?;
        daemon.join().unwrap();
        Ok(())
    }
}
