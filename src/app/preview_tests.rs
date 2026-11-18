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
        if let Some(error) = response.get("error") {
            return Err(crate::rpc::RequestError { method: method.into(), details: error.clone() }.into());
        }
        Ok(response)
    }
}

fn metadata(status: &str) -> Value {
    json!({"thread":{"id":"thread","status":{"type":status}}})
}

#[test]
fn preview_markdown_formats_text_without_loading_untrusted_content() {
    let sample = "# 核对记录\n\n- Linux\n- macOS\n\n> 保留报告\n\n```sh\nprintf 'ok'\n```\n\n\
        | 平台 | 结果 |\n| --- | --- |\n| ARM | 通过 |\n\n[报告](https://example.com/report)\n\n\
        <script>sample</script>\n\n[禁用](file:///tmp/example) ![图示](https://example.com/image.png)";
    let rendered = render_markdown(sample);
    for expected in ["<h1>", "<ul>", "<li>", "<blockquote>", "<pre><code", "<table>",
        "href=\"https://example.com/report\"", "&lt;script&gt;sample&lt;/script&gt;", "禁用</a> 图示"] {
        assert!(rendered.contains(expected), "缺少 {expected}: {rendered}");
    }
    for excluded in ["<script>", "<img", "file://", "image.png"] {
        assert!(!rendered.contains(excluded), "意外出现 {excluded}: {rendered}");
    }
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
    let mut running = page("second", "inProgress", "final_answer");
    running["nextCursor"] = json!("older");
    let mut reader = Fixture(VecDeque::from([
        ("thread/read", metadata("active")),
        ("thread/turns/list", running.clone()),
        ("thread/turns/list", completed),
        ("thread/turns/list", running),
        ("thread/read", metadata("active")),
    ]));
    assert_eq!(read_reply(&mut reader, "thread")?, json!({
        "turnId":"first","inProgress":true,"html":"<p>first final</p>\n<p>second paragraph</p>\n"
    }));
    assert!(reader.0.is_empty());
    Ok(())
}

#[test]
fn reply_reader_distinguishes_empty_from_unknown_and_rechecks_changed_turns() -> Result<()> {
    for response in [
        json!({"data":[]}),
        json!({"error":{"code":-32600,"message":"thread thread is not materialized yet; thread/turns/list is unavailable before first user message"}}),
        page("first", "interrupted", "final_answer"),
        page("first", "failed", "final_answer"),
        page("first", "completed", "commentary"),
    ] {
        let mut reader = Fixture(VecDeque::from([
            ("thread/read", metadata("notLoaded")),
            ("thread/turns/list", response.clone()),
            ("thread/turns/list", response),
            ("thread/read", metadata("notLoaded")),
        ]));
        assert_eq!(read_reply(&mut reader, "thread")?,
            json!({"turnId":null,"html":null,"inProgress":false}));
    }
    let mut unknown = Fixture(VecDeque::from([("thread/read", metadata("systemError"))]));
    assert!(read_reply(&mut unknown, "thread").is_err());
    let mut malformed = Fixture(VecDeque::from([
        ("thread/read", metadata("idle")),
        ("thread/turns/list", json!({"data":null})),
    ]));
    assert!(read_reply(&mut malformed, "thread").is_err());
    let old = page("first", "completed", "final_answer");
    let new = page("second", "completed", "final_answer");
    let mut changed = Fixture(VecDeque::from([
        ("thread/read", metadata("idle")),
        ("thread/turns/list", old),
        ("thread/turns/list", new.clone()),
        ("thread/read", metadata("idle")),
        ("thread/read", metadata("idle")),
        ("thread/turns/list", new.clone()),
        ("thread/turns/list", new),
        ("thread/read", metadata("idle")),
    ]));
    assert_eq!(read_reply(&mut changed, "thread")?["turnId"], "second");
    assert!(changed.0.is_empty());
    Ok(())
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
    let result = app.request("/api/preview-reply", Some(&json!({"id":identifier})))?;
    backend.join().map_err(|_| anyhow::anyhow!("测试后台出错"))??;
    assert_eq!(result["html"], "<p>first final</p>\n<p>second paragraph</p>\n");
    assert!(app.request("/api/preview-reply", Some(&json!({"id":identifier}))).is_err());
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

#[test]
fn snapshot_rechecks_idle_turns_when_the_list_revision_is_unchanged() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(crate::config::Config {
        data_dir: directory.path().into(),
        ..crate::config::Config::default()
    })?;
    let mut revisions = HashMap::new();
    let row = json!({"id":"thread","name":"测试会话","updatedAt":100,"status":{"type":"idle"}});
    app.store.discover(std::slice::from_ref(&row))?;
    let start = std::time::Instant::now();
    let interval = Duration::from_secs(app.config.runtime_poll_seconds);
    for (index, (turn, status, phase, expected)) in [
        ("first", "completed", "final_answer", Some("first")),
        ("second", "completed", "final_answer", Some("second")),
        ("third", "completed", "commentary", None),
        ("fourth", "interrupted", "final_answer", None),
    ].into_iter().enumerate() {
        let mut replies = VecDeque::new();
        if index > 0 {
            replies.push_back(("thread/turns/list", json!({"data":[{"id":turn,"status":status}]})));
        }
        replies.push_back(("thread/turns/list", page(turn, status, phase)));
        let mut reader = Fixture(replies);
        crate::observer::observe_runtime_threads_at(
            &app, &mut reader, &mut revisions, 0.0, std::slice::from_ref(&row),
            start + interval * (index as u32),
        )?;
        assert_eq!(app.snapshot()?["sessions"][0]["previewTurnId"].as_str(), expected);
        assert!(reader.0.is_empty());
    }
    Ok(())
}
