use anyhow::{Context, Result, anyhow, bail, ensure};
use rustix::io::Errno;
use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

pub trait Rpc {
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}

#[derive(Debug)]
pub struct RequestError {
    pub method: String,
    pub details: Value,
}

impl fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = self.details["message"].as_str().unwrap_or("请求失败");
        write!(formatter, "{}: {message}", self.method)
    }
}

impl std::error::Error for RequestError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Reader,
    NativeReader,
    TitleWriter,
    ArchiveWriter,
}

impl Role {
    fn check(self, method: &str) -> Result<()> {
        let allowed = match self {
            Self::Reader => matches!(method, "initialize" | "thread/list" | "thread/turns/list"),
            Self::NativeReader => matches!(
                method,
                "initialize" | "thread/list" | "thread/read" | "threadSection/list"
            ),
            Self::TitleWriter => matches!(method, "initialize" | "thread/read" | "thread/name/set"),
            Self::ArchiveWriter => matches!(
                method,
                "initialize"
                    | "thread/list"
                    | "thread/read"
                    | "thread/turns/list"
                    | "thread/archive"
            ),
        };
        ensure!(allowed, "此 RPC 连接不允许调用 {method}");
        Ok(())
    }
}

fn deadline_after(timeout: Duration) -> Result<Instant> {
    ensure!(!timeout.is_zero(), "RPC 超时必须大于零");
    Instant::now().checked_add(timeout).context("RPC 超时过大")
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "RPC 操作超时"))
}

fn deadline_io<Output>(
    deadline: Instant,
    mut operation: impl FnMut() -> io::Result<Output>,
) -> io::Result<Output> {
    loop {
        let budget = remaining(deadline)?;
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(budget.min(Duration::from_millis(2)));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => {
                remaining(deadline)?;
                return result;
            }
        }
    }
}

fn write_json(writer: &mut impl Write, message: &Value, deadline: Instant) -> Result<()> {
    let mut bytes = serde_json::to_vec(message)?;
    bytes.push(b'\n');
    let mut written = 0;
    while written < bytes.len() {
        let count = deadline_io(deadline, || writer.write(&bytes[written..]))?;
        ensure!(count != 0, "Codex 读取连接已关闭");
        written += count;
    }
    deadline_io(deadline, || writer.flush())?;
    Ok(())
}

fn response_result(message: Value, request_id: u64, method: &str) -> Result<Option<Value>> {
    if message.get("method").is_some() || message["id"].as_u64() != Some(request_id) {
        return Ok(None);
    }
    if let Some(details) = message.get("error") {
        return Err(RequestError {
            method: method.to_owned(),
            details: details.clone(),
        }
        .into());
    }
    message
        .get("result")
        .cloned()
        .map(Some)
        .context("RPC 回执缺少 result")
}

struct ManagedChild(Child);

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct StdioReader {
    _process: ManagedChild,
    input: ChildStdin,
    output: ChildStdout,
    buffer: Vec<u8>,
    request_id: u64,
    timeout: Duration,
}

impl StdioReader {
    pub fn connect(command: &[String], timeout: Duration) -> Result<Self> {
        let mut reader = Self::start(command, timeout)?;
        reader.call(
            "initialize",
            json!({
                "clientInfo": {"name": "local_session_status", "version": "0.1.0"}
            }),
        )?;
        write_json(
            &mut reader.input,
            &json!({"method": "initialized", "params": {}}),
            deadline_after(timeout)?,
        )?;
        Ok(reader)
    }

    fn start(command: &[String], timeout: Duration) -> Result<Self> {
        let program = command
            .first()
            .filter(|part| !part.is_empty())
            .context("Codex 命令为空")?;
        deadline_after(timeout)?;
        let mut process = ManagedChild(
            Command::new(program)
                .args(&command[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .context("启动 Codex 只读进程失败")?,
        );
        let input = process.0.stdin.take().context("Codex 进程缺少 stdin")?;
        let output = process.0.stdout.take().context("Codex 进程缺少 stdout")?;
        rustix::io::ioctl_fionbio(&input, true)?;
        rustix::io::ioctl_fionbio(&output, true)?;
        Ok(Self {
            _process: process,
            input,
            output,
            buffer: Vec::new(),
            request_id: 0,
            timeout,
        })
    }

    fn next_message(&mut self, deadline: Instant) -> Result<Value> {
        loop {
            remaining(deadline)?;
            if let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
                let bytes: Vec<_> = self.buffer.drain(..=index).collect();
                if let Ok(message) = serde_json::from_slice::<Value>(&bytes) {
                    return Ok(message);
                }
                continue;
            }
            let mut chunk = [0_u8; 8192];
            let count = deadline_io(deadline, || self.output.read(&mut chunk))?;
            if count == 0 {
                ensure!(!self.buffer.is_empty(), "Codex 读取连接已断开");
                return serde_json::from_slice(&std::mem::take(&mut self.buffer))
                    .context("Codex 读取连接已断开");
            }
            self.buffer.extend_from_slice(&chunk[..count]);
        }
    }
}

impl Rpc for StdioReader {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        Role::Reader.check(method)?;
        let deadline = deadline_after(self.timeout)?;
        self.request_id = self.request_id.checked_add(1).context("RPC 请求编号耗尽")?;
        write_json(
            &mut self.input,
            &json!({"id": self.request_id, "method": method, "params": params}),
            deadline,
        )?;
        loop {
            let message = self.next_message(deadline)?;
            if let Some(result) = response_result(message, self.request_id, method)? {
                return Ok(result);
            }
        }
    }
}

fn verified_socket(path: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::FileTypeExt;
    let target = path.canonicalize().context("桌面 socket 不存在")?;
    let endpoint = target.metadata()?;
    let parent = target
        .parent()
        .context("桌面 socket 缺少父目录")?
        .metadata()?;
    let owner = rustix::process::getuid().as_raw();
    ensure!(
        endpoint.file_type().is_socket()
            && endpoint.uid() == owner
            && endpoint.mode() & 0o7777 == 0o600
            && parent.uid() == owner
            && parent.mode() & 0o7777 == 0o700,
        "桌面运行连接的所有者或权限无效"
    );
    Ok(target)
}

fn wait_connected(stream: UnixStream, deadline: Instant) -> Result<UnixStream> {
    loop {
        remaining(deadline)?;
        if let Some(error) = stream.take_error()? {
            return Err(error.into());
        }
        if stream.peer_addr().is_ok() {
            return Ok(stream);
        }
        thread::sleep(remaining(deadline)?.min(Duration::from_millis(2)));
    }
}

fn connect_unix(path: &Path, deadline: Instant) -> Result<UnixStream> {
    let address = SocketAddrUnix::new(path)?;
    loop {
        remaining(deadline)?;
        let socket = rustix::net::socket_with(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )?;
        match rustix::net::connect(&socket, &address) {
            Ok(()) => return Ok(UnixStream::from(socket)),
            Err(Errno::INPROGRESS | Errno::ALREADY | Errno::INTR) => {
                return wait_connected(UnixStream::from(socket), deadline);
            }
            Err(Errno::AGAIN) => {
                thread::sleep(remaining(deadline)?.min(Duration::from_millis(2)));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[derive(Debug)]
struct DeadlineStream {
    stream: UnixStream,
    deadline: Instant,
}

impl Read for DeadlineStream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        deadline_io(self.deadline, || self.stream.read(bytes))
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        deadline_io(self.deadline, || self.stream.write(bytes))
    }

    fn flush(&mut self) -> io::Result<()> {
        deadline_io(self.deadline, || self.stream.flush())
    }
}

pub struct DesktopConnection {
    connection: WebSocket<DeadlineStream>,
    request_id: u64,
    timeout: Duration,
    role: Role,
    title_deadline: Option<Instant>,
}

impl DesktopConnection {
    pub fn connect(path: &Path, timeout: Duration, role: Role) -> Result<Self> {
        let budget = if role == Role::TitleWriter {
            timeout.min(Duration::from_secs(3))
        } else {
            timeout
        };
        let deadline = deadline_after(budget)?;
        let socket = connect_unix(&verified_socket(path)?, deadline)?;
        let stream = DeadlineStream {
            stream: socket,
            deadline,
        };
        let (connection, _) = tungstenite::client("ws://localhost/rpc", stream)
            .map_err(|error| anyhow!("桌面 WebSocket 握手失败: {error}"))?;
        let mut reader = Self {
            connection,
            request_id: 0,
            timeout,
            role,
            title_deadline: matches!(role, Role::TitleWriter | Role::ArchiveWriter).then_some(deadline),
        };
        reader.call(
            "initialize",
            json!({
                "clientInfo": {"name": "session_board_runtime", "version": "0.2.5"}
            }),
        )?;
        reader.send_json(&json!({"method": "initialized"}), reader.call_deadline()?)?;
        Ok(reader)
    }

    fn call_deadline(&self) -> Result<Instant> {
        let deadline = match self.title_deadline {
            Some(deadline) => deadline,
            None => deadline_after(self.timeout)?,
        };
        remaining(deadline)?;
        Ok(deadline)
    }

    fn send_json(&mut self, message: &Value, deadline: Instant) -> Result<()> {
        self.connection.get_mut().deadline = deadline;
        self.connection
            .send(Message::Text(serde_json::to_string(message)?.into()))?;
        remaining(deadline)?;
        Ok(())
    }

    fn next_message(&mut self, deadline: Instant) -> Result<Value> {
        loop {
            self.connection.get_mut().deadline = deadline;
            let message = self.connection.read()?;
            remaining(deadline)?;
            match message {
                Message::Text(text) => return Ok(serde_json::from_str(text.as_str())?),
                Message::Binary(bytes) => return Ok(serde_json::from_slice(&bytes)?),
                Message::Close(_) => bail!("桌面运行连接已断开"),
                _ => {}
            }
        }
    }
}

impl Rpc for DesktopConnection {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.role.check(method)?;
        let deadline = self.call_deadline()?;
        self.request_id = self.request_id.checked_add(1).context("RPC 请求编号耗尽")?;
        self.send_json(
            &json!({"id": self.request_id, "method": method, "params": params}),
            deadline,
        )?;
        loop {
            let message = self.next_message(deadline)?;
            if let Some(result) = response_result(message, self.request_id, method)? {
                return Ok(result);
            }
        }
    }
}

impl Drop for DesktopConnection {
    fn drop(&mut self) {
        let close_deadline = Instant::now() + Duration::from_millis(100);
        self.connection.get_mut().deadline = self
            .title_deadline
            .unwrap_or(close_deadline)
            .min(close_deadline);
        let _ = self.connection.close(None);
        let _ = self.connection.get_mut().stream.shutdown(Shutdown::Both);
    }
}

pub struct SidebarConnection {
    stream: DeadlineStream,
    caller: String,
    host: String,
    request_id: u64,
    read_only: bool,
}

fn sidebar_path(configured: &Path) -> Result<PathBuf> {
    match configured.canonicalize() {
        Ok(path) => return Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = configured.parent().context("分区连接缺少父目录")?;
    let candidates = std::fs::read_dir(parent)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sock")
        })
        .collect::<Vec<_>>();
    ensure!(
        candidates.len() == 1,
        "分区连接已更换，无法唯一确定新 socket"
    );
    Ok(candidates[0].canonicalize()?)
}

impl SidebarConnection {
    pub fn reader(config: &crate::config::Config, timeout: Duration) -> Result<Self> {
        let mut connection = Self::connect(config, timeout)?;
        connection.read_only = true;
        Ok(connection)
    }

    pub fn connect(config: &crate::config::Config, timeout: Duration) -> Result<Self> {
        use std::os::unix::fs::FileTypeExt;
        let path = sidebar_path(&config.sidebar_socket)?;
        let metadata = path.metadata()?;
        let parent = path.parent().context("分区连接缺少父目录")?.metadata()?;
        let owner = rustix::process::getuid().as_raw();
        ensure!(
            metadata.file_type().is_socket()
                && metadata.uid() == owner
                && metadata.mode() & 0o7777 == 0o600
                && parent.uid() == owner
                && parent.mode() & 0o022 == 0,
            "分区连接所有者或权限无效"
        );
        let deadline = deadline_after(timeout)?;
        Ok(Self {
            stream: DeadlineStream {
                stream: connect_unix(&path, deadline)?,
                deadline,
            },
            caller: config.sidebar_caller.clone(),
            host: config.host_id.clone(),
            request_id: 0,
            read_only: false,
        })
    }

    fn receive(&mut self) -> Result<Value> {
        let mut header = [0; 4];
        self.stream.read_exact(&mut header)?;
        let length = u32::from_le_bytes(header) as usize;
        ensure!(length <= 8 * 1024 * 1024, "分区响应过大");
        let mut bytes = vec![0; length];
        self.stream.read_exact(&mut bytes)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

impl Rpc for SidebarConnection {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let reading = matches!(method, "list_threads" | "list_projects");
        ensure!(
            !self.read_only || reading,
            "只读分区连接不允许调用 {method}"
        );
        ensure!(
            reading || method == "move_thread_to_sidebar_section",
            "分区连接不允许调用 {method}"
        );
        self.request_id = self.request_id.checked_add(1).context("分区请求编号耗尽")?;
        let request = json!({"jsonrpc":"2.0","id":self.request_id,"method":"tools/call",
            "params":{"namespace":"codex_app","tool":method,"arguments":params,
                "callerSource":"codex","hostId":self.host,"threadId":self.caller,
                "turnId":"session-board-sections","callId":uuid::Uuid::new_v4().to_string()}});
        let bytes = serde_json::to_vec(&request)?;
        self.stream
            .write_all(&u32::try_from(bytes.len())?.to_le_bytes())?;
        self.stream.write_all(&bytes)?;
        loop {
            let response = self.receive()?;
            if let Some(result) = response_result(response, self.request_id, method)? {
                ensure!(
                    result["success"] == true,
                    "分区工具失败: {}",
                    result["contentItems"]
                );
                let content = result["contentItems"]
                    .as_array()
                    .context("分区回执缺少内容")?;
                let text = content
                    .iter()
                    .find(|item| item["type"] == "inputText")
                    .and_then(|item| item["text"].as_str())
                    .context("分区回执缺少 JSON")?;
                return Ok(serde_json::from_str(text)?);
            }
        }
    }
}

pub fn list_threads(
    reader: &mut dyn Rpc,
    page_size: usize,
    max_pages: usize,
) -> Result<(Vec<Value>, bool)> {
    let mut threads = Vec::new();
    let mut identifiers = HashSet::new();
    let mut cursors = HashSet::new();
    let mut cursor = Value::Null;
    for _ in 0..max_pages {
        let result = reader.call("thread/list", thread_list_params(page_size, cursor))?;
        append_threads(&result, &mut identifiers, &mut threads)?;
        cursor = result.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() || cursor.as_str() == Some("") {
            return Ok((threads, true));
        }
        let next = cursor.as_str().context("会话分页游标必须是字符串")?;
        ensure!(
            cursors.insert(next.to_owned()),
            "会话分页游标重复，本次发现未完成"
        );
    }
    Ok((threads, false))
}

fn thread_list_params(page_size: usize, cursor: Value) -> Value {
    json!({
        "limit": page_size, "cursor": cursor, "sortKey": "updated_at",
        "sortDirection": "desc", "modelProviders": [],
        "sourceKinds": ["cli", "vscode", "appServer"],
        "archived": false, "useStateDbOnly": true
    })
}

fn append_threads(
    result: &Value,
    identifiers: &mut HashSet<String>,
    threads: &mut Vec<Value>,
) -> Result<()> {
    for entry in result["data"]
        .as_array()
        .context("会话列表缺少 data 数组")?
    {
        let identifier = entry["id"].as_str().context("会话缺少字符串 ID")?;
        if identifiers.insert(identifier.to_owned()) {
            threads.push(entry.clone());
        }
    }
    Ok(())
}

pub fn latest_turn(reader: &mut dyn Rpc, id: &str) -> Result<Value> {
    let result = reader.call(
        "thread/turns/list",
        json!({
            "threadId": id, "limit": 1, "sortDirection": "desc", "itemsView": "full"
        }),
    )?;
    match result.get("data") {
        None | Some(Value::Null) => Ok(json!({})),
        Some(Value::Array(turns)) => Ok(turns.first().map(summarize_turn).unwrap_or(json!({}))),
        _ => bail!("轮次列表的 data 必须是数组"),
    }
}

pub fn summarize_turn(turn: &Value) -> Value {
    let items = turn["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut users: Vec<_> = items
        .iter()
        .filter(|item| item["type"] == "userMessage")
        .filter_map(|item| item["id"].as_str())
        .collect();
    users.sort_unstable();
    json!({
        "id": turn["id"], "status": turn["status"],
        "startedAt": turn["startedAt"], "completedAt": turn["completedAt"],
        "userItems": users,
        "hasFinal": items.iter().any(|item| {
            item["type"] == "agentMessage" && item["phase"] == "final_answer"
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tungstenite::protocol::Role as WebSocketRole;

    #[derive(Default)]
    struct MockRpc {
        replies: VecDeque<Value>,
        calls: Vec<(String, Value)>,
    }

    fn sidebar_pair() -> Result<(SidebarConnection, UnixStream)> {
        let (client, server) = UnixStream::pair()?;
        client.set_nonblocking(true)?;
        Ok((
            SidebarConnection {
                stream: DeadlineStream {
                    stream: client,
                    deadline: deadline_after(Duration::from_millis(300))?,
                },
                caller: "caller".into(),
                host: "local".into(),
                request_id: 0,
                read_only: false,
            },
            server,
        ))
    }

    #[test]
    fn sidebar_frames_wrap_caller_and_preserve_target_and_decode_content() -> Result<()> {
        let (mut client, mut server) = sidebar_pair()?;
        let worker = thread::spawn(move || -> Result<()> {
            let mut header = [0; 4];
            server.read_exact(&mut header)?;
            let mut bytes = vec![0; u32::from_le_bytes(header) as usize];
            server.read_exact(&mut bytes)?;
            let request: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(request["params"]["threadId"], "caller");
            assert_eq!(request["params"]["arguments"]["threadId"], "target");
            assert_eq!(request["params"]["namespace"], "codex_app");
            assert_eq!(request["params"]["tool"], "move_thread_to_sidebar_section");
            let response = serde_json::to_vec(&json!({"id":request["id"],"result":{
                "success":true,"contentItems":[{"type":"inputText","text":"{\"ok\":true}"}]}}))?;
            server.write_all(&u32::try_from(response.len())?.to_le_bytes())?;
            server.write_all(&response)?;
            Ok(())
        });
        assert_eq!(
            client.call(
                "move_thread_to_sidebar_section",
                json!({"threadId":"target"})
            )?["ok"],
            true
        );
        worker.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn sidebar_denies_other_tools_before_io_and_bounds_missing_responses() -> Result<()> {
        let (mut client, mut server) = sidebar_pair()?;
        client.read_only = true;
        for method in [
            "move_thread_to_sidebar_section",
            "create_sidebar_section",
            "delete_sidebar_section",
            "send_message_to_thread",
            "thread/unarchive",
        ] {
            assert!(client.call(method, json!({})).is_err());
        }
        server.set_nonblocking(true)?;
        assert_eq!(
            server.read(&mut [0; 1]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        client.stream.deadline = deadline_after(Duration::from_millis(30))?;
        let started = Instant::now();
        assert!(client.call("list_threads", json!({})).is_err());
        assert!(started.elapsed() < Duration::from_millis(150));
        Ok(())
    }

    #[test]
    fn sidebar_successor_is_unique_and_permissions_are_rechecked() -> Result<()> {
        let sandbox = Sandbox::create()?;
        let (path, _listener) = sandbox.listener()?;
        let old = sandbox.0.join("previous.sock");
        assert_eq!(sidebar_path(&old)?, path);
        let config = crate::config::Config {
            sidebar_socket: old.clone(),
            ..crate::config::Config::default()
        };
        let _connection = SidebarConnection::connect(&config, Duration::from_millis(100))?;
        fs::set_permissions(&path, Permissions::from_mode(0o666))?;
        assert!(SidebarConnection::connect(&config, Duration::from_millis(100)).is_err());
        let _another = UnixListener::bind(sandbox.0.join("another.sock"))?;
        assert!(sidebar_path(&old).is_err());
        Ok(())
    }

    impl Rpc for MockRpc {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            self.calls.push((method.to_owned(), params));
            self.replies.pop_front().context("模拟回执耗尽")
        }
    }

    struct Sandbox(PathBuf);

    impl Sandbox {
        fn create() -> Result<Self> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let identifier = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("board-rpc-{}-{identifier}", std::process::id()));
            fs::create_dir(&path)?;
            let sandbox = Self(path);
            fs::set_permissions(&sandbox.0, Permissions::from_mode(0o700))?;
            Ok(sandbox)
        }

        fn listener(&self) -> Result<(PathBuf, UnixListener)> {
            let path = self.0.join("desktop.sock");
            let listener = UnixListener::bind(&path)?;
            fs::set_permissions(&path, Permissions::from_mode(0o600))?;
            Ok((path, listener))
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn local_connection(role: Role) -> Result<(DesktopConnection, WebSocket<UnixStream>)> {
        let (client, server) = UnixStream::pair()?;
        client.set_nonblocking(true)?;
        server.set_read_timeout(Some(Duration::from_secs(1)))?;
        server.set_write_timeout(Some(Duration::from_secs(1)))?;
        let deadline = deadline_after(Duration::from_secs(1))?;
        let connection = WebSocket::from_raw_socket(
            DeadlineStream {
                stream: client,
                deadline,
            },
            WebSocketRole::Client,
            None,
        );
        let client = DesktopConnection {
            connection,
            request_id: 0,
            timeout: Duration::from_secs(1),
            role,
            title_deadline: (role != Role::Reader).then_some(deadline),
        };
        Ok((
            client,
            WebSocket::from_raw_socket(server, WebSocketRole::Server, None),
        ))
    }

    fn read_json(server: &mut WebSocket<UnixStream>) -> Result<Value> {
        Ok(serde_json::from_slice(&server.read()?.into_data())?)
    }

    fn send_response(server: &mut WebSocket<UnixStream>, response: Value) -> Result<()> {
        server.send(Message::Text(response.to_string().into()))?;
        Ok(())
    }

    #[test]
    fn pagination_preserves_scope_first_occurrence_and_order() -> Result<()> {
        let mut reader = MockRpc {
            replies: VecDeque::from([
                json!({"data": [{"id": "b", "name": "first"}], "nextCursor": "next"}),
                json!({"data": [{"id": "b", "name": "second"}, {"id": "a"}]}),
            ]),
            ..MockRpc::default()
        };
        let (threads, complete) = list_threads(&mut reader, 37, 9)?;
        assert!(complete);
        assert_eq!(
            threads,
            vec![json!({"id": "b", "name": "first"}), json!({"id": "a"})]
        );
        assert_eq!(
            reader.calls[0],
            (
                "thread/list".to_owned(),
                json!({
                    "limit": 37, "cursor": null, "sortKey": "updated_at", "sortDirection": "desc",
                    "modelProviders": [], "sourceKinds": ["cli", "vscode", "appServer"],
                    "archived": false, "useStateDbOnly": true
                })
            )
        );
        assert_eq!(reader.calls[1].1["cursor"], "next");
        Ok(())
    }

    #[test]
    fn pagination_discloses_limits_and_rejects_cursor_loops() -> Result<()> {
        let page = json!({"data": [{"id": "sample"}], "nextCursor": "same"});
        let mut reader = MockRpc {
            replies: VecDeque::from([page.clone(), page.clone(), page]),
            ..MockRpc::default()
        };
        assert!(!list_threads(&mut reader, 100, 1)?.1);
        assert!(list_threads(&mut reader, 100, 2).is_err());
        assert_eq!(list_threads(&mut reader, 100, 0)?, (Vec::new(), false));
        Ok(())
    }

    #[test]
    fn latest_turn_requests_one_summary_and_drops_message_bodies() -> Result<()> {
        let mut reader = MockRpc {
            replies: VecDeque::from([json!({"data": [{
                "id": "turn", "status": "completed", "startedAt": 1.25, "completedAt": 2.5,
                "items": [
                    {"id": "z", "type": "userMessage", "text": "private"},
                    {"id": "a", "type": "userMessage", "text": "private"},
                    {"id": "reply", "type": "agentMessage", "phase": "final_answer", "text": "private"}
                ]
            }]})]),
            ..MockRpc::default()
        };
        let summary = latest_turn(&mut reader, "conversation")?;
        assert_eq!(
            summary,
            json!({
                "id": "turn", "status": "completed", "startedAt": 1.25, "completedAt": 2.5,
                "userItems": ["a", "z"], "hasFinal": true
            })
        );
        assert!(!summary.to_string().contains("private"));
        assert_eq!(
            reader.calls[0],
            (
                "thread/turns/list".to_owned(),
                json!({
                    "threadId": "conversation", "limit": 1, "sortDirection": "desc", "itemsView": "full"
                })
            )
        );
        Ok(())
    }

    #[test]
    fn empty_turns_and_missing_summary_fields_are_safe() -> Result<()> {
        let mut reader = MockRpc {
            replies: VecDeque::from([json!({"data": []}), json!({})]),
            ..MockRpc::default()
        };
        assert_eq!(latest_turn(&mut reader, "sample")?, json!({}));
        assert_eq!(latest_turn(&mut reader, "sample")?, json!({}));
        assert_eq!(
            summarize_turn(&json!({"id": "turn"})),
            json!({
                "id": "turn", "status": null, "startedAt": null, "completedAt": null,
                "userItems": [], "hasFinal": false
            })
        );
        Ok(())
    }

    #[test]
    fn roles_reject_all_unrelated_methods_before_io() -> Result<()> {
        for role in [Role::Reader, Role::TitleWriter] {
            let (mut connection, _server) = local_connection(role)?;
            for method in [
                "turn/start",
                "thread/archive",
                "thread/resume",
                "turn/interrupt",
                "thread/goal/set",
            ] {
                assert!(connection.call(method, json!({})).is_err());
            }
            let denied = if role == Role::Reader {
                "thread/name/set"
            } else {
                "thread/list"
            };
            assert!(connection.call(denied, json!({})).is_err());
            assert_eq!(connection.request_id, 0);
        }
        Ok(())
    }

    #[test]
    fn archive_role_rejects_other_writes_before_io() -> Result<()> {
        let (mut connection, _server) = local_connection(Role::ArchiveWriter)?;
        for method in [
            "turn/start",
            "thread/resume",
            "thread/unarchive",
            "thread/name/set",
        ] {
            assert!(connection.call(method, json!({})).is_err());
        }
        for method in [
            "initialize",
            "thread/list",
            "thread/read",
            "thread/turns/list",
            "thread/archive",
        ] {
            Role::ArchiveWriter.check(method)?;
        }
        assert_eq!(connection.request_id, 0);
        Ok(())
    }

    #[test]
    fn each_role_accepts_exactly_its_three_declared_methods() -> Result<()> {
        let roles = [
            (
                Role::Reader,
                ["initialize", "thread/list", "thread/turns/list"],
            ),
            (
                Role::TitleWriter,
                ["initialize", "thread/read", "thread/name/set"],
            ),
        ];
        for (role, methods) in roles {
            let (mut connection, mut server) = local_connection(role)?;
            let worker = thread::spawn(move || -> Result<()> {
                for _ in 0..3 {
                    let request = read_json(&mut server)?;
                    send_response(&mut server, json!({"id": request["id"], "result": {}}))?;
                }
                Ok(())
            });
            for method in methods {
                assert_eq!(connection.call(method, json!({}))?, json!({}));
            }
            worker.join().map_err(|_| anyhow!("模拟服务异常"))??;
        }
        Ok(())
    }

    #[test]
    fn native_reader_rejects_writes_and_keeps_the_history_reader_boundary() -> Result<()> {
        for method in ["initialize", "thread/list", "thread/read", "threadSection/list"] {
            Role::NativeReader.check(method)?;
        }
        let (mut connection, _server) = local_connection(Role::NativeReader)?;
        for method in ["thread/archive", "thread/name/set", "threadSection/move", "turn/start"] {
            assert!(connection.call(method, json!({})).is_err());
        }
        assert_eq!(connection.request_id, 0);
        assert!(Role::Reader.check("thread/read").is_err());
        assert!(Role::Reader.check("threadSection/list").is_err());
        Ok(())
    }

    #[test]
    fn notifications_and_other_ids_do_not_complete_a_request() -> Result<()> {
        let (mut connection, mut server) = local_connection(Role::Reader)?;
        let worker = thread::spawn(move || -> Result<()> {
            let request = read_json(&mut server)?;
            send_response(
                &mut server,
                json!({"method": "notice", "id": request["id"]}),
            )?;
            send_response(&mut server, json!({"id": 900, "result": "wrong"}))?;
            send_response(
                &mut server,
                json!({"id": request["id"], "result": {"data": []}}),
            )
        });
        assert_eq!(
            connection.call("thread/list", json!({}))?,
            json!({"data": []})
        );
        worker.join().map_err(|_| anyhow!("模拟服务异常"))??;
        Ok(())
    }

    #[test]
    fn request_errors_are_downcastable_and_connection_remains_usable() -> Result<()> {
        let (mut connection, mut server) = local_connection(Role::Reader)?;
        let worker = thread::spawn(move || -> Result<()> {
            let request = read_json(&mut server)?;
            send_response(
                &mut server,
                json!({
                    "id": request["id"], "error": {"code": -32602, "message": "unreadable"}
                }),
            )?;
            let request = read_json(&mut server)?;
            send_response(
                &mut server,
                json!({"id": request["id"], "result": {"data": []}}),
            )
        });
        let error = connection
            .call("thread/turns/list", json!({}))
            .expect_err("请求应失败");
        let request = error
            .downcast_ref::<RequestError>()
            .context("请求错误类型丢失")?;
        assert_eq!(request.method, "thread/turns/list");
        assert_eq!(request.details["code"], -32602);
        assert_eq!(
            connection.call("thread/list", json!({}))?,
            json!({"data": []})
        );
        worker.join().map_err(|_| anyhow!("模拟服务异常"))??;
        Ok(())
    }

    #[test]
    fn transport_failure_is_not_a_request_error() -> Result<()> {
        let (mut connection, server) = local_connection(Role::Reader)?;
        drop(server);
        let error = connection
            .call("thread/list", json!({}))
            .expect_err("连接应失败");
        assert!(error.downcast_ref::<RequestError>().is_none());
        Ok(())
    }

    #[test]
    fn socket_validation_requires_exact_permissions_and_socket_type() -> Result<()> {
        let sandbox = Sandbox::create()?;
        let (path, _listener) = sandbox.listener()?;
        assert_eq!(verified_socket(&path)?, path);
        fs::set_permissions(&path, Permissions::from_mode(0o660))?;
        assert!(verified_socket(&path).is_err());
        fs::set_permissions(&path, Permissions::from_mode(0o600))?;
        fs::set_permissions(&sandbox.0, Permissions::from_mode(0o750))?;
        assert!(verified_socket(&path).is_err());
        fs::set_permissions(&sandbox.0, Permissions::from_mode(0o700))?;
        let file = sandbox.0.join("regular");
        fs::write(&file, [])?;
        fs::set_permissions(&file, Permissions::from_mode(0o600))?;
        assert!(verified_socket(&file).is_err());
        Ok(())
    }

    fn delayed_desktop(listener: UnixListener, delay: Duration) -> Result<()> {
        let (stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(1)))?;
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;
        let mut server = tungstenite::accept(stream).map_err(|error| anyhow!("{error}"))?;
        for _ in 0..3 {
            let mut request = read_json(&mut server)?;
            if request["method"] == "initialized" {
                request = read_json(&mut server)?;
            }
            thread::sleep(delay);
            let result = if request["method"] == "thread/read" {
                json!({"thread": {"name": "完整标题"}})
            } else {
                json!({})
            };
            if send_response(&mut server, json!({"id": request["id"], "result": result})).is_err() {
                return Ok(());
            }
        }
        Ok(())
    }

    #[test]
    fn title_deadline_is_shared_across_initialize_read_and_write() -> Result<()> {
        let sandbox = Sandbox::create()?;
        let (path, listener) = sandbox.listener()?;
        let worker = thread::spawn(move || delayed_desktop(listener, Duration::from_millis(80)));
        let started = Instant::now();
        let mut writer =
            DesktopConnection::connect(&path, Duration::from_millis(210), Role::TitleWriter)?;
        writer.call(
            "thread/read",
            json!({"threadId": "sample", "includeTurns": false}),
        )?;
        assert!(
            writer
                .call(
                    "thread/name/set",
                    json!({"threadId": "sample", "name": "标题"})
                )
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(600));
        drop(writer);
        worker.join().map_err(|_| anyhow!("模拟服务异常"))??;
        Ok(())
    }

    #[test]
    fn title_deadline_expires_during_websocket_handshake() -> Result<()> {
        let sandbox = Sandbox::create()?;
        let (path, listener) = sandbox.listener()?;
        let worker = thread::spawn(move || -> io::Result<()> {
            let (_stream, _) = listener.accept()?;
            thread::sleep(Duration::from_millis(120));
            Ok(())
        });
        let started = Instant::now();
        assert!(
            DesktopConnection::connect(&path, Duration::from_millis(50), Role::TitleWriter)
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(100));
        worker.join().map_err(|_| anyhow!("模拟服务异常"))??;
        Ok(())
    }

    #[test]
    fn title_deadline_bounds_a_full_unix_listener_queue() -> Result<()> {
        let sandbox = Sandbox::create()?;
        let (path, listener) = sandbox.listener()?;
        rustix::net::listen(&listener, 0)?;
        let _queued = connect_unix(&path, deadline_after(Duration::from_secs(1))?)?;
        let started = Instant::now();
        assert!(
            DesktopConnection::connect(&path, Duration::from_millis(50), Role::TitleWriter)
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        Ok(())
    }

    #[test]
    fn title_budget_never_exceeds_three_seconds() -> Result<()> {
        let sandbox = Sandbox::create()?;
        let (path, listener) = sandbox.listener()?;
        let worker = thread::spawn(move || -> Result<()> {
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(1)))?;
            let mut server = tungstenite::accept(stream).map_err(|error| anyhow!("{error}"))?;
            let request = read_json(&mut server)?;
            assert_eq!(request["method"], "initialize");
            send_response(&mut server, json!({"id": request["id"], "result": {}}))?;
            assert_eq!(read_json(&mut server)?, json!({"method": "initialized"}));
            Ok(())
        });
        let writer = DesktopConnection::connect(&path, Duration::from_secs(20), Role::TitleWriter)?;
        let deadline = writer.title_deadline.context("缺少标题总截止时间")?;
        assert!(remaining(deadline)? <= Duration::from_secs(3));
        worker.join().map_err(|_| anyhow!("模拟服务异常"))??;
        Ok(())
    }

    fn stdio_command(script: &str) -> Vec<String> {
        vec!["/bin/sh".to_owned(), "-c".to_owned(), script.to_owned()]
    }

    #[test]
    fn stdio_handshake_filters_noise_and_reaps_child_on_drop() -> Result<()> {
        let script = r#"read -r request
printf 'noise\n{"id":1,"result":{}}\n'
read -r notification
read -r request
printf '{"method":"notice"}\n{"id":999,"result":{}}\n{"id":2,"result":{"data":[]}}\n'
while read -r request; do :; done"#;
        let mut reader = StdioReader::connect(&stdio_command(script), Duration::from_secs(1))?;
        assert!(reader.call("thread/name/set", json!({})).is_err());
        assert_eq!(reader.call("thread/list", json!({}))?, json!({"data": []}));
        let child_id = rustix::process::Pid::from_raw(reader._process.0.id().try_into()?);
        drop(reader);
        assert!(matches!(
            rustix::process::waitpid(child_id, rustix::process::WaitOptions::NOHANG),
            Err(Errno::CHILD)
        ));
        Ok(())
    }

    #[test]
    fn stdio_request_errors_are_recoverable() -> Result<()> {
        let script = r#"read -r request
printf '{"id":1,"result":{}}\n'
read -r notification
read -r request
printf '{"id":2,"error":{"message":"unreadable"}}\n'
read -r request
printf '{"id":3,"result":{"data":[]}}\n'"#;
        let mut reader = StdioReader::connect(&stdio_command(script), Duration::from_secs(1))?;
        let error = reader
            .call("thread/turns/list", json!({}))
            .expect_err("请求应失败");
        assert!(error.downcast_ref::<RequestError>().is_some());
        assert_eq!(reader.call("thread/list", json!({}))?, json!({"data": []}));
        Ok(())
    }

    #[test]
    fn stdio_accepts_a_final_json_record_without_newline() -> Result<()> {
        let script = r#"read -r request
printf '{"id":1,"result":{}}\n'
read -r notification
read -r request
printf '{"id":2,"result":{"data":[]}}'"#;
        let mut reader = StdioReader::connect(&stdio_command(script), Duration::from_secs(1))?;
        assert_eq!(reader.call("thread/list", json!({}))?, json!({"data": []}));
        Ok(())
    }

    #[test]
    fn stdio_timeout_is_bounded_without_a_reader_thread() -> Result<()> {
        let script = r#"read -r request
printf '{"id":1,"result":{}}\n'
read -r notification
read -r request
while :; do :; done"#;
        let mut reader = StdioReader::connect(&stdio_command(script), Duration::from_millis(100))?;
        let started = Instant::now();
        assert!(reader.call("thread/list", json!({})).is_err());
        drop(reader);
        assert!(started.elapsed() < Duration::from_millis(500));
        Ok(())
    }
}
