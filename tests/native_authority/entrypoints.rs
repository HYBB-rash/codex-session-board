use super::*;
use super::{
    archive::Engine,
    transport::{Server, section_id},
};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Live {
    path: PathBuf,
    url: String,
}

fn cli(path: &Path, action: &str) -> Result<std::process::Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .args(["--config", path.to_str().unwrap(), action])
        .output()?)
}

impl Live {
    fn new(config: &mut Config, directory: &Path) -> Result<Self> {
        let available = std::net::TcpListener::bind("127.0.0.1:0")?;
        config.port = available.local_addr()?.port();
        drop(available);
        let path = directory.join("config.json");
        std::fs::write(&path, serde_json::to_vec(&*config)?)?;
        let result = cli(&path, "launch")?;
        anyhow::ensure!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(Self {
            path,
            url: format!("http://127.0.0.1:{}", config.port),
        })
    }

    fn snapshot(&self, client: &reqwest::blocking::Client) -> Result<Value> {
        Ok(client
            .get(format!("{}/api/sessions", self.url))
            .send()?
            .error_for_status()?
            .json()?)
    }

    fn ready(&self, client: &reqwest::blocking::Client) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = self.snapshot(client)?;
            if snapshot["health"]["runtimeVerified"] == true
                && snapshot["sessions"].as_array().is_some_and(|sessions| {
                    sessions.iter().any(|session| {
                        session["id"] == FIRST && session["sectionVersion"].is_string()
                    })
                })
            {
                return Ok(snapshot);
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "native service did not become ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn post(
        &self,
        client: &reqwest::blocking::Client,
        body: &Value,
    ) -> Result<reqwest::blocking::Response> {
        let html = client.get(&self.url).send()?.error_for_status()?.text()?;
        let token = html
            .split("const csrf = \"")
            .nth(1)
            .and_then(|tail| tail.split('"').next())
            .ok_or_else(|| anyhow::anyhow!("missing CSRF"))?;
        Ok(client
            .post(format!("{}/api/state", self.url))
            .header("X-CSRF-Token", token)
            .json(body)
            .send()?)
    }

    fn mcp(&self, name: &str, body: Value) -> Result<Value> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
            .args(["mcp", "--config", self.path.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"id":1,"method":"tools/call","params":{"name":name,"arguments":body}})
        )?;
        let result = child.wait_with_output()?;
        anyhow::ensure!(result.status.success(), "MCP process failed");
        Ok(serde_json::from_slice::<Value>(&result.stdout)?["result"].clone())
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = cli(&self.path, "stop");
    }
}

fn session(snapshot: &Value) -> &Value {
    snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["id"] == FIRST)
        .unwrap()
}

fn update(session: &Value, state: &str, note: &str) -> Value {
    json!({"id":FIRST,"state":state,"note":note,"version":session["version"],"inputVersion":session["inputVersion"],"sectionVersion":session["sectionVersion"]})
}

fn check_bridge(live: &Live, client: &reqwest::blocking::Client) -> Result<Value> {
    let view = live.ready(client)?;
    live.post(client, &update(session(&view), "to_archive", "HTTP 备注"))?
        .error_for_status()?;
    let snapshot = live.mcp("board_snapshot", json!({}))?["structuredContent"].clone();
    assert_eq!(session(&snapshot)["state"], "to_archive");
    assert_eq!(session(&snapshot)["note"], "HTTP 备注");
    let body = update(session(&snapshot), "review", "MCP 备注");
    assert_eq!(
        live.mcp("board_update", body.clone())?["structuredContent"]["ok"],
        true
    );
    assert_eq!(live.mcp("board_update", body)?["isError"], true);
    let snapshot = live.mcp("board_snapshot", json!({}))?["structuredContent"].clone();
    assert_eq!(session(&snapshot)["sectionVersion"], section_id("review"));
    assert_eq!(session(&snapshot)["note"], "MCP 备注");
    Ok(snapshot)
}

#[test]
fn built_binary_preserves_native_position_guards_over_http_and_mcp() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let _engine = Engine::new(
        &server.config.desktop_socket,
        std::sync::Arc::clone(&server.surface),
    )?;
    drop(server.app()?);
    let live = Live::new(&mut server.config, directory.path())?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?;
    let snapshot = check_bridge(&live, &client)?;
    server
        .surface
        .lock()
        .unwrap()
        .slots
        .insert(FIRST.into(), "to_archive".into());
    let result = live.post(&client, &update(session(&snapshot), "unknown", "不能覆盖"))?;
    assert!(!result.status().is_success());
    assert!(result.json::<Value>()?["error"].is_string());
    assert_eq!(
        server
            .surface
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|method| method.as_str() == "move_thread_to_sidebar_section")
            .count(),
        2
    );
    assert!(cli(&live.path, "stop")?.status.success());
    Ok(())
}
