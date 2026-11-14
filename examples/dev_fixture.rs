use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
use std::os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "dev_fixture/state.rs"]
mod state;

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args().collect();
    let mode = arguments.get(1).context("需要 prepare/serve/history/open")?;
    let root = Path::new(arguments.get(2).context("缺少私有环境目录")?);
    match mode.as_str() {
        "prepare" => state::prepare(root, arguments.get(3).context("缺少端口")?.parse()?),
        "serve" => serve(root, arguments.get(3)),
        "history" => history(root),
        "open" => {
            let target = arguments.get(3).context("缺少导航目标")?;
            ensure!(target.starts_with("codex://threads/"), "只接受测试导航");
            let mut file = std::fs::OpenOptions::new().create(true).append(true)
                .open(root.join("navigation.jsonl"))?;
            writeln!(file, "{}", json!({"target":target}))?;
            Ok(())
        }
        _ => bail!("开发 fixture 禁止外部命令"),
    }
}

fn history(root: &Path) -> Result<()> {
    use codex_session_board::rpc::{DesktopConnection, Role, Rpc};
    let mut connection = DesktopConnection::connect(
        &root.join("desktop/rpc.sock"), Duration::from_secs(3), Role::Reader)?;
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        if request["id"].is_null() {
            continue;
        }
        let result = connection.call(request["method"].as_str().unwrap_or(""), request["params"].clone())?;
        println!("{}", json!({"id":request["id"],"result":result}));
    }
    Ok(())
}

fn reply(surface: &mut state::Surface, request: &Value, sidebar: bool) -> Value {
    let (method, params) = if sidebar {
        (&request["params"]["tool"], &request["params"]["arguments"])
    } else {
        (&request["method"], &request["params"])
    };
    match surface.answer(method.as_str().unwrap_or(""), params) {
        Ok(value) if sidebar => json!({"id":request["id"],"result":{
            "success":true,"contentItems":[{"type":"inputText","text":value.to_string()}]}}),
        Ok(value) => json!({"id":request["id"],"result":value}),
        Err(error) => json!({"id":request["id"],"error":{"code":-32602,"message":error.to_string()}}),
    }
}

fn listener(root: &Path, name: &str) -> Result<UnixListener> {
    let path = root.join(name);
    ensure!(!path.exists(), "fixture socket 已存在，请先确认旧环境退出");
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

struct BoardProcess(std::process::Child);

impl Drop for BoardProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn serve(root: &Path, program: Option<&String>) -> Result<()> {
    let desktop = listener(root, "desktop/rpc.sock")?;
    let sidebar = listener(root, "sidebar/rpc.sock")?;
    let surface = Arc::new(Mutex::new(state::Surface::load(root)?));
    let mut board = program.map(|program| -> Result<BoardProcess> {
        let output = std::fs::OpenOptions::new().append(true).create(true).open(root.join("service.log"))?;
        let child = std::process::Command::new(program).arg("--config").arg(root.join("config.json"))
            .arg("serve").stdout(output.try_clone()?).stderr(output).spawn()?;
        std::fs::write(root.join("service-child.pending"), json!({"pid":child.id(),"exit":null}).to_string())?;
        std::fs::rename(root.join("service-child.pending"), root.join("service-child.json"))?;
        Ok(BoardProcess(child))
    }).transpose()?;
    println!("{}", json!({"fixture":true,"root":root}));
    loop {
        if let Some(board) = &mut board {
            ensure!(board.0.try_wait()?.is_none(), "开发看板已退出");
        }
        for (listener, framed) in [(&desktop, false), (&sidebar, true)] {
            match listener.accept() {
                Ok((stream, _)) => {
                    let surface = Arc::clone(&surface);
                    std::thread::spawn(move || {
                        if let Err(error) = connection(stream, surface, framed) {
                            eprintln!("开发 fixture 连接结束：{error}");
                        }
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn connection(stream: UnixStream, surface: Arc<Mutex<state::Surface>>, framed: bool) -> Result<()> {
    if stream.peer_addr().is_err() {
        return Ok(());
    }
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    if framed {
        return sidebar_connection(stream, surface);
    }
    let mut socket = tungstenite::accept(stream)?;
    socket.get_mut().set_read_timeout(None)?;
    loop {
        let message = socket.read()?;
        if message.is_close() {
            return Ok(());
        }
        if !message.is_text() {
            continue;
        }
        let request: Value = serde_json::from_str(message.to_text()?)?;
        if !request["id"].is_null() {
            let response = shared_reply(&surface, &request, false)?;
            socket.send(tungstenite::Message::Text(response.to_string().into()))?;
        }
    }
}

fn sidebar_connection(mut stream: UnixStream, surface: Arc<Mutex<state::Surface>>) -> Result<()> {
    loop {
        let mut header = [0; 4];
        if stream.read_exact(&mut header).is_err() {
            return Ok(());
        }
        let size = u32::from_le_bytes(header) as usize;
        ensure!(size <= 65536, "fixture 帧过大");
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes)?;
        let request: Value = serde_json::from_slice(&bytes)?;
        let response = shared_reply(&surface, &request, true)?;
        let bytes = serde_json::to_vec(&response)?;
        stream.write_all(&(u32::try_from(bytes.len())?).to_le_bytes())?;
        stream.write_all(&bytes)?;
    }
}

fn shared_reply(surface: &Mutex<state::Surface>, request: &Value, framed: bool) -> Result<Value> {
    let mut locked = surface.lock().map_err(|_| anyhow::anyhow!("fixture 锁损坏"))?;
    Ok(reply(&mut locked, request, framed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_nonblocking_stream_serves_a_delayed_sidebar_frame() -> Result<()> {
        let directory = tempfile::tempdir()?;
        state::prepare(directory.path(), 18888)?;
        let surface = Arc::new(Mutex::new(state::Surface::load(directory.path())?));
        let (mut client, server) = UnixStream::pair()?;
        server.set_nonblocking(true)?;
        client.set_read_timeout(Some(Duration::from_secs(2)))?;
        let worker = std::thread::spawn(move || connection(server, surface, true));
        std::thread::sleep(Duration::from_millis(50));
        let request = serde_json::to_vec(&json!({"id":1,"params":{
            "tool":"threadSection/list","arguments":{}}}))?;
        client.write_all(&(u32::try_from(request.len())?).to_le_bytes())?;
        client.write_all(&request)?;
        let mut header = [0; 4];
        client.read_exact(&mut header)?;
        let mut response = vec![0; u32::from_le_bytes(header) as usize];
        client.read_exact(&mut response)?;
        assert_eq!(serde_json::from_slice::<Value>(&response)?["result"]["success"], true);
        drop(client);
        worker.join().expect("fixture connection thread")?;
        Ok(())
    }
}
