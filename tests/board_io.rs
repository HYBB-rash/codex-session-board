#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/read_only_backend.rs"]
mod read_only_backend;

use anyhow::{Result, bail};
use codex_session_board::{app, config, http, mcp, observer, rpc::Rpc, titles};
use fixture::{ChildGuard, PortLease};
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
const SECOND: &str = "22222222-2222-4222-8222-222222222222";

struct Fixture {
    _directory: TempDir,
    _port: Arc<PortLease>,
    path: PathBuf,
    settings: Value,
    native_endpoint: Option<String>,
    server: Mutex<Option<ChildGuard>>,
}

impl Fixture {
    fn new(sync: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let port = PortLease::acquire().unwrap();
        let settings = json!({"data_dir":"data","port":port.port,"sync_titles":sync,
            "desktop_socket":"","codex_command":["/usr/bin/false"],"open_command":["/usr/bin/true"],
            "poll_seconds":1,"rpc_timeout_seconds":1,"runtime_poll_seconds":1});
        fs::write(&path, settings.to_string()).unwrap();
        Self {
            _directory: directory,
            _port: port,
            path,
            settings,
            native_endpoint: None,
            server: Mutex::new(None),
        }
    }

    fn app(&self) -> app::App {
        app::App::new(config::load(Some(&self.path)).unwrap()).unwrap()
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-session-board"));
        if let Some(endpoint) = &self.native_endpoint {
            command.env("CODEX_APP_SERVER_WS_URL", endpoint);
        }
        command
    }

    fn register_projects(&mut self, identifiers: &[&str]) -> PathBuf {
        let path = self._directory.path().join("projects.json");
        let mut entries = serde_json::Map::new();
        for (position, identifier) in identifiers.iter().enumerate() {
            let root = self._directory.path().join(format!("project-{position}"));
            fs::create_dir_all(&root).unwrap();
            entries.insert(
                identifier.to_string(),
                json!({"id":identifier,"name":"同名项目","rootPaths":[root]}),
            );
        }
        fs::write(&path, json!({"local-projects":entries}).to_string()).unwrap();
        self.settings["sidebar_state_file"] = json!(path);
        fs::write(&self.path, self.settings.to_string()).unwrap();
        path
    }

    fn capture_opens(&mut self) -> PathBuf {
        let path = self._directory.path().join("opened-urls");
        self.settings["open_command"] =
            json!(["/bin/sh", "-c", "printf '%s\\n' \"$1\" >> \"$0\"", path]);
        fs::write(&self.path, self.settings.to_string()).unwrap();
        path
    }

    fn mcp_calls(&self, requests: &[(&str, Value)]) -> Vec<Value> {
        let messages: Vec<_> = requests.iter().enumerate().map(|(position, (name, arguments))| {
            json!({"id":position + 1,"method":"tools/call","params":{"name":name,"arguments":arguments}})
        }).collect();
        self.mcp_requests(&messages)
    }

    fn mcp_requests(&self, requests: &[Value]) -> Vec<Value> {
        let mut child = self.command()
            .args(["mcp", "--config", self.path.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        for request in requests {
            writeln!(input, "{request}").unwrap();
        }
        drop(input);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn cli(&self, action: &str) -> std::process::Output {
        self.command()
            .args(["--config", self.path.to_str().unwrap(), action])
            .output()
            .unwrap()
    }

    fn launch(&self) -> Value {
        self.launch_in(self._directory.path())
    }

    fn launch_in(&self, directory: &std::path::Path) -> Value {
        let mut slot = self.server.lock().unwrap();
        if slot.is_some() {
            let output = self.cli("serve");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return serde_json::from_slice(&output.stdout).unwrap();
        }
        let mut child = ChildGuard::spawn_in(
            self.command(),
            &self.path,
            Arc::clone(&self._port),
            directory,
        )
        .unwrap();
        let value = child.startup_receipt().unwrap();
        *slot = Some(child);
        value
    }

    fn populate(&self) {
        let app = self.app();
        app.store
            .discover(&[
                json!({"id":FIRST,"name":"同名会话","cwd":"/test/project","updatedAt":2}),
                json!({"id":SECOND,"name":"同名会话","updatedAt":1}),
            ])
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(&self.path, self.settings.to_string());
        let _ = self.command()
            .args(["--config", self.path.to_str().unwrap(), "stop"])
            .output();
        let slot = self
            .server
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(slot.take());
    }
}

struct Native {
    name: String,
    reads: usize,
    writes: Vec<Value>,
    fail: bool,
    lose_ack: bool,
    callback: Option<Box<dyn FnOnce()>>,
    after_write: Option<Box<dyn FnOnce()>>,
}

impl Native {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            reads: 0,
            writes: vec![],
            fail: false,
            lose_ack: false,
            callback: None,
            after_write: None,
        }
    }
}

impl Rpc for Native {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        if self.fail {
            bail!("controlled title connection failure");
        }
        match method {
            "thread/read" => {
                assert_eq!(params, json!({"threadId":FIRST,"includeTurns":false}));
                self.reads += 1;
                if let Some(callback) = self.callback.take() {
                    callback();
                }
                Ok(json!({"thread":{"name":self.name}}))
            }
            "thread/name/set" => {
                self.name = params["name"].as_str().unwrap().into();
                self.writes.push(params);
                if let Some(callback) = self.after_write.take() {
                    callback();
                }
                if self.lose_ack {
                    bail!("lost acknowledgement");
                }
                Ok(json!({}))
            }
            _ => bail!("unexpected method"),
        }
    }
}

#[test]
fn title_prefixes_are_idempotent_and_preserve_other_text() {
    let body = "🧪 [项目] 新正文 [进行中] 结束";
    let original = format!("[已完成] 【待归档】 [进行中] {body}");
    let projected = titles::projected_title(&original, "review").unwrap();
    assert_eq!(projected, format!("[待审阅] {body}"));
    assert_eq!(
        titles::projected_title(&projected, "review").unwrap(),
        projected
    );
    assert!(titles::projected_title(body, "invalid").is_err());
    assert_eq!(
        titles::projected_title("", "unknown").unwrap(),
        "[待判断] 未命名会话"
    );
}

#[test]
fn title_sync_reads_full_fresh_name_and_avoids_duplicate_native_write() {
    let fixture = Fixture::new(true);
    let app = fixture.app();
    let body = format!("🧪 [项目] {}", "完整正文".repeat(350));
    let listed: String = body.chars().take(18).collect();
    app.discover(&[json!({"id":FIRST,"name":listed,"updatedAt":3})], true)
        .unwrap();
    assert_eq!(app.store.sessions().unwrap()[0]["title"], listed);
    let mut native = Native::new(&body);
    app.store
        .set_state(FIRST, Some("review"), "保留备注", 0, None)
        .unwrap();
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert_eq!(native.name, format!("[待审阅] {body}"));
    assert_eq!(native.writes.len(), 1);
    app.store
        .set_state(FIRST, Some("review"), "保留备注", 1, None)
        .unwrap();
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert_eq!(native.writes.len(), 1);
    assert!(app.store.pending_titles(None).unwrap().is_empty());
}

#[test]
fn title_failure_persists_and_lost_ack_is_reconciled() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let app = fixture.app();
    app.store
        .set_state(FIRST, Some("to_archive"), "备注", 0, None)
        .unwrap();
    let mut native = Native::new("原名");
    native.fail = true;
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    drop(app);
    let app = fixture.app();
    assert_eq!(app.store.sessions().unwrap()[0]["note"], "备注");
    assert!(app.store.sessions().unwrap()[0]["titleSyncError"].is_string());
    native.fail = false;
    native.lose_ack = true;
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert_eq!(native.name, "[待归档] 原名");
    assert_eq!(native.writes.len(), 1);
    drop(app);
    let app = fixture.app();
    assert_eq!(app.store.pending_titles(Some(FIRST)).unwrap().len(), 1);
    assert_eq!(app.store.note(FIRST).unwrap(), "备注");
    native.lose_ack = false;
    let reads = native.reads;
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert!(native.reads > reads, "重开后须读回目标会话的原生标题");
    assert_eq!(native.writes.len(), 1);
    assert!(app.store.pending_titles(None).unwrap().is_empty());
}

#[test]
fn newer_state_before_write_prevents_stale_title() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let app = fixture.app();
    app.store
        .set_state(FIRST, Some("review"), "", 0, None)
        .unwrap();
    let store = Arc::clone(&app.store);
    let mut native = Native::new("原名");
    native.callback = Some(Box::new(move || {
        store
            .set_state(FIRST, Some("completed"), "较新备注", 1, None)
            .unwrap();
    }));
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert!(native.writes.is_empty());
    assert_eq!(
        app.store.pending_titles(None).unwrap()[0]["state"],
        "completed"
    );
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert_eq!(native.name, "[已完成] 原名");
}

#[test]
fn title_ack_during_newer_manual_selection_leaves_new_job_for_retry() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let app = fixture.app();
    app.store
        .set_state(FIRST, Some("review"), "", 0, None)
        .unwrap();
    let store = Arc::clone(&app.store);
    let mut native = Native::new("原名");
    native.after_write = Some(Box::new(move || {
        store
            .set_state(FIRST, Some("completed"), "较新备注", 1, None)
            .unwrap();
    }));
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert_eq!(native.name, "[待审阅] 原名");
    assert_eq!(
        app.store.pending_titles(None).unwrap()[0]["state"],
        "completed"
    );
    titles::sync(&app.store, &mut native, FIRST).unwrap();
    assert_eq!(native.name, "[已完成] 原名");
    assert_eq!(native.writes.len(), 2);
}

#[test]
fn persistent_title_failures_rotate_behind_unattempted_jobs() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let app = fixture.app();
    app.store
        .set_state(FIRST, Some("review"), "", 0, None)
        .unwrap();
    app.store
        .set_state(SECOND, Some("review"), "", 0, None)
        .unwrap();
    app.store.title_failed(FIRST, 1, "controlled").unwrap();
    assert_eq!(
        app.store.pending_titles(None).unwrap()[0]["thread_id"],
        SECOND
    );
    app.store.title_failed(SECOND, 1, "controlled").unwrap();
    app.store
        .title_failed(FIRST, 1, "controlled again")
        .unwrap();
    assert_eq!(
        app.store.pending_titles(None).unwrap()[0]["thread_id"],
        SECOND
    );
}

#[test]
fn saved_state_reports_pending_title_without_rollback() {
    let mut fixture = Fixture::new(true);
    fixture.settings["desktop_socket"] = json!(fixture._directory.path().join("missing.sock"));
    fs::write(&fixture.path, fixture.settings.to_string()).unwrap();
    fixture.populate();
    let app = fixture.app();
    let result = app
        .request(
            "/api/state",
            Some(&json!({
                "id":FIRST,"state":"review","note":"已保存","version":0,"inputVersion":0
            })),
        )
        .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["titlePending"], true);
    assert!(
        result["titleError"]
            .as_str()
            .unwrap()
            .contains("完整退出并重新打开 Codex")
    );
    assert!(!result["titleError"].as_str().unwrap().contains("os error"));
    let row = &app.store.sessions().unwrap()[0];
    assert_eq!(row["state"], "review");
    assert_eq!(row["note"], "已保存");
}

#[test]
fn disabled_title_sync_persists_without_native_jobs() {
    let fixture = Fixture::new(false);
    fixture.populate();
    let app = fixture.app();
    app.request(
        "/api/state",
        Some(&json!({"id":FIRST,"state":"completed","note":"保留备注","version":0,"inputVersion":0})),
    )
    .unwrap();
    assert!(app.store.pending_titles(None).unwrap().is_empty());
    assert_eq!(app.store.title_epoch, None);
    let saved = app.store.sessions().unwrap().into_iter()
        .find(|row| row["id"] == FIRST).unwrap();
    assert_eq!(saved["state"], "completed");
    assert_eq!(saved["note"], "保留备注");
    assert_eq!(saved["title"], "同名会话");
}

#[test]
fn manual_save_preserves_note_when_field_is_omitted() {
    let fixture = Fixture::new(false);
    fixture.populate();
    let app = fixture.app();
    app.request(
        "/api/state",
        Some(&json!({
            "id":FIRST,"state":"review","note":"必须保留的备注","version":0,"inputVersion":0
        })),
    )
    .unwrap();
    app.request(
        "/api/state",
        Some(&json!({
            "id":FIRST,"state":"to_archive","version":1,"inputVersion":0
        })),
    )
    .unwrap();
    assert_eq!(app.store.sessions().unwrap()[0]["note"], "必须保留的备注");
}

#[test]
fn manual_save_cannot_bypass_input_conflict_by_omitting_version() {
    let fixture = Fixture::new(false);
    fixture.populate();
    let app = fixture.app();
    let turn = json!({"id":"round","userItems":["first"],"startedAt":1});
    app.store.observe(FIRST, &turn, None).unwrap();
    app.request(
        "/api/state",
        Some(&json!({
            "id":FIRST,"state":"review","note":"原备注","version":0,"inputVersion":1
        })),
    )
    .unwrap();
    app.store
        .observe(
            FIRST,
            &json!({"id":"round","userItems":["first","second"],"startedAt":1}),
            None,
        )
        .unwrap();
    let before = app.store.sessions().unwrap();
    for body in [
        json!({"id":FIRST,"state":"completed","version":1}),
        json!({"id":FIRST,"state":"completed","version":1,"inputVersion":1}),
    ] {
        assert!(app.request("/api/state", Some(&body)).is_err());
    }
    assert_eq!(app.store.sessions().unwrap(), before);
    assert_eq!(before[0]["followUp"], true);
    app.request(
        "/api/state",
        Some(&json!({
            "id":FIRST,"state":"completed","version":1,"inputVersion":2
        })),
    )
    .unwrap();
    assert_eq!(app.store.sessions().unwrap()[0]["followUp"], false);
}

struct BrokenNative(Value);

impl Rpc for BrokenNative {
    fn call(&mut self, method: &str, _: Value) -> Result<Value> {
        assert_eq!(
            method, "thread/read",
            "Malformed title data must never trigger a rename"
        );
        Ok(self.0.clone())
    }
}

#[test]
fn title_malformed_native_response_preserves_pending_job_without_rename() {
    for response in [
        json!({}),
        json!({"thread":null}),
        json!({"thread":{}}),
        json!({"thread":{"name":123}}),
        json!({"thread":{"id":SECOND,"name":"另一会话"}}),
    ] {
        let fixture = Fixture::new(true);
        fixture.populate();
        let app = fixture.app();
        app.store
            .set_state(FIRST, Some("review"), "原备注", 0, None)
            .unwrap();
        titles::sync(&app.store, &mut BrokenNative(response), FIRST).unwrap();
        let row = &app.store.sessions().unwrap()[0];
        assert_eq!(row["title"], "同名会话");
        assert_eq!(row["note"], "原备注");
        assert_eq!(row["titleSyncPending"], true);
        assert!(row["titleSyncError"].is_string());
    }
}

#[test]
fn host_origin_and_token_are_required_for_mutation() {
    assert!(http::authorized(
        Some("127.0.0.1:1234"),
        None,
        None,
        "token",
        1234,
        false
    ));
    assert!(!http::authorized(
        Some("evil:1234"),
        None,
        None,
        "token",
        1234,
        false
    ));
    assert!(!http::authorized(
        Some("127.0.0.1:1234"),
        None,
        None,
        "token",
        1234,
        true
    ));
    assert!(!http::authorized(
        Some("127.0.0.1:1234"),
        Some("https://evil"),
        Some("token"),
        "token",
        1234,
        true
    ));
    assert!(http::authorized(
        Some("127.0.0.1:1234"),
        Some("http://127.0.0.1:1234"),
        Some("token"),
        "token",
        1234,
        true
    ));
}

#[test]
fn navigation_is_canonical_and_encodes_host_as_one_value() {
    assert!(app::conversation_url("invalid; touch /tmp/invalid", "local").is_err());
    let url = app::conversation_url(FIRST, "a&extra=b").unwrap();
    let parsed = url::Url::parse(&url).unwrap();
    assert_eq!(
        parsed.query_pairs().collect::<Vec<_>>(),
        vec![("hostId".into(), "a&extra=b".into())]
    );
}

fn check_tool_visibility(definitions: &Value) {
    let tools = definitions["tools"].as_array().unwrap();
    let mut views = tools.iter().filter(|tool| tool["name"] == "board_view");
    let view = views.next().unwrap();
    assert!(views.next().is_none());
    assert_eq!(view["_meta"]["ui"]["visibility"], json!(["app","model"]));
    assert_eq!(view["_meta"]["ui"]["resourceUri"], mcp::RESOURCE);
    assert_eq!(
        view["_meta"]["openai/ui"]["entrypoints"][0]["type"],
        "global"
    );
    assert!(
        view["icons"][0]["src"]
            .as_str()
            .unwrap()
            .starts_with("data:image/svg+xml;base64,")
    );
    for tool in tools.iter().filter(|tool| tool["name"] != "board_view") {
        assert_eq!(tool["_meta"]["ui"]["visibility"], json!(["app"]));
    }
    let create = tools.iter().find(|tool| tool["name"] == "board_new").unwrap();
    assert_eq!(create["annotations"]["readOnlyHint"], false);
    assert_eq!(
        create["inputSchema"]["properties"]["projectId"],
        json!({"type":"string"})
    );
    assert_eq!(create["inputSchema"]["required"], json!([]));
    assert_eq!(create["inputSchema"]["properties"]["operationId"]["format"], "uuid");
}

#[test]
fn archive_batch_tools_share_http_history() -> Result<()> {
    let native_directory = tempfile::tempdir()?;
    let native = read_only_backend::Backend::start(native_directory.path(), SECOND)?;
    native.set("idle", "completed")?;
    let mut fixture = Fixture::new(false);
    fixture.native_endpoint = Some(format!("ws+unix://localhost{}:/rpc", native.socket.display()));
    fixture.settings["desktop_socket"] = json!(native.socket);
    fixture.register_projects(&[]);
    let started = fixture.launch();
    let origin = started["url"].as_str().unwrap();
    let url = format!("{origin}/api/archive-batches");
    let client = client();
    wait_for_native_archive_target(&client, origin)?;
    let list = || client.get(&url).send()?.error_for_status()?.json::<Value>();
    assert_eq!(list()?, json!({"batches":[]}));
    let target = json!({"id":SECOND,"title":"隔离会话","version":7,"inputVersion":2,"sectionVersion":"position"});
    let replies = fixture.mcp_calls(&[
        ("board_archive_batch_start", json!({"id":FIRST,"items":[target]})),
        ("board_archive_batch_start", json!({"id":SECOND,"items":[]})),
        ("board_archive_batches", json!({})),
    ]);
    let batch = &replies[0]["result"]["structuredContent"]["batch"];
    assert_eq!(batch["id"], FIRST);
    assert_eq!(batch["items"].as_array().unwrap().len(), 1);
    for (key, value) in target.as_object().unwrap() {
        assert_eq!(&batch["items"][0][key], value);
    }
    assert_eq!(batch["items"][0]["status"], "awaiting");
    assert_eq!(replies[1]["result"]["isError"], true);
    assert!(replies[1]["result"]["content"][0]["text"].as_str().is_some_and(|text| !text.trim().is_empty()));
    let expected = json!({"batches":[batch]});
    assert_eq!(replies[2]["result"]["structuredContent"], expected);
    assert_eq!(list()?, expected);
    let database = rusqlite::Connection::open_with_flags(fixture._directory.path().join("data/status.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let stored: i64 = database.query_row(
        "SELECT COUNT(*) FROM archive_batches WHERE id=?1", [SECOND], |row| row.get(0))?;
    assert_eq!(stored, 0, "被拒的批次仍已入库");
    let retry = fixture.mcp_calls(&[("board_archive_batch_start", json!({"id":SECOND,"items":[target]}))]);
    assert_eq!(retry[0]["result"]["structuredContent"]["batch"]["id"], SECOND);
    Ok(())
}

fn wait_for_native_archive_target(client: &reqwest::blocking::Client, origin: &str) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let view: Value = client.get(format!("{origin}/api/sessions")).send()?.error_for_status()?.json()?;
        let verified = view["health"]["runtimeVerified"] == true;
        let target_present = view["sessions"].as_array().unwrap().iter().any(|row| row["id"] == SECOND);
        if verified && target_present {
            return Ok(());
        }
        anyhow::ensure!(std::time::Instant::now() < deadline,
            "原生归档测试会话未就绪：runtimeVerified={verified}, targetPresent={target_present}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn real_stdio_mcp_handshake_and_resource_round_trip() {
    let fixture = Fixture::new(false);
    let mut requests = vec![
        json!({"id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"method":"notifications/initialized"}),
        json!({"id":2,"method":"tools/list"}),
        json!({"id":3,"method":"resources/read","params":{"uri":mcp::RESOURCE}}),
    ];
    for name in ["board_view", "/api/info", "/api/native-calibration/reset", "board_missing"] {
        requests.push(json!({"id":requests.len(),"method":"tools/call","params":{"name":name,"arguments":{}}}));
    }
    let lines = fixture.mcp_requests(&requests);
    assert_eq!(
        lines.iter().map(|reply| &reply["id"]).collect::<Vec<_>>(),
        requests.iter().filter_map(|request| request.get("id")).collect::<Vec<_>>()
    );
    assert_eq!(
        lines[0]["result"]["serverInfo"]["name"],
        "codex-session-board"
    );
    check_tool_visibility(&lines[1]["result"]);
    let resource = &lines[2]["result"]["contents"][0];
    assert_eq!(resource["uri"], mcp::RESOURCE);
    assert_eq!(resource["mimeType"], mcp::MIME);
    for domain in ["connectDomains", "resourceDomains", "frameDomains"] {
        assert_eq!(resource["_meta"]["ui"]["csp"][domain], json!([]));
    }
    assert!(!resource["text"].as_str().unwrap().contains("__CSRF_TOKEN__"));
    assert_eq!(lines[3]["result"]["structuredContent"], json!({"ready":true}));
    for reply in &lines[4..] {
        assert_eq!(reply["result"]["isError"], true);
        assert_eq!(reply["result"]["content"][0]["text"], "未知看板工具");
    }
    check_plugin_update_page(&fixture, resource["text"].as_str().unwrap()).unwrap();
}

#[test]
fn mcp_bridge_uses_http_versions_and_reports_pending_title_without_rollback() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let body = json!({"id":FIRST,"state":"review","note":"桥接备注","version":0,"inputVersion":0});
    let replies = fixture.mcp_calls(&[
        ("board_update", body.clone()),
        ("board_update", body),
        ("board_snapshot", json!({})),
        ("board_open", json!({"id":"invalid"})),
    ]);
    assert_eq!(
        replies[0]["result"]["structuredContent"]["titlePending"], true,
        "{replies:#?}"
    );
    assert_eq!(replies[1]["result"]["isError"], true);
    let row = &replies[2]["result"]["structuredContent"]["sessions"][0];
    assert_eq!(row["state"], "review");
    assert_eq!(row["note"], "桥接备注");
    assert_eq!(replies[3]["result"]["isError"], true);
}

#[test]
fn remote_cards_use_the_same_http_and_mcp_annotations_without_native_writes() {
    let mut fixture = Fixture::new(true);
    fixture.settings["legacy_mode"] = json!(true);
    fs::write(&fixture.path, fixture.settings.to_string()).unwrap();
    let captured = fixture.capture_opens();
    fixture.populate();
    let store = fixture.app().store;
    store
        .discover_remote(
            "mac & host",
            "Mac",
            &[json!({
                "id":FIRST,"hostId":"mac & host","title":"远程","updatedAt":3
            })],
        )
        .unwrap();
    let identifier = codex_session_board::store::remote::identity("mac & host", FIRST);
    let body =
        json!({"id":identifier,"state":"blocked","note":"远程备注","version":0,"inputVersion":0});
    let replies = fixture.mcp_calls(&[
        ("board_update", body.clone()),
        ("board_update", body),
        ("board_snapshot", json!({})),
        ("board_open", json!({"id":identifier})),
        (
            "board_archive",
            json!({"id":identifier,"version":1,"inputVersion":0}),
        ),
    ]);
    check_remote_replies(&replies);
    let url = fs::read_to_string(captured).unwrap();
    assert_eq!(
        url.trim(),
        app::conversation_url(FIRST, "mac & host").unwrap()
    );
    assert!(store.pending_titles(None).unwrap().is_empty());
    assert_eq!(store.sessions().unwrap().len(), 2);
    assert!(fixture.cli("stop").status.success());
    assert_eq!(
        fixture.app().store.remote_session(&identifier).unwrap()["note"],
        "远程备注"
    );
}

fn check_remote_replies(replies: &[Value]) {
    assert_eq!(
        replies[0]["result"]["structuredContent"]["titleEnabled"],
        false
    );
    assert_eq!(replies[1]["result"]["isError"], true);
    let row = &replies[2]["result"]["structuredContent"]["sessions"][0];
    assert_eq!(row["note"], "远程备注");
    assert_eq!(row["state"], "blocked");
    assert_eq!(row["hostId"], "mac & host");
    assert_eq!(replies[4]["result"]["isError"], true);
}

#[test]
fn mcp_new_session_forwards_optional_project_to_http() {
    let mut fixture = Fixture::new(false);
    fixture.register_projects(&["project-a"]);
    let replies = fixture.mcp_calls(&[
        ("board_new", json!({"projectId":"project-a"})),
        ("board_new", json!({})),
        ("board_new", json!({"projectId":"removed"})),
    ]);
    assert_eq!(
        replies[0]["result"]["structuredContent"]["url"],
        "codex://new?projectId=project-a"
    );
    assert_eq!(
        replies[1]["result"]["structuredContent"]["url"],
        "codex://threads/new"
    );
    assert_eq!(replies[2]["result"]["isError"], true);
}

#[test]
fn observer_selection_prioritizes_changes_without_starving_backfill() {
    let rows = vec![
        json!({"id":"old","updated_at":1,"observed_revision":1,"observation_error":null}),
        json!({"id":"changed","updated_at":3,"observed_revision":2,"observation_error":null}),
        json!({"id":"timeout","updated_at":2,"observed_revision":1,"observation_error":"failed"}),
    ];
    let selected = observer::history_candidates(&rows, 2);
    assert_eq!(
        selected
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["changed", "old"]
    );
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[test]
fn real_http_persistence_security_and_launcher_reuse() {
    let fixture = Fixture::new(false);
    fixture.populate();
    let running = fixture.launch();
    let url = running["url"].as_str().unwrap();
    let client = client();
    let info: Value = client
        .get(format!("{url}/api/info"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let repeated = fixture.launch();
    let repeated_info: Value = client
        .get(format!("{url}/api/info"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(info["pid"], repeated_info["pid"]);
    assert_eq!(repeated["started"], false);
    save_over_http(url, &client);
    let snapshot: Value = client
        .get(format!("{url}/api/sessions"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(snapshot["sessions"][0]["state"], "to_archive");
    assert_eq!(snapshot["sessions"][0]["note"], "跨列备注");
    assert!(!snapshot["health"]["runtimeVerified"].as_bool().unwrap());
    assert!(fixture.cli("stop").status.success());
    assert_eq!(
        fixture.app().store.sessions().unwrap()[0]["state"],
        "to_archive"
    );
}

#[test]
fn real_http_pwa_manifest_and_assets_are_local_and_uncached() {
    let fixture = Fixture::new(false);
    fixture.populate();
    let running = fixture.launch();
    let origin = running["url"].as_str().unwrap();
    let client = client();
    let html = client.get(origin).send().unwrap().text().unwrap();
    assert!(!html.contains("__CSRF_TOKEN__"));
    check_pwa_page(&fixture, origin);
    let response = client
        .get(format!("{origin}/manifest.webmanifest"))
        .send()
        .unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.headers()["Content-Type"],
        "application/manifest+json; charset=utf-8"
    );
    assert_eq!(response.headers()["Cache-Control"], "no-store");
    let manifest: Value = response.json().unwrap();
    assert_eq!(manifest["id"], "/");
    assert_eq!(manifest["display"], "standalone");
    assert_eq!(manifest["start_url"], "/");
    assert_eq!(manifest["scope"], "/");
    assert_eq!(manifest["name"], "Codex 会话看板");
    check_pwa_icons(&client, &manifest, origin);
    for path in ["/assets/pwa.js", "/sw.js"] {
        let response = client.get(format!("{origin}{path}")).send().unwrap();
        assert!(response.status().is_success());
        assert_eq!(
            response.headers()["Content-Type"],
            "text/javascript; charset=utf-8"
        );
        assert_eq!(response.headers()["Cache-Control"], "no-store");
        assert_eq!(response.headers()["X-Content-Type-Options"], "nosniff");
        assert!(!response.text().unwrap().contains("__CSRF_TOKEN__"));
    }
}

fn check_pwa_page(fixture: &Fixture, origin: &str) {
    let mut browser = Command::new("node")
        .args(["scripts/check-pwa-page.mjs", origin])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut transitions = Vec::new();
    for line in BufReader::new(browser.stdout.take().unwrap()).lines() {
        let transition = line.unwrap();
        match transition.as_str() {
            "stop" => drop(fixture.server.lock().unwrap().take()),
            "start" => assert_eq!(fixture.launch()["url"], origin),
            _ => panic!("未知 PWA fixture 操作：{transition}"),
        }
        transitions.push(transition);
    }
    let output = browser.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(transitions, ["stop", "start"]);
}

fn check_pwa_icons(client: &reqwest::blocking::Client, manifest: &Value, origin: &str) {
    let icons = manifest["icons"].as_array().unwrap();
    assert!(!icons.is_empty());
    for icon in icons {
        let url = url::Url::parse(origin)
            .unwrap()
            .join(icon["src"].as_str().unwrap())
            .unwrap();
        assert_eq!(url.origin(), url::Url::parse(origin).unwrap().origin());
        let response = client.get(url).send().unwrap();
        assert!(response.status().is_success());
        assert_eq!(
            response.headers()["Content-Type"],
            icon["type"].as_str().unwrap()
        );
        assert!(response.text().unwrap().starts_with("<svg"));
    }
}

fn check_plugin_update_page(fixture: &Fixture, html: &str) -> Result<()> {
    let running = fixture.launch();
    let resource = fixture._directory.path().join("update-resource.html");
    fs::write(&resource, html)?;
    let output = Command::new("node").arg("scripts/check-plugin-update-page.mjs")
        .arg(running["url"].as_str().unwrap()).arg(resource).output()?;
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["surfaces"], json!(["http", "mcp"]));
    assert_eq!(report["cleanup"]["profileRemoved"], true);
    Ok(())
}

#[test]
fn real_http_pwa_assets_preserve_host_and_mutation_checks() {
    let fixture = Fixture::new(false);
    let running = fixture.launch();
    let origin = running["url"].as_str().unwrap();
    let client = client();
    for path in ["/", "/manifest.webmanifest", "/assets/pwa.js", "/sw.js"] {
        let endpoint = format!("{origin}{path}");
        let response = client
            .get(&endpoint)
            .header("Host", "attacker.invalid")
            .send()
            .unwrap();
        assert_eq!(response.status(), 403);
        let response = client.post(&endpoint).json(&json!({})).send().unwrap();
        assert_eq!(response.status(), 403);
    }
    let response = client
        .get(format!("{origin}/assets/../config.example.json"))
        .send()
        .unwrap();
    assert!(!response.status().is_success());
}

fn save_over_http(url: &str, client: &reqwest::blocking::Client) {
    let html = client.get(url).send().unwrap().text().unwrap();
    let token = html
        .split("<meta name=\"board-request-token\" content=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let body =
        json!({"id":FIRST,"state":"to_archive","note":"跨列备注","version":0,"inputVersion":0});
    let endpoint = format!("{url}/api/state");
    assert_eq!(
        client.post(&endpoint).json(&body).send().unwrap().status(),
        403
    );
    assert_eq!(
        client
            .post(&endpoint)
            .header("X-CSRF-Token", token)
            .header("Origin", "https://evil")
            .json(&body)
            .send()
            .unwrap()
            .status(),
        403
    );
    check_saved_http(client, &endpoint, token, &body);
}

fn check_saved_http(client: &reqwest::blocking::Client, endpoint: &str, token: &str, body: &Value) {
    assert!(
        client
            .post(endpoint)
            .header("X-CSRF-Token", token)
            .json(body)
            .send()
            .unwrap()
            .status()
            .is_success()
    );
    assert_eq!(
        client
            .post(endpoint)
            .header("X-CSRF-Token", token)
            .json(body)
            .send()
            .unwrap()
            .status(),
        400
    );
}

#[test]
fn changed_config_or_port_cannot_start_second_service() {
    let fixture = Fixture::new(false);
    fixture.launch();
    let mut altered = fixture.settings.clone();
    altered["host_id"] = json!("changed");
    fs::write(&fixture.path, altered.to_string()).unwrap();
    assert!(!fixture.cli("serve").status.success());
    altered = fixture.settings.clone();
    altered["port"] = json!(1);
    fs::write(&fixture.path, altered.to_string()).unwrap();
    assert!(!fixture.cli("serve").status.success());
}

#[test]
fn service_update_rejects_old_build_but_can_verify_and_stop_its_process() {
    let fixture = Fixture::new(false);
    let executable = fixture._directory.path().join("board-bin");
    let replacement = fixture._directory.path().join("board-next");
    install_test_binary(std::path::Path::new(env!("CARGO_BIN_EXE_codex-session-board")), &executable, false);
    let invoke = |action| {
        Command::new(&executable)
            .args(["--config", fixture.path.to_str().unwrap(), action])
            .output()
            .unwrap()
    };
    let mut child =
        ChildGuard::spawn(&executable, &fixture.path, Arc::clone(&fixture._port)).unwrap();
    child.startup_receipt().unwrap();
    install_test_binary(&executable, &replacement, true);
    fs::rename(&replacement, &executable).unwrap();
    let duplicate = invoke("serve");
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("版本"));
    let stopped = invoke("stop");
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    let _ = child.child.wait();
}

fn install_test_binary(source: &std::path::Path, destination: &std::path::Path, variation: bool) {
    let script = r#"import shutil,sys,subprocess
shutil.copy2(sys.argv[1],sys.argv[2])
if sys.argv[3]=='true' and sys.platform!='darwin':
    with open(sys.argv[2],'ab') as output: output.write(b'controlled build variation')
if sys.platform=='darwin':
    identifier='board-test-variation' if sys.argv[3]=='true' else 'board-test-baseline'
    subprocess.run(['/usr/bin/codesign','--force','--sign','-','--identifier',identifier,sys.argv[2]],check=True,stdout=subprocess.DEVNULL)
"#;
    assert!(Command::new("python3")
        .args(["-c", script])
        .arg(source).arg(destination).arg(variation.to_string())
        .status().unwrap().success());
}

#[test]
fn open_accepts_only_known_uuid_and_preserves_all_state() {
    let fixture = Fixture::new(false);
    fixture.populate();
    let app = fixture.app();
    let before = app.store.sessions().unwrap();
    assert!(app.request("/api/open", Some(&json!({"id":FIRST}))).is_ok());
    assert!(
        app.request(
            "/api/open",
            Some(&json!({"id":"33333333-3333-4333-8333-333333333333"}))
        )
        .is_err()
    );
    assert_eq!(app.store.sessions().unwrap(), before);
    let created = app.request("/api/new", Some(&json!({}))).unwrap();
    assert_eq!(created["url"], "codex://threads/new");
    assert_eq!(app.store.sessions().unwrap(), before);
    assert!(app.request("/api/new", None).is_err());
    let mut failing_config = app.config.clone();
    failing_config.open_command = vec!["/usr/bin/false".into()];
    let failing_app = app::App::new(failing_config).unwrap();
    assert!(failing_app.request("/api/new", Some(&json!({}))).is_err());
}

#[test]
fn open_and_new_survive_removed_service_working_directory() {
    let mut fixture = Fixture::new(false);
    fixture.register_projects(&["project-a"]);
    let captured = fixture._directory.path().join("client-cwd");
    fixture.settings["open_command"] = json!(["/bin/sh", "-c",
        "pwd -P >> \"$0\"; printf '%s\\n' \"$1\" >> \"$0\"", captured]);
    fs::write(&fixture.path, fixture.settings.to_string()).unwrap();
    fixture.populate();
    let temporary = tempfile::tempdir().unwrap();
    let running = fixture.launch_in(temporary.path());
    temporary.close().unwrap();
    let url = running["url"].as_str().unwrap();
    let client = client();
    let html = client.get(url).send().unwrap().text().unwrap();
    let token = html.split("<meta name=\"board-request-token\" content=\"").nth(1).unwrap().split('"').next().unwrap();
    for (endpoint, body) in [("open", json!({"id":FIRST})), ("new", json!({})), ("new", json!({"projectId":"project-a"}))] {
        assert!(client.post(format!("{url}/api/{endpoint}"))
            .header("X-CSRF-Token", token).json(&body).send().unwrap().status().is_success());
    }
    let lines = fs::read_to_string(captured).unwrap();
    let expected = config::load(Some(&fixture.path)).unwrap().data_dir.canonicalize().unwrap();
    let lines: Vec<_> = lines.lines().collect();
    assert_eq!(lines.len(), 6);
    for position in [0, 2, 4] {
        assert_eq!(std::path::Path::new(lines[position]), expected);
    }
    assert_eq!(lines[1], app::conversation_url(FIRST, "local").unwrap());
    assert_eq!(lines[3], "codex://threads/new");
    assert_eq!(lines[5], "codex://new?projectId=project-a");
}

#[test]
fn open_reports_exit_status_and_unavailable_stable_directory() {
    let fixture = Fixture::new(false);
    let mut app = fixture.app();
    app.config.open_command = vec!["/bin/sh".into(), "-c".into(), "exit 21".into()];
    let error = app.request("/api/new", Some(&json!({}))).unwrap_err().to_string();
    assert!(error.contains("21") && error.contains("工作目录"));
    fs::remove_dir_all(&app.config.data_dir).unwrap();
    assert!(app.request("/api/new", Some(&json!({}))).unwrap_err().to_string().contains("工作目录不可用"));
}

#[test]
fn new_session_uses_explicit_projects_and_preserves_state() {
    let mut fixture = Fixture::new(false);
    let identifiers = ["project-a", "项目 B &prompt=unexpected?#/"];
    fixture.register_projects(&identifiers);
    let captured = fixture.capture_opens();
    fixture.populate();
    let app = fixture.app();
    let before = app.store.sessions().unwrap();
    assert_eq!(
        app.request("/api/new", Some(&json!({}))).unwrap()["url"],
        "codex://threads/new"
    );
    let mut expected = vec!["codex://threads/new".to_string()];
    for identifier in identifiers {
        let result = app
            .request("/api/new", Some(&json!({"projectId":identifier})))
            .unwrap();
        let url = result["url"].as_str().unwrap();
        let parsed = url::Url::parse(url).unwrap();
        assert_eq!(parsed.host_str(), Some("new"));
        assert_eq!(
            parsed.query_pairs().collect::<Vec<_>>(),
            vec![("projectId".into(), identifier.into())]
        );
        expected.push(url.to_string());
    }
    assert_eq!(
        fs::read_to_string(captured)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(app.store.sessions().unwrap(), before);
}

#[test]
fn new_session_rejects_stale_projects_without_changing_default_navigation() {
    let mut fixture = Fixture::new(false);
    let projects = fixture.register_projects(&["project-a"]);
    let captured = fixture.capture_opens();
    let app = fixture.app();
    app.request("/api/new", Some(&json!({"projectId":"project-a"})))
        .unwrap();
    for invalid in [json!(null), json!(42), json!(""), json!("unknown")] {
        assert!(
            app.request("/api/new", Some(&json!({"projectId":invalid})))
                .is_err()
        );
    }
    for contents in [Some("{\"local-projects\":{}}"), Some("{"), None] {
        fixture.register_projects(&["project-a"]);
        codex_session_board::projects::refresh(&app).unwrap();
        match contents {
            Some(text) => fs::write(&projects, text).unwrap(),
            None => fs::remove_file(&projects).unwrap(),
        }
        assert!(
            app.request("/api/new", Some(&json!({"projectId":"project-a"})))
                .is_err()
        );
        assert_eq!(
            app.request("/api/new", Some(&json!({}))).unwrap()["url"],
            "codex://threads/new"
        );
    }
    assert_eq!(
        fs::read_to_string(captured)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        vec![
            "codex://new?projectId=project-a",
            "codex://threads/new",
            "codex://threads/new",
            "codex://threads/new"
        ]
    );
}
