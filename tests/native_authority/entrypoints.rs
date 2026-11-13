use super::*;
use super::{
    archive::Engine,
    transport::{Server, section_id},
};
#[path = "../support/fixture.rs"]
mod fixture;

use fixture::{ChildGuard, PortLease};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(super) struct Live {
    path: PathBuf,
    url: String,
    csrf: String,
    child: Option<ChildGuard>,
}

fn csrf(url: &str) -> Result<String> {
    let html = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(url)
        .send()?
        .error_for_status()?
        .text()?;
    html.split("const csrf = \"")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("missing CSRF"))
}

fn cli(path: &Path, action: &str) -> Result<std::process::Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .args(["--config", path.to_str().unwrap(), action])
        .output()?)
}

impl Live {
    pub(super) fn new(config: &mut Config, directory: &Path) -> Result<Self> {
        let port = PortLease::acquire()?;
        config.port = port.port;
        let path = directory.join("config.json");
        std::fs::write(&path, serde_json::to_vec(&*config)?)?;
        let mut child = ChildGuard::spawn(env!("CARGO_BIN_EXE_codex-session-board"), &path, port)?;
        let stdout = child
            .child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("看板没有输出"))?;
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line)?;
        let started: Value = serde_json::from_str(line.trim())?;
        anyhow::ensure!(started["started"] == true, "{line}");
        let url = format!("http://127.0.0.1:{}", config.port);
        let csrf = csrf(&url)?;
        Ok(Self {
            path,
            url,
            csrf,
            child: Some(child),
        })
    }

    pub(super) fn snapshot(&self, client: &reqwest::blocking::Client) -> Result<Value> {
        Ok(client
            .get(format!("{}/api/sessions", self.url))
            .send()?
            .error_for_status()?
            .json()?)
    }

    pub(super) fn ready(&self, client: &reqwest::blocking::Client) -> Result<Value> {
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
        self.post_path(client, "/api/state", body)
    }

    pub(super) fn post_path(
        &self,
        client: &reqwest::blocking::Client,
        path: &str,
        body: &Value,
    ) -> Result<reqwest::blocking::Response> {
        Ok(client
            .post(format!("{}{path}", self.url))
            .header("X-CSRF-Token", &self.csrf)
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
        drop(self.child.take());
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

fn optional_update(live: &Live, client: &reqwest::blocking::Client, note: Option<Value>, mcp: bool) -> Result<Value> {
    let before = live.snapshot(client)?;
    let mut body = update(session(&before), "blocked", "");
    if let Some(note) = note {
        body["note"] = note;
    } else {
        body.as_object_mut().unwrap().remove("note");
    }
    if mcp {
        assert_eq!(live.mcp("board_update", body)?["structuredContent"]["ok"], true);
    } else {
        live.post(client, &body)?.error_for_status()?;
    }
    let after = live.snapshot(client)?;
    assert_eq!(session(&after)["manual"], true);
    assert_eq!(session(&after)["state"], "blocked");
    assert_eq!(session(&after)["version"], session(&before)["version"].as_i64().unwrap() + 1);
    Ok(after)
}

fn invalid_optional_notes(live: &Live, client: &reqwest::blocking::Client) -> Result<()> {
    let before = live.snapshot(client)?;
    for note in [Value::Null, json!(12), json!("字".repeat(2001))] {
        let mut body = update(session(&before), "blocked", "");
        body["note"] = note;
        assert!(!live.post(client, &body)?.status().is_success());
        assert_eq!(live.mcp("board_update", body)?["isError"], true);
        let after = live.snapshot(client)?;
        assert_eq!(session(&after)["version"], session(&before)["version"]);
        assert_eq!(session(&after)["note"], "");
    }
    let mut stale = update(session(&before), "blocked", "");
    stale["version"] = json!(0);
    assert!(!live.post(client, &stale)?.status().is_success());
    Ok(())
}

#[test]
fn built_binary_distinguishes_omitted_and_empty_notes_over_http_and_mcp() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    server.config.desktop_socket = directory.path().join("engine.sock");
    let _engine = Engine::new(&server.config.desktop_socket, std::sync::Arc::clone(&server.surface))?;
    drop(server.app()?);
    let live = Live::new(&mut server.config, directory.path())?;
    let client = reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build()?;
    live.ready(&client)?;
    for mcp in [false, true] {
        assert_eq!(session(&optional_update(&live, &client, Some(json!("")), mcp)?)["note"], "");
        optional_update(&live, &client, Some(json!("旧备注")), mcp)?;
        assert_eq!(session(&optional_update(&live, &client, None, mcp)?)["note"], "旧备注");
        assert_eq!(session(&optional_update(&live, &client, Some(json!(" \t\r\n　")), mcp)?)["note"], "");
    }
    invalid_optional_notes(&live, &client)?;
    let moves = server.surface.lock().unwrap().calls.iter()
        .filter(|method| method.as_str() == "move_thread_to_sidebar_section").count();
    assert_eq!(moves, 1);
    drop(live);
    let reopened = Live::new(&mut server.config, directory.path())?;
    let saved = reopened.ready(&client)?;
    assert_eq!(session(&saved)["state"], "blocked");
    assert_eq!(session(&saved)["manual"], true);
    assert_eq!(session(&saved)["note"], "");
    Ok(())
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
