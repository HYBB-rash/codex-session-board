#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::{Result, bail};
use codex_session_board::{app, config, http, mcp, observer, rpc::Rpc, titles};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
const SECOND: &str = "22222222-2222-4222-8222-222222222222";

struct Fixture {
    _directory: TempDir,
    path: PathBuf,
    settings: Value,
}

impl Fixture {
    fn new(sync: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let settings = json!({"data_dir":"data","port":port,"sync_titles":sync,
            "desktop_socket":"","codex_command":["/bin/false"],"open_command":["/bin/true"],
            "poll_seconds":1,"rpc_timeout_seconds":1,"runtime_poll_seconds":1});
        fs::write(&path, settings.to_string()).unwrap();
        Self {
            _directory: directory,
            path,
            settings,
        }
    }

    fn app(&self) -> app::App {
        app::App::new(config::load(Some(&self.path)).unwrap()).unwrap()
    }

    fn cli(&self, action: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
            .args(["--config", self.path.to_str().unwrap(), action])
            .output()
            .unwrap()
    }

    fn launch(&self) -> Value {
        let output = self.cli("launch");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
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
        let _ = self.cli("stop");
    }
}

struct Native {
    name: String,
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
    fixture.populate();
    let app = fixture.app();
    let body = format!("🧪 [项目] {}", "完整正文".repeat(350));
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
    assert_eq!(native.writes.len(), 1);
    native.lose_ack = false;
    titles::sync(&app.store, &mut native, FIRST).unwrap();
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
fn old_ack_cannot_delete_newer_title_job() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let app = fixture.app();
    app.store
        .set_state(FIRST, Some("review"), "", 0, None)
        .unwrap();
    app.store
        .set_state(FIRST, Some("completed"), "新状态", 1, None)
        .unwrap();
    assert!(!app.store.title_synced(FIRST, 1, "[待审阅] 原名").unwrap());
    assert_eq!(app.store.pending_titles(None).unwrap()[0]["version"], 2);
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
    let fixture = Fixture::new(true);
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
        Some(&json!({"id":FIRST,"state":"completed","version":0,"inputVersion":0})),
    )
    .unwrap();
    assert!(app.store.pending_titles(None).unwrap().is_empty());
    assert_eq!(app.store.sessions().unwrap()[0]["state"], "completed");
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

#[test]
fn native_resource_and_tools_expose_global_app_only_actions() {
    let definitions = mcp::tools();
    let view = &definitions["tools"][0];
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
    for tool in definitions["tools"].as_array().unwrap().iter().skip(1) {
        assert_eq!(tool["_meta"]["ui"]["visibility"], json!(["app"]));
    }
    let resource = mcp::resource();
    let html = resource["text"].as_str().unwrap();
    assert_eq!(resource["mimeType"], mcp::MIME);
    assert!(html.contains("window.boardRequest"));
    assert!(html.contains("\"/api/new\":\"board_new\""));
    assert!(html.contains("id=\"new-session\""));
    let create = definitions["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "board_new")
        .unwrap();
    assert_eq!(create["_meta"]["ui"]["visibility"], json!(["app"]));
    assert_eq!(create["annotations"]["readOnlyHint"], false);
    assert!(!html.contains("<iframe") && !html.contains("https://cdn"));
    assert!(!html.contains("__CSRF_TOKEN__"));
}

#[test]
fn real_stdio_mcp_handshake_and_resource_round_trip() {
    let fixture = Fixture::new(false);
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .args(["mcp", "--config", fixture.path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({"id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"method":"notifications/initialized"}),
        json!({"id":2,"method":"tools/list"}),
        json!({"id":3,"method":"resources/read","params":{"uri":mcp::RESOURCE}}),
    ];
    let mut input = child.stdin.take().unwrap();
    for request in requests {
        writeln!(input, "{request}").unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0]["result"]["serverInfo"]["name"],
        "codex-session-board"
    );
    assert_eq!(lines[2]["result"]["contents"][0]["mimeType"], mcp::MIME);
}

#[test]
fn mcp_bridge_uses_http_versions_and_reports_pending_title_without_rollback() {
    let fixture = Fixture::new(true);
    fixture.populate();
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .args(["mcp", "--config", fixture.path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let body = json!({"id":FIRST,"state":"review","note":"桥接备注","version":0,"inputVersion":0});
    let mut input = child.stdin.take().unwrap();
    for (identifier, name, arguments) in [
        (1, "board_update", body.clone()),
        (2, "board_update", body),
        (3, "board_snapshot", json!({})),
        (4, "board_open", json!({"id":"invalid"})),
    ] {
        writeln!(
            input,
            "{}",
            json!({"id":identifier,"method":"tools/call",
            "params":{"name":name,"arguments":arguments}})
        )
        .unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        replies[0]["result"]["structuredContent"]["titlePending"],
        true
    );
    assert_eq!(replies[1]["result"]["isError"], true);
    let row = &replies[2]["result"]["structuredContent"]["sessions"][0];
    assert_eq!(row["state"], "review");
    assert_eq!(row["note"], "桥接备注");
    assert_eq!(replies[3]["result"]["isError"], true);
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

fn save_over_http(url: &str, client: &reqwest::blocking::Client) {
    let html = client.get(url).send().unwrap().text().unwrap();
    let token = html
        .split("const csrf = \"")
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
    assert!(!fixture.cli("launch").status.success());
    altered = fixture.settings.clone();
    altered["port"] = json!(1);
    fs::write(&fixture.path, altered.to_string()).unwrap();
    assert!(!fixture.cli("launch").status.success());
}

#[test]
fn service_update_rejects_old_build_but_can_verify_and_stop_its_process() {
    let fixture = Fixture::new(false);
    let executable = fixture._directory.path().join("board-bin");
    let replacement = fixture._directory.path().join("board-next");
    fs::copy(env!("CARGO_BIN_EXE_codex-session-board"), &executable).unwrap();
    let invoke = |action| {
        Command::new(&executable)
            .args(["--config", fixture.path.to_str().unwrap(), action])
            .output()
            .unwrap()
    };
    let launched = invoke("launch");
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    fs::copy(&executable, &replacement).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&replacement)
        .unwrap()
        .write_all(b"controlled build variation")
        .unwrap();
    fs::rename(&replacement, &executable).unwrap();
    let duplicate = invoke("launch");
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("版本"));
    let stopped = invoke("stop");
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
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
    failing_config.open_command = vec!["/bin/false".into()];
    let failing_app = app::App::new(failing_config).unwrap();
    assert!(failing_app.request("/api/new", Some(&json!({}))).is_err());
}
