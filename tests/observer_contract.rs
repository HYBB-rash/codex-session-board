#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use codex_session_board::{
    app::App,
    config::Config,
    observer::{scan_history, scan_runtime},
    rpc::{RequestError, Rpc},
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::{Error, ErrorKind};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    app: App,
}

impl Fixture {
    fn new(quota: usize, max_pages: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: directory.path().join("data"),
            desktop_socket: directory.path().join("unused-desktop.sock"),
            codex_command: vec!["unused-fixture-command".into()],
            open_command: vec!["unused-fixture-client".into()],
            sync_titles: false,
            page_size: 2,
            history_per_poll: quota,
            max_pages,
            ..Config::default()
        };
        Self {
            app: App::new(config).unwrap(),
            _directory: directory,
        }
    }
}

#[derive(Clone)]
enum Reply {
    Data(Value),
    Request(&'static str),
    Timeout(&'static str),
}

impl Reply {
    fn result(self, method: &str) -> Result<Value> {
        match self {
            Self::Data(value) => Ok(value),
            Self::Request(message) => Err(RequestError {
                method: method.to_string(),
                details: json!({"code": -32000, "message": message}),
            }
            .into()),
            Self::Timeout(message) => Err(Error::new(ErrorKind::TimedOut, message).into()),
        }
    }
}

struct FakeRpc {
    pages: VecDeque<Reply>,
    turns: HashMap<String, Reply>,
    calls: Vec<(String, Value)>,
}

impl FakeRpc {
    fn new(threads: &[Value]) -> Self {
        Self {
            pages: VecDeque::from([Reply::Data(json!({"data": threads}))]),
            turns: threads
                .iter()
                .map(|entry| {
                    let id = entry["id"].as_str().unwrap();
                    (
                        id.to_string(),
                        Reply::Data(json!({"data": [raw_turn(id, "completed")]})),
                    )
                })
                .collect(),
            calls: Vec::new(),
        }
    }

    fn set_turn(&mut self, identifier: &str, turn: Value) {
        self.turns
            .insert(identifier.to_string(), Reply::Data(json!({"data": [turn]})));
    }

    fn fail_turn(&mut self, identifier: &str, reply: Reply) {
        self.turns.insert(identifier.to_string(), reply);
    }

    fn turn_ids(&self) -> Vec<String> {
        self.calls
            .iter()
            .filter(|(method, _)| method == "thread/turns/list")
            .map(|(_, params)| params["threadId"].as_str().unwrap().to_string())
            .collect()
    }

    fn list_calls(&self) -> Vec<&Value> {
        self.calls
            .iter()
            .filter(|(method, _)| method == "thread/list")
            .map(|(_, params)| params)
            .collect()
    }
}

impl Rpc for FakeRpc {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.calls.push((method.to_string(), params.clone()));
        let reply = match method {
            "thread/list" => self
                .pages
                .pop_front()
                .expect("unexpected extra page request"),
            "thread/turns/list" => self
                .turns
                .get(params["threadId"].as_str().unwrap())
                .expect("unexpected conversation read")
                .clone(),
            _ => panic!("observer attempted an unplanned RPC method: {method}"),
        };
        reply.result(method)
    }
}

fn thread(identifier: &str, revision: i64, status: &str) -> Value {
    json!({
        "id": identifier, "name": format!("会话 {identifier}"),
        "cwd": "/fixture/project", "updatedAt": revision,
        "status": {"type": status},
    })
}

fn raw_turn(identifier: &str, phase: &str) -> Value {
    json!({
        "id": format!("{identifier}-round"), "status": phase,
        "startedAt": 1001.0, "completedAt": 1002.0,
        "items": [
            {"id": "user-one", "type": "userMessage", "text": "private input"},
            {"id": "answer-one", "type": "agentMessage", "phase": "final_answer",
             "text": "private answer"},
        ],
    })
}

fn session(app: &App, identifier: &str) -> Value {
    app.store
        .sessions()
        .unwrap()
        .into_iter()
        .find(|row| row["id"] == identifier)
        .unwrap()
}

fn health(app: &App) -> Value {
    app.snapshot().unwrap()["health"].clone()
}

fn decision(app: &App, identifier: &str) -> Value {
    let row = session(app, identifier);
    json!({
        "state": row["state"], "note": row["note"], "manual": row["manual"],
        "version": row["version"], "inputVersion": row["inputVersion"],
        "followUp": row["followUp"],
    })
}

fn manual(app: &App, identifier: &str) {
    app.store
        .discover(&[thread(identifier, 1, "notLoaded")])
        .unwrap();
    app.store
        .set_state(identifier, Some("completed"), "保留人工验收", 0, Some(0))
        .unwrap();
}

fn stored_summary(app: &App, identifier: &str) -> Value {
    let row = app
        .store
        .pending()
        .unwrap()
        .into_iter()
        .find(|row| row["id"] == identifier)
        .unwrap();
    serde_json::from_str(row["turn_json"].as_str().unwrap()).unwrap()
}

fn assert_read_contract(reader: &FakeRpc, page_size: usize) {
    for (method, params) in &reader.calls {
        match method.as_str() {
            "thread/list" => assert_eq!(
                *params,
                json!({
                    "limit": page_size, "cursor": params["cursor"],
                    "sortKey": "updated_at", "sortDirection": "desc",
                    "modelProviders": [], "sourceKinds": ["cli", "vscode", "appServer"],
                    "archived": false, "useStateDbOnly": true,
                })
            ),
            "thread/turns/list" => assert_eq!(
                *params,
                json!({
                    "threadId": params["threadId"], "limit": 1,
                    "sortDirection": "desc", "itemsView": "full",
                })
            ),
            _ => panic!("unexpected observer method: {method}"),
        }
    }
}

fn assert_history_error(app: &App, message: &str) {
    let status = health(app);
    assert_eq!(status["refreshing"], false);
    assert_eq!(status["scanComplete"], false);
    assert!(status["error"].as_str().unwrap().contains(message));
    assert!(status["lastScanAt"].is_number());
}

fn changed_and_backfill(round: i64) -> Vec<Value> {
    let mut threads = vec![
        thread("changed-a", 100 + round, "notLoaded"),
        thread("changed-b", 200 + round, "notLoaded"),
    ];
    threads.extend((1..=4).map(|index| thread(&format!("backfill-{index}"), index, "notLoaded")));
    threads
}

fn edit_body(row: &Value, state: &str) -> Value {
    json!({
        "id": row["id"], "state": state, "version": row["version"],
        "inputVersion": row["inputVersion"],
    })
}

fn runtime_manual_fixture() -> (Fixture, HashMap<String, String>, Value) {
    let fixture = Fixture::new(2, 2);
    let mut reader = FakeRpc::new(&[thread("owner", 1, "active")]);
    reader.set_turn("owner", raw_turn("owner", "inProgress"));
    let mut revisions = HashMap::new();
    scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap();
    let mut body = edit_body(&session(&fixture.app, "owner"), "completed");
    body["note"] = json!("保留人工验收备注");
    fixture.app.request("/api/state", Some(&body)).unwrap();
    let saved = session(&fixture.app, "owner");
    assert_eq!(saved["inputVersion"], 1);
    (fixture, revisions, saved)
}

fn runtime_extra_input(app: &App, revisions: &mut HashMap<String, String>) {
    let mut reader = FakeRpc::new(&[thread("owner", 1, "active")]);
    let mut latest = raw_turn("owner", "inProgress");
    latest["items"].as_array_mut().unwrap().push(json!({
        "id": "user-two", "type": "userMessage", "text": "private new input",
    }));
    reader.set_turn("owner", latest);
    scan_runtime(app, &mut reader, revisions, 1000.0).unwrap();
    assert_eq!(reader.turn_ids(), ["owner"]);
    assert_read_contract(&reader, 2);
}

#[test]
fn history_scan_paginates_with_declared_scope_and_deduplicates_before_reads() {
    let fixture = Fixture::new(4, 3);
    let threads = vec![
        thread("first", 30, "notLoaded"),
        thread("second", 20, "notLoaded"),
        thread("third", 10, "notLoaded"),
    ];
    let mut reader = FakeRpc::new(&threads);
    let mut duplicate = threads[0].clone();
    duplicate["name"] = json!("不得覆盖首次出现的名称");
    reader.pages = VecDeque::from([
        Reply::Data(json!({"data": &threads[..2], "nextCursor": "next-page"})),
        Reply::Data(json!({"data": [duplicate, threads[2]]})),
    ]);
    scan_history(&fixture.app, &mut reader).unwrap();
    assert_eq!(reader.turn_ids(), ["first", "second", "third"]);
    assert_eq!(reader.list_calls()[0]["cursor"], Value::Null);
    assert_eq!(reader.list_calls()[1]["cursor"], "next-page");
    assert_eq!(session(&fixture.app, "first")["title"], "会话 first");
    let status = health(&fixture.app);
    assert_eq!(status["scanComplete"], true);
    assert_eq!(status["discovered"], 3);
    assert_eq!(status["refreshing"], false);
    assert!(status["error"].is_null());
    assert!(status["lastSuccessAt"].is_number());
    assert_read_contract(&reader, 2);
}

#[test]
fn history_scan_stores_real_rpc_summary_without_message_bodies() {
    let fixture = Fixture::new(2, 2);
    let mut reader = FakeRpc::new(&[thread("sample", 1, "notLoaded")]);
    scan_history(&fixture.app, &mut reader).unwrap();
    let row = session(&fixture.app, "sample");
    assert_eq!(row["state"], "review");
    assert_eq!(row["source"], "history");
    assert_eq!(row["turnStatus"], "completed");
    let summary = stored_summary(&fixture.app, "sample");
    assert_eq!(summary["id"], "sample-round");
    assert_eq!(summary["userItems"], json!(["user-one"]));
    assert_eq!(summary["hasFinal"], true);
    assert!(!summary.to_string().contains("private"));
    assert_read_contract(&reader, 2);
}

#[test]
fn incomplete_history_scan_reports_partial_discovery_and_retains_missing_manual_row() {
    let fixture = Fixture::new(2, 1);
    manual(&fixture.app, "saved");
    let saved = decision(&fixture.app, "saved");
    let mut reader = FakeRpc::new(&[thread("new", 10, "notLoaded")]);
    reader.pages[0] = Reply::Data(json!({
        "data": [thread("new", 10, "notLoaded")], "nextCursor": "not-read",
    }));
    scan_history(&fixture.app, &mut reader).unwrap();
    let status = health(&fixture.app);
    assert_eq!(status["scanComplete"], false);
    assert_eq!(status["discovered"], 1);
    assert_eq!(status["refreshing"], false);
    assert!(status["error"].is_null());
    assert!(status["lastSuccessAt"].is_number());
    assert_eq!(decision(&fixture.app, "saved"), saved);
    assert_eq!(fixture.app.store.sessions().unwrap().len(), 2);
    assert_eq!(reader.list_calls().len(), 1);
    assert_eq!(reader.turn_ids(), ["new"]);
    assert_read_contract(&reader, 2);
}

#[test]
fn empty_history_discovery_keeps_cached_manual_session() {
    let fixture = Fixture::new(2, 2);
    manual(&fixture.app, "saved");
    let saved = decision(&fixture.app, "saved");
    let mut reader = FakeRpc::new(&[]);
    scan_history(&fixture.app, &mut reader).unwrap();
    assert_eq!(decision(&fixture.app, "saved"), saved);
    assert_eq!(fixture.app.store.sessions().unwrap().len(), 1);
    assert!(reader.turn_ids().is_empty());
    assert_eq!(health(&fixture.app)["scanComplete"], true);
    assert_eq!(health(&fixture.app)["discovered"], 0);
}

#[test]
fn history_request_error_marks_only_that_session_and_continues_next_read() {
    let fixture = Fixture::new(2, 2);
    manual(&fixture.app, "unreadable");
    let before = decision(&fixture.app, "unreadable");
    let mut reader = FakeRpc::new(&[
        thread("unreadable", 30, "notLoaded"),
        thread("next", 20, "notLoaded"),
    ]);
    reader.fail_turn("unreadable", Reply::Request("one history unreadable"));
    scan_history(&fixture.app, &mut reader).unwrap();
    assert_eq!(reader.turn_ids(), ["unreadable", "next"]);
    assert_eq!(decision(&fixture.app, "unreadable"), before);
    assert!(
        session(&fixture.app, "unreadable")["observationError"]
            .as_str()
            .unwrap()
            .contains("one history unreadable")
    );
    assert_eq!(session(&fixture.app, "next")["state"], "review");
    assert!(session(&fixture.app, "next")["observationError"].is_null());
    assert_eq!(health(&fixture.app)["scanComplete"], true);
    assert!(health(&fixture.app)["error"].is_null());
    assert_read_contract(&reader, 2);
}

#[test]
fn history_transport_failure_preserves_manual_state_and_reports_failed_scan() {
    let fixture = Fixture::new(2, 2);
    manual(&fixture.app, "unreadable");
    let before = decision(&fixture.app, "unreadable");
    let mut reader = FakeRpc::new(&[
        thread("unreadable", 30, "notLoaded"),
        thread("next", 20, "notLoaded"),
    ]);
    reader.fail_turn("unreadable", Reply::Timeout("history transport timeout"));
    let error = scan_history(&fixture.app, &mut reader).unwrap_err();
    assert!(error.downcast_ref::<RequestError>().is_none());
    assert_eq!(reader.turn_ids(), ["unreadable"]);
    assert_eq!(decision(&fixture.app, "unreadable"), before);
    assert_eq!(session(&fixture.app, "next")["state"], "unknown");
    assert_history_error(&fixture.app, "history transport timeout");
    assert!(health(&fixture.app)["lastSuccessAt"].is_null());
    assert_read_contract(&reader, 2);
}

#[test]
fn history_list_failure_does_not_remove_or_reclassify_saved_sessions() {
    let fixture = Fixture::new(2, 2);
    manual(&fixture.app, "saved");
    let before = decision(&fixture.app, "saved");
    let mut reader = FakeRpc::new(&[]);
    reader.pages[0] = Reply::Timeout("list connection lost");
    assert!(scan_history(&fixture.app, &mut reader).is_err());
    assert_eq!(decision(&fixture.app, "saved"), before);
    assert_eq!(fixture.app.store.sessions().unwrap().len(), 1);
    assert!(reader.turn_ids().is_empty());
    assert_history_error(&fixture.app, "list connection lost");
    assert_read_contract(&reader, 2);
}

#[test]
fn later_history_success_clears_transport_and_per_session_errors() {
    let fixture = Fixture::new(2, 2);
    let threads = [thread("sample", 1, "notLoaded")];
    let mut failed = FakeRpc::new(&threads);
    failed.fail_turn("sample", Reply::Timeout("temporary history timeout"));
    assert!(scan_history(&fixture.app, &mut failed).is_err());
    assert_history_error(&fixture.app, "temporary history timeout");
    let mut recovered = FakeRpc::new(&threads);
    scan_history(&fixture.app, &mut recovered).unwrap();
    let status = health(&fixture.app);
    assert_eq!(status["scanComplete"], true);
    assert_eq!(status["refreshing"], false);
    assert!(status["error"].is_null());
    assert!(status["lastSuccessAt"].is_number());
    assert!(session(&fixture.app, "sample")["observationError"].is_null());
    assert_eq!(session(&fixture.app, "sample")["state"], "review");
}

#[test]
fn failed_history_scan_preserves_last_success_timestamp() {
    let fixture = Fixture::new(2, 2);
    let mut success = FakeRpc::new(&[thread("sample", 1, "notLoaded")]);
    scan_history(&fixture.app, &mut success).unwrap();
    let previous_success = health(&fixture.app)["lastSuccessAt"].clone();
    let mut failure = FakeRpc::new(&[]);
    failure.pages[0] = Reply::Timeout("later list timeout");
    assert!(scan_history(&fixture.app, &mut failure).is_err());
    assert_history_error(&fixture.app, "later list timeout");
    assert_eq!(health(&fixture.app)["lastSuccessAt"], previous_success);
    assert_eq!(session(&fixture.app, "sample")["state"], "review");
}

#[test]
fn changed_quota_keeps_backfill_budget_under_repeated_request_timeouts() {
    let fixture = Fixture::new(4, 2);
    let initial = [
        thread("changed-a", 1, "notLoaded"),
        thread("changed-b", 2, "notLoaded"),
    ];
    scan_history(&fixture.app, &mut FakeRpc::new(&initial)).unwrap();
    let mut seen_backfill = BTreeSet::new();
    for round in 1..=3 {
        let threads = changed_and_backfill(round);
        let mut reader = FakeRpc::new(&threads);
        for index in 1..=4 {
            reader.fail_turn(
                &format!("backfill-{index}"),
                Reply::Request("retry timeout"),
            );
        }
        scan_history(&fixture.app, &mut reader).unwrap();
        let ids = reader.turn_ids();
        assert_eq!(ids.len(), 4);
        assert_eq!(ids[..2], ["changed-b", "changed-a"]);
        assert!(ids[2..].iter().all(|id| id.starts_with("backfill-")));
        seen_backfill.extend(ids[2..].iter().cloned());
        assert_eq!(health(&fixture.app)["scanComplete"], true);
        assert_read_contract(&reader, 2);
    }
    assert_eq!(seen_backfill.len(), 4);
    for identifier in seen_backfill {
        assert!(
            session(&fixture.app, &identifier)["observationError"]
                .as_str()
                .unwrap()
                .contains("retry timeout")
        );
    }
}

#[test]
fn repeated_history_transport_timeouts_rotate_attempts_across_all_sessions() {
    let fixture = Fixture::new(2, 2);
    let threads: Vec<_> = (1..=5)
        .map(|index| thread(&format!("timeout-{index}"), index, "notLoaded"))
        .collect();
    let mut attempted = Vec::new();
    for _round in 0..5 {
        let mut reader = FakeRpc::new(&threads);
        for entry in &threads {
            reader.fail_turn(
                entry["id"].as_str().unwrap(),
                Reply::Timeout("repeated timeout"),
            );
        }
        assert!(scan_history(&fixture.app, &mut reader).is_err());
        assert_eq!(reader.turn_ids().len(), 1);
        attempted.extend(reader.turn_ids());
        assert_history_error(&fixture.app, "repeated timeout");
    }
    assert_eq!(attempted.iter().collect::<BTreeSet<_>>().len(), 5);
    assert_eq!(fixture.app.store.sessions().unwrap().len(), 5);
}

#[test]
fn repeated_broken_history_does_not_starve_a_successful_session() {
    let fixture = Fixture::new(2, 2);
    let initial = [thread("broken", 1, "notLoaded")];
    scan_history(&fixture.app, &mut FakeRpc::new(&initial)).unwrap();
    let threads = [
        thread("broken", 30, "notLoaded"),
        thread("good", 10, "notLoaded"),
    ];
    let mut seen = Vec::new();
    for _round in 0..2 {
        let mut reader = FakeRpc::new(&threads);
        reader.fail_turn("broken", Reply::Timeout("persistent timeout"));
        assert!(scan_history(&fixture.app, &mut reader).is_err());
        seen.extend(reader.turn_ids());
    }
    assert_eq!(seen[..2], ["broken", "good"]);
    assert_eq!(session(&fixture.app, "good")["state"], "review");
}

#[test]
fn runtime_first_owner_active_scan_reads_a_real_turn_summary() {
    let fixture = Fixture::new(2, 2);
    let mut reader = FakeRpc::new(&[thread("owner", 1, "active")]);
    reader.set_turn("owner", raw_turn("owner", "inProgress"));
    let mut revisions = HashMap::new();
    scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap();
    assert_eq!(reader.turn_ids(), ["owner"]);
    assert_eq!(session(&fixture.app, "owner")["state"], "in_progress");
    assert_eq!(session(&fixture.app, "owner")["source"], "runtime");
    assert_eq!(session(&fixture.app, "owner")["manual"], false);
    assert!(revisions.contains_key("owner"));
    let status = health(&fixture.app);
    assert_eq!(status["runtimeVerified"], true);
    assert!(status["runtimeError"].is_null());
    assert!(status["runtimeLastAt"].is_number());
    assert_read_contract(&reader, 2);
}

#[test]
fn active_thread_metadata_with_terminal_summary_does_not_invent_execution() {
    let fixture = Fixture::new(2, 2);
    let mut reader = FakeRpc::new(&[thread("owner", 1, "active")]);
    let mut revisions = HashMap::new();
    scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap();
    assert_eq!(reader.turn_ids(), ["owner"]);
    assert_eq!(session(&fixture.app, "owner")["state"], "review");
    assert_eq!(session(&fixture.app, "owner")["source"], "history");
    assert!(!revisions.contains_key("owner"));
    assert_eq!(health(&fixture.app)["runtimeVerified"], true);
    assert_read_contract(&reader, 2);
}

#[test]
fn runtime_active_threads_are_read_each_poll_while_unchanged_idle_is_cached() {
    let fixture = Fixture::new(2, 2);
    let mut revisions = HashMap::new();
    for _round in 0..2 {
        let mut reader = FakeRpc::new(&[thread("owner", 1, "active")]);
        reader.set_turn("owner", raw_turn("owner", "inProgress"));
        scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap();
        assert_eq!(reader.turn_ids(), ["owner"]);
    }
    let threads = [thread("owner", 2, "idle")];
    let mut terminal = FakeRpc::new(&threads);
    scan_runtime(&fixture.app, &mut terminal, &mut revisions, 1000.0).unwrap();
    assert_eq!(terminal.turn_ids(), ["owner"]);
    let mut repeated = FakeRpc::new(&threads);
    scan_runtime(&fixture.app, &mut repeated, &mut revisions, 1000.0).unwrap();
    assert!(repeated.turn_ids().is_empty());
    assert_eq!(session(&fixture.app, "owner")["state"], "review");
}

#[test]
fn runtime_owner_revision_change_recovers_a_short_unseen_completed_turn() {
    let fixture = Fixture::new(2, 2);
    let mut revisions = HashMap::new();
    let mut first = FakeRpc::new(&[thread("owner", 1, "active")]);
    first.set_turn("owner", raw_turn("first", "inProgress"));
    scan_runtime(&fixture.app, &mut first, &mut revisions, 1000.0).unwrap();
    assert_eq!(session(&fixture.app, "owner")["state"], "in_progress");
    let mut second = FakeRpc::new(&[thread("owner", 2, "idle")]);
    let mut short_turn = raw_turn("unseen", "completed");
    short_turn["items"] = json!([]);
    second.set_turn("owner", short_turn);
    scan_runtime(&fixture.app, &mut second, &mut revisions, 1000.0).unwrap();
    assert_eq!(second.turn_ids(), ["owner"]);
    assert_eq!(session(&fixture.app, "owner")["state"], "review");
    assert_eq!(session(&fixture.app, "owner")["source"], "runtime");
    assert_eq!(health(&fixture.app)["runtimeVerified"], true);
    assert_read_contract(&second, 2);
}

#[test]
fn runtime_first_poll_catches_execution_that_failed_between_polls() {
    let fixture = Fixture::new(2, 2);
    let mut reader = FakeRpc::new(&[thread("owner", 2, "idle")]);
    reader.set_turn("owner", raw_turn("unseen", "failed"));
    scan_runtime(&fixture.app, &mut reader, &mut HashMap::new(), 1000.0).unwrap();
    assert_eq!(reader.turn_ids(), ["owner"]);
    assert_eq!(session(&fixture.app, "owner")["state"], "blocked");
    assert_eq!(session(&fixture.app, "owner")["source"], "runtime");
    assert_eq!(health(&fixture.app)["runtimeVerified"], true);
}

#[test]
fn runtime_request_failures_are_scoped_counted_and_do_not_stop_other_threads() {
    let fixture = Fixture::new(4, 2);
    manual(&fixture.app, "bad-one");
    let before = decision(&fixture.app, "bad-one");
    let threads = [
        thread("bad-one", 3, "active"),
        thread("bad-two", 2, "active"),
        thread("good", 1, "active"),
    ];
    let mut reader = FakeRpc::new(&threads);
    reader.fail_turn("bad-one", Reply::Request("first unreadable"));
    reader.fail_turn("bad-two", Reply::Request("second unreadable"));
    reader.set_turn("good", raw_turn("good", "inProgress"));
    let mut revisions = HashMap::new();
    scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap();
    assert_eq!(reader.turn_ids(), ["bad-one", "bad-two", "good"]);
    assert_eq!(decision(&fixture.app, "bad-one"), before);
    assert_eq!(session(&fixture.app, "good")["state"], "in_progress");
    assert_eq!(revisions.len(), 1);
    assert!(revisions.contains_key("good"));
    assert_eq!(health(&fixture.app)["runtimeVerified"], true);
    assert_eq!(
        health(&fixture.app)["runtimeError"],
        "2 个会话的执行状态暂不可读"
    );
    assert_read_contract(&reader, 2);
}

#[test]
fn failed_runtime_revision_is_retried_and_success_clears_partial_health_error() {
    let fixture = Fixture::new(2, 2);
    let threads = [thread("bad", 2, "idle"), thread("good", 1, "idle")];
    let mut revisions = HashMap::new();
    let mut first = FakeRpc::new(&threads);
    first.fail_turn("bad", Reply::Request("temporary unreadable"));
    scan_runtime(&fixture.app, &mut first, &mut revisions, 1000.0).unwrap();
    assert!(!revisions.contains_key("bad"));
    assert!(revisions.contains_key("good"));
    assert_eq!(
        health(&fixture.app)["runtimeError"],
        "1 个会话的执行状态暂不可读"
    );
    let mut retry = FakeRpc::new(&threads);
    scan_runtime(&fixture.app, &mut retry, &mut revisions, 1000.0).unwrap();
    assert_eq!(retry.turn_ids(), ["bad"]);
    assert_eq!(revisions.len(), 2);
    assert_eq!(session(&fixture.app, "bad")["state"], "review");
    assert!(health(&fixture.app)["runtimeError"].is_null());
    assert_eq!(health(&fixture.app)["runtimeVerified"], true);
}

#[test]
fn runtime_transport_failure_propagates_without_reclassifying_manual_state() {
    let fixture = Fixture::new(2, 2);
    manual(&fixture.app, "broken");
    let before = decision(&fixture.app, "broken");
    let mut reader = FakeRpc::new(&[thread("broken", 2, "active"), thread("next", 1, "active")]);
    reader.fail_turn("broken", Reply::Timeout("runtime transport lost"));
    let mut revisions = HashMap::new();
    let error = scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap_err();
    assert!(error.to_string().contains("runtime transport lost"));
    assert!(error.downcast_ref::<RequestError>().is_none());
    assert_eq!(reader.turn_ids(), ["broken"]);
    assert_eq!(decision(&fixture.app, "broken"), before);
    assert_eq!(session(&fixture.app, "next")["state"], "unknown");
    assert!(revisions.is_empty());
    assert_read_contract(&reader, 2);
}

#[test]
fn runtime_list_connection_failure_leaves_saved_sessions_and_revision_cache_untouched() {
    let fixture = Fixture::new(2, 2);
    manual(&fixture.app, "saved");
    let before = decision(&fixture.app, "saved");
    let mut revisions = HashMap::from([("saved".to_string(), "existing-revision".to_string())]);
    let before_revisions = revisions.clone();
    let mut reader = FakeRpc::new(&[]);
    reader.pages[0] = Reply::Timeout("owner connection lost");
    let error = scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap_err();
    assert!(error.to_string().contains("owner connection lost"));
    assert!(error.downcast_ref::<RequestError>().is_none());
    assert_eq!(decision(&fixture.app, "saved"), before);
    assert_eq!(revisions, before_revisions);
    assert!(reader.turn_ids().is_empty());
    assert_eq!(fixture.app.store.sessions().unwrap().len(), 1);
}

#[test]
fn runtime_incomplete_pages_fail_before_discovery_or_any_turn_observation() {
    let fixture = Fixture::new(2, 1);
    manual(&fixture.app, "saved");
    let before = decision(&fixture.app, "saved");
    let mut reader = FakeRpc::new(&[thread("untrusted-partial", 2, "active")]);
    reader.pages[0] = Reply::Data(json!({
        "data": [thread("untrusted-partial", 2, "active")],
        "nextCursor": "remaining-page",
    }));
    let mut revisions = HashMap::new();
    let error = scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap_err();
    assert!(error.to_string().contains("桌面运行状态分页未完成"));
    assert!(reader.turn_ids().is_empty());
    assert_eq!(reader.list_calls().len(), 1);
    assert_eq!(decision(&fixture.app, "saved"), before);
    assert!(!fixture.app.store.contains("untrusted-partial").unwrap());
    assert!(revisions.is_empty());
    assert_eq!(health(&fixture.app)["runtimeVerified"], false);
    assert_read_contract(&reader, 2);
}

#[test]
fn runtime_pagination_uses_full_declared_scope_before_reading_owner_turns() {
    let fixture = Fixture::new(2, 3);
    let threads = [thread("first", 2, "active"), thread("second", 1, "active")];
    let mut reader = FakeRpc::new(&threads);
    reader.set_turn("first", raw_turn("first", "inProgress"));
    reader.set_turn("second", raw_turn("second", "inProgress"));
    reader.pages = VecDeque::from([
        Reply::Data(json!({"data": [threads[0]], "nextCursor": "next-owner-page"})),
        Reply::Data(json!({"data": [threads[0], threads[1]]})),
    ]);
    let mut revisions = HashMap::new();
    scan_runtime(&fixture.app, &mut reader, &mut revisions, 1000.0).unwrap();
    assert_eq!(reader.calls[0].0, "thread/list");
    assert_eq!(reader.calls[1].0, "thread/list");
    assert_eq!(reader.list_calls()[0]["cursor"], Value::Null);
    assert_eq!(reader.list_calls()[1]["cursor"], "next-owner-page");
    assert_eq!(reader.turn_ids(), ["first", "second"]);
    assert_eq!(revisions.len(), 2);
    assert_eq!(health(&fixture.app)["runtimeVerified"], true);
    assert_read_contract(&reader, 2);
}

#[test]
fn same_phase_runtime_input_rejects_stale_input_version_without_changing_state_version() {
    let (fixture, mut revisions, saved) = runtime_manual_fixture();
    runtime_extra_input(&fixture.app, &mut revisions);
    let updated = session(&fixture.app, "owner");
    assert_eq!(updated["inputVersion"], 2);
    assert_eq!(updated["version"], saved["version"]);
    assert_eq!(updated["state"], "completed");
    assert_eq!(updated["manual"], true);
    assert_eq!(updated["note"], saved["note"]);
    assert_eq!(updated["followUp"], true);
    let before = decision(&fixture.app, "owner");
    let error = fixture
        .app
        .request("/api/state", Some(&edit_body(&saved, "to_archive")))
        .unwrap_err();
    assert!(error.to_string().contains("新输入"));
    assert_eq!(decision(&fixture.app, "owner"), before);
    runtime_extra_input(&fixture.app, &mut revisions);
    assert_eq!(decision(&fixture.app, "owner"), before);
    let config = fixture.app.config.clone();
    drop(fixture.app);
    let reopened = App::new(config).unwrap();
    assert_eq!(decision(&reopened, "owner"), before);
}

#[test]
fn runtime_input_confirmation_clears_notice_and_preserves_omitted_note() {
    let (fixture, mut revisions, saved) = runtime_manual_fixture();
    runtime_extra_input(&fixture.app, &mut revisions);
    let current = session(&fixture.app, "owner");
    let body = edit_body(&current, "completed");
    assert!(body.get("note").is_none());
    assert_eq!(
        fixture.app.request("/api/state", Some(&body)).unwrap()["ok"],
        true
    );
    let confirmed = session(&fixture.app, "owner");
    assert_eq!(confirmed["followUp"], false);
    assert_eq!(confirmed["note"], saved["note"]);
    assert_eq!(confirmed["inputVersion"], current["inputVersion"]);
    assert_eq!(
        confirmed["version"].as_i64().unwrap(),
        current["version"].as_i64().unwrap() + 1
    );
    let acknowledged = decision(&fixture.app, "owner");
    runtime_extra_input(&fixture.app, &mut revisions);
    assert_eq!(decision(&fixture.app, "owner"), acknowledged);
}

#[test]
fn runtime_input_confirmation_requires_explicit_input_version() {
    let (fixture, _revisions, saved) = runtime_manual_fixture();
    let before = decision(&fixture.app, "owner");
    let mut body = edit_body(&saved, "to_archive");
    body.as_object_mut().unwrap().remove("inputVersion");
    let error = fixture.app.request("/api/state", Some(&body)).unwrap_err();
    assert!(error.to_string().contains("缺少输入版本"));
    assert_eq!(decision(&fixture.app, "owner"), before);
}
