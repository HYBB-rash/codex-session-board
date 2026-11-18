use super::*;
use std::collections::VecDeque;
use std::os::unix::{fs::PermissionsExt, net::UnixListener};

struct Fixture(VecDeque<(&'static str, Value)>);

impl Rpc for Fixture {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let (expected, response) = self.0.pop_front().context("unexpected RPC call")?;
        assert_eq!(method, expected);
        assert_eq!(params["threadId"], "thread");
        if method == "thread/read" {
            assert_eq!(params["includeTurns"], false);
        } else {
            assert_eq!(params["limit"], 1);
            assert_eq!(params["sortDirection"], "desc");
        }
        Ok(response)
    }
}

fn metadata(status: &str) -> Value {
    json!({"thread":{"id":"thread","status":{"type":status}}})
}

fn page(turn: &str, status: &str, phase: &str) -> Value {
    json!({"data":[{"id":turn,"status":status,"items":[
        {"type":"agentMessage","phase":"commentary","text":"progress"},
        {"type":"agentMessage","phase":phase,"text":"first final\n\nsecond paragraph"},
    ]}]})
}

#[test]
fn reply_reader_only_returns_the_expected_completed_turn() -> Result<()> {
    let completed = page("first", "completed", "final_answer");
    let mut reader = Fixture(VecDeque::from([
        ("thread/read", metadata("notLoaded")),
        ("thread/turns/list", completed.clone()),
        ("thread/turns/list", completed),
        ("thread/read", metadata("idle")),
    ]));
    assert_eq!(read_reply(&mut reader, "thread", "first")?, "first final\n\nsecond paragraph");
    assert!(reader.0.is_empty());
    Ok(())
}

#[test]
fn reply_reader_rejects_active_interrupted_unmarked_and_changed_turns() {
    for (before, response, after, last_status) in [
        ("active", page("first", "completed", "final_answer"),
            page("first", "completed", "final_answer"), "active"),
        ("idle", page("first", "interrupted", "final_answer"), Value::Null, "idle"),
        ("idle", page("first", "completed", "commentary"), Value::Null, "idle"),
        ("idle", page("first", "completed", "final_answer"),
            page("second", "completed", "final_answer"), "idle"),
        ("idle", page("first", "completed", "final_answer"),
            page("first", "completed", "final_answer"), "active"),
    ] {
        let mut reader = Fixture(VecDeque::from([
            ("thread/read", metadata(before)),
            ("thread/turns/list", response),
            ("thread/turns/list", after),
            ("thread/read", metadata(last_status)),
        ]));
        assert!(read_reply(&mut reader, "thread", "first").is_err());
    }
}

#[test]
fn reply_endpoint_reads_only_on_click_and_retires_on_transport_failure() -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let path = directory.path().join("backend.sock");
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let app = App::new(crate::config::Config {
        desktop_socket: path,
        local_authority: Some(crate::rpc::authority::LocalAuthority::owned_listener(&listener)?),
        data_dir: directory.path().join("board"),
        ..crate::config::Config::default()
    })?;
    let identifier = uuid::Uuid::new_v4().to_string();
    let row = json!({"id":identifier,"name":"专用会话","updatedAt":1,"status":{"type":"idle"}});
    app.store.discover(std::slice::from_ref(&row))?;
    app.preview_turns.observe(&identifier,&row["status"],&json!({
        "id":"first","status":"completed","hasFinal":true
    }))?;
    app.update_health(json!({"runtimeVerified":true}))?;
    let snapshot = app.snapshot()?;
    assert_eq!(snapshot["sessions"][0]["previewTurnId"], "first");
    assert!(!snapshot.to_string().contains("first final"));
    let thread_id = identifier.clone();
    let backend = std::thread::spawn(move || serve_preview(listener, &thread_id));
    let result = app.request("/api/preview-reply", Some(&json!({"id":identifier,"turnId":"first"})))?;
    backend.join().map_err(|_| anyhow::anyhow!("测试后台出错"))??;
    assert_eq!(result["text"], "first final\n\nsecond paragraph");
    assert!(app.request("/api/preview-reply", Some(&json!({"id":identifier,"turnId":"first"}))).is_err());
    app.update_health(json!({"runtimeVerified":false}))?;
    assert!(app.snapshot()?["sessions"][0]["previewTurnId"].is_null());
    Ok(())
}

fn serve_preview(listener: UnixListener, thread: &str) -> Result<()> {
    let (stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut wire = tungstenite::accept(stream)?;
    for (method, view, answer) in [
        ("initialize", "", json!({})),
        ("initialized", "", Value::Null),
        ("thread/read", "", json!({"thread":{"id":thread,"status":{"type":"idle"}}})),
        ("thread/turns/list", "summary", page("first", "completed", "final_answer")),
        ("thread/turns/list", "notLoaded", json!({"data":[{"id":"first","status":"completed"}]})),
        ("thread/read", "", json!({"thread":{"id":thread,"status":{"type":"idle"}}})),
    ] {
        let request: Value = serde_json::from_str(wire.read()?.to_text()?)?;
        assert_eq!(request["method"], method);
        if !view.is_empty() {
            assert_eq!(request["params"]["itemsView"], view);
        }
        if let Some(id) = request.get("id") {
            wire.send(tungstenite::Message::Text(json!({"id":id,"result":answer}).to_string().into()))?;
        }
    }
    Ok(())
}

#[test]
fn snapshot_tracks_observed_turns_across_same_second_transitions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(crate::config::Config {
        data_dir: directory.path().into(),
        ..crate::config::Config::default()
    })?;
    let mut revisions = HashMap::new();
    let mut row = json!({"id":"thread","name":"测试会话","updatedAt":1,"status":{"type":"idle"}});
    app.store.discover(std::slice::from_ref(&row))?;
    for (thread_status, turn, turn_status, phase, expected) in [
        ("idle", "first", "completed", "final_answer", Some("first")),
        ("active", "first", "completed", "final_answer", None),
        ("active", "second", "inProgress", "commentary", None),
        ("idle", "second", "completed", "final_answer", Some("second")),
        ("active", "third", "inProgress", "final_answer", None),
        ("idle", "third", "interrupted", "final_answer", None),
    ] {
        row["status"]["type"] = json!(thread_status);
        let mut reader = Fixture(VecDeque::from([("thread/turns/list", page(turn, turn_status, phase))]));
        crate::observer::observe_runtime_threads(&app, &mut reader, &mut revisions, 0.0, std::slice::from_ref(&row))?;
        let snapshot = app.snapshot()?;
        assert_eq!(snapshot["sessions"][0]["previewTurnId"].as_str(), expected);
        assert!(!snapshot.to_string().contains("first final"));
    }
    Ok(())
}
