use anyhow::Result;
use codex_session_board::{app::App, config::Config, mcp, store::Store};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
};

#[test]
fn column_sort_defaults_persistence_and_independent_conflicts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    let defaults = store.column_sorts()?;
    assert_eq!(defaults.as_object().map(|value| value.len()), Some(5));
    assert_eq!(defaults["to_archive"]["direction"], "asc");
    assert_eq!(defaults["review"]["direction"], "desc");
    for state in ["unknown", "in_progress", "blocked"] {
        assert_eq!(defaults[state]["direction"], "desc");
    }
    let change = json!({"state":"review","field":"updated_at","direction":"asc","version":0});
    assert_eq!(store.save_column_sort(&change)?["preference"]["version"], 1);
    assert!(store.save_column_sort(&change).is_err());
    store.save_column_sort(
        &json!({"state":"unknown","field":"updated_at","direction":"asc","version":0}),
    )?;
    drop(store);
    let reopened = Store::open(directory.path(), true)?;
    assert_eq!(reopened.column_sorts()?["review"]["direction"], "asc");
    assert_eq!(reopened.column_sorts()?["unknown"]["version"], 1);
    assert_eq!(reopened.column_sorts()?["blocked"]["version"], 0);
    let unchanged = json!({"state":"review","field":"updated_at","direction":"asc","version":1});
    assert_eq!(
        reopened.save_column_sort(&unchanged)?["preference"]["version"],
        1
    );
    Ok(())
}

#[test]
fn column_sort_rejects_invalid_requests_without_touching_sessions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    store.discover(&[json!({"id":"session","name":"原任务","updatedAt":1})])?;
    let database = rusqlite::Connection::open(directory.path().join("status.sqlite3"))?;
    database.execute(
        "UPDATE sessions SET note='必须保留',needs_review=1,input_revision=7,selected_at=1",
        [],
    )?;
    let before = store.sessions()?;
    let defaults = store.column_sorts()?;
    let valid = json!({"state":"review","field":"updated_at","direction":"asc","version":0});
    for (key, value) in [
        ("state", json!("completed")),
        ("field", json!("priority")),
        ("direction", json!("reverse")),
        ("version", json!(-1)),
        ("version", json!(0.5)),
        ("version", json!("0")),
        ("extra", json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(store.save_column_sort(&invalid).is_err(), "{invalid}");
    }
    assert!(store.save_column_sort(&json!({})).is_err());
    assert_eq!(store.column_sorts()?, defaults);
    store.save_column_sort(&valid)?;
    assert_eq!(store.sessions()?, before);
    assert!(store.pending_titles(None)?.is_empty());
    assert_eq!(
        database.query_row("SELECT COUNT(*) FROM changes", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

fn start_fixture(path: &Path) -> Result<(Value, Child)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    fs::write(
        path,
        json!({"data_dir":"data","port":port,"sync_titles":false,
        "codex_command":["/bin/false"],"desktop_socket":"","sidebar_socket":"",
        "auto_archive_seconds":null})
        .to_string(),
    )?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .arg("--config")
        .arg(path)
        .arg("serve")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("看板没有输出"))?;
    let mut line = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut line)?;
    Ok((serde_json::from_str(line.trim())?, child))
}

fn mcp_sort_roundtrip(path: &Path, change: &Value) -> Result<()> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .arg("--config")
        .arg(path)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing stdin"))?;
    for identifier in 1..=2 {
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":identifier,"method":"tools/call",
            "params":{"name":"board_sort","arguments":change}})
        )?;
    }
    drop(input);
    let output = child.wait_with_output()?;
    assert!(output.status.success());
    let replies: Vec<Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<_>>()?;
    assert_eq!(replies.len(), 2);
    assert_eq!(
        replies[0]["result"]["structuredContent"]["preference"]["version"],
        1
    );
    assert_eq!(replies[1]["result"]["isError"], true);
    Ok(())
}

fn check_transport(path: &Path, running: &Value) -> Result<()> {
    let url = running["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing URL"))?;
    let client = reqwest::blocking::Client::new();
    let change = json!({"state":"review","field":"updated_at","direction":"asc","version":0});
    let response = client
        .post(format!("{url}/api/column-sort"))
        .json(&change)
        .send()?;
    assert_eq!(response.status(), 403);
    mcp_sort_roundtrip(path, &change)?;
    let snapshot: Value = client.get(format!("{url}/api/sessions")).send()?.json()?;
    assert_eq!(snapshot["columnSorts"]["review"]["direction"], "asc");
    Ok(())
}

#[test]
fn column_sort_real_http_and_stdio_mcp_share_persistent_settings() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.json");
    let (running, mut child) = start_fixture(&path)?;
    let checks = check_transport(&path, &running);
    let stopped = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .arg("--config")
        .arg(&path)
        .arg("stop")
        .output();
    checks?;
    assert!(stopped?.status.success());
    let _ = child.wait();
    let reopened = Store::open(&directory.path().join("data"), false)?;
    assert_eq!(reopened.column_sorts()?["review"]["direction"], "asc");
    Ok(())
}

#[test]
fn column_sort_app_and_mcp_expose_separate_settings_operation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        ..Config::default()
    })?;
    let change = json!({"state":"to_archive","field":"updated_at","direction":"desc","version":0});
    assert_eq!(app.request("/api/column-sort", Some(&change))?["ok"], true);
    assert_eq!(
        app.snapshot()?["columnSorts"]["to_archive"]["direction"],
        "desc"
    );
    let tool = mcp::tools()["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == "board_sort"))
        .cloned();
    assert!(tool.is_some());
    assert!(
        mcp::resource()["text"]
            .as_str()
            .is_some_and(|html| html.contains("\"/api/column-sort\":\"board_sort\""))
    );
    Ok(())
}
