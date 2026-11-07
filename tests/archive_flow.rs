#![allow(clippy::unwrap_used)]

use anyhow::{Result, bail};
use codex_session_board::{app::App, archive, config::Config, mcp, rpc::Rpc, store::Store};
use serde_json::{Value, json};
use std::collections::VecDeque;

#[path = "archive_flow/transport.rs"]
mod transport;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";

fn populate(store: &Store, state: &str) -> Result<()> {
    store.discover(&[json!({"id":FIRST,"name":"归档测试","updatedAt":1})])?;
    store.set_state(FIRST, Some(state), "保留备注", 0, Some(0))
}

#[derive(Default)]
struct Native {
    replies: VecDeque<Value>,
    calls: Vec<(String, Value)>,
    fail_archive: bool,
}

impl Rpc for Native {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.calls.push((method.to_string(), params));
        if method == "thread/archive" && self.fail_archive {
            bail!("lost acknowledgement");
        }
        Ok(self.replies.pop_front().unwrap())
    }
}

fn native() -> Native {
    Native {
        replies: VecDeque::from([
            json!({"data":[],"nextCursor":null}),
            json!({"thread":{"id":FIRST,"status":{"type":"idle"}}}),
            json!({"data":[]}),
            json!({}),
        ]),
        ..Native::default()
    }
}

fn assert_archived(store: &Store) -> Result<()> {
    assert!(store.sessions()?.is_empty());
    assert!(!store.contains(FIRST)?);
    assert!(store.pending()?.is_empty());
    assert!(store.pending_titles(None)?.is_empty());
    assert_eq!(store.note(FIRST)?, "保留备注");
    assert!(
        store
            .set_state(FIRST, Some("review"), "", 2, Some(0))
            .is_err()
    );
    assert!(
        store
            .archive(FIRST, 2, 0, |_| panic!("duplicate archive"))
            .is_err()
    );
    Ok(())
}

#[test]
fn archive_projection_has_five_states_and_preserves_legacy_records() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        sync_titles: false,
        ..Config::default()
    })?;
    populate(&app.store, "completed")?;
    let snapshot = app.snapshot()?;
    let states: Vec<_> = snapshot["states"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        states,
        ["unknown", "in_progress", "review", "blocked", "to_archive"]
    );
    let row = &snapshot["sessions"][0];
    assert_eq!(row["state"], "to_archive");
    assert_eq!(row["note"], "保留备注");
    assert_eq!(row["manual"], true);
    assert_eq!(row["version"], 1);
    assert_eq!(app.store.state_revision(FIRST)?.0, "completed");
    Ok(())
}

#[test]
fn archive_confirmation_persists_and_stale_observers_cannot_restore_the_card() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    populate(&store, "to_archive")?;
    let mut writer = native();
    store.archive(FIRST, 1, 0, |expected| {
        archive::archive(&mut writer, FIRST, 10, expected)
    })?;
    assert_eq!(
        writer
            .calls
            .iter()
            .filter(|(method, _)| method == "thread/archive")
            .count(),
        1
    );
    assert_eq!(writer.calls[3].1, json!({"threadId":FIRST}));
    assert_archived(&store)?;
    store.discover(&[json!({"id":FIRST,"name":"旧扫描","updatedAt":2})])?;
    store.observe(
        FIRST,
        &json!({"id":"late","status":"completed","hasFinal":true}),
        None,
    )?;
    assert!(!store.observe_runtime(
        FIRST,
        &json!({"type":"active"}),
        &json!({"id":"late","status":"inProgress"}),
        None
    )?);
    assert_archived(&store)?;
    drop(store);
    assert_archived(&Store::open(directory.path(), true)?)?;
    Ok(())
}

#[test]
fn archive_rejects_stale_versions_new_input_and_wrong_state_before_external_io() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), false)?;
    populate(&store, "review")?;
    for (version, input) in [(0, 0), (1, 1), (1, 0)] {
        assert!(
            store
                .archive(FIRST, version, input, |_| panic!("invalid archive"))
                .is_err()
        );
    }
    store.observe(FIRST, &json!({"id":"old","userItems":["old-input"]}), None)?;
    store.set_state(FIRST, Some("to_archive"), "备注", 1, Some(1))?;
    store.observe(
        FIRST,
        &json!({"id":"old","userItems":["old-input","new-input"]}),
        None,
    )?;
    assert!(
        store
            .archive(FIRST, 2, 0, |_| panic!("stale input"))
            .is_err()
    );
    assert!(
        store
            .archive(FIRST, 2, 2, |_| panic!("unreviewed input"))
            .is_err()
    );
    assert_eq!(store.sessions()?.len(), 1);
    Ok(())
}

#[test]
fn archive_lost_ack_keeps_card_and_retry_reads_back_before_another_write() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    populate(&store, "to_archive")?;
    let mut writer = native();
    writer.fail_archive = true;
    assert!(
        store
            .archive(FIRST, 1, 0, |expected| archive::archive(
                &mut writer,
                FIRST,
                10,
                expected
            ))
            .is_err()
    );
    assert_eq!(store.sessions()?.len(), 1);
    assert_eq!(store.state_revision(FIRST)?.1, 1);
    assert_eq!(store.note(FIRST)?, "保留备注");
    assert_eq!(store.pending_titles(None)?.len(), 1);
    drop(store);
    let store = Store::open(directory.path(), true)?;
    let mut reader = Native {
        replies: VecDeque::from([
            json!({"data":[],"nextCursor":"next"}),
            json!({"data":[{"id":FIRST}],"nextCursor":null}),
        ]),
        ..Native::default()
    };
    store.archive(FIRST, 1, 0, |expected| {
        archive::archive(&mut reader, FIRST, 10, expected)
    })?;
    assert!(
        reader
            .calls
            .iter()
            .all(|(method, params)| method == "thread/list" && params["archived"] == true)
    );
    assert_eq!(reader.calls[1].1["cursor"], "next");
    assert!(store.sessions()?.is_empty());
    Ok(())
}

fn native_turn(identifier: &str, users: &[&str]) -> Value {
    json!({"id":identifier,"status":"completed",
        "items":users.iter().map(|identifier| json!({"type":"userMessage","id":identifier})).collect::<Vec<_>>()})
}

#[test]
fn archive_checks_native_inputs_even_when_local_revision_has_not_changed() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), false)?;
    populate(&store, "to_archive")?;
    store.observe(FIRST, &json!({"id":"old","userItems":["original"]}), None)?;
    store.set_state(FIRST, Some("to_archive"), "保留备注", 1, Some(1))?;
    for turn in [
        native_turn("new", &["new-input"]),
        native_turn("old", &["original", "new-input"]),
    ] {
        let mut writer = native();
        writer.replies[2] = json!({"data":[turn]});
        let result = store.archive(FIRST, 2, 1, |expected| {
            archive::archive(&mut writer, FIRST, 10, expected)
        });
        assert!(result.unwrap_err().to_string().contains("原生会话有新输入"));
        assert_eq!(writer.calls.len(), 3);
        assert_eq!(store.sessions()?[0]["inputVersion"], 1);
        assert_eq!(store.state_revision(FIRST)?.1, 2);
    }
    let mut writer = native();
    writer.replies[2] = json!({"data":[native_turn("old", &["original"])]});
    store.archive(FIRST, 2, 1, |expected| {
        archive::archive(&mut writer, FIRST, 10, expected)
    })?;
    assert_archived(&store)?;
    Ok(())
}

#[test]
fn archive_unverifiable_native_input_never_reaches_the_write() {
    for result in [
        json!({}),
        json!({"data":[{}]}),
        json!({"data":[{"id":"turn","status":"inProgress","items":[]} ]}),
    ] {
        let mut writer = native();
        writer.replies[2] = result;
        assert!(archive::archive(&mut writer, FIRST, 10, &json!({})).is_err());
        assert_eq!(writer.calls.len(), 3);
    }
}

#[test]
fn archive_rejects_malformed_user_items_even_when_other_inputs_match() {
    for identifier in [Value::Null, json!(1), json!("")] {
        let mut turn = native_turn("old", &["original"]);
        turn["items"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"userMessage","id":identifier}));
        let mut writer = native();
        writer.replies[2] = json!({"data":[turn]});
        let expected = json!({"id":"old","userItems":["original"]});
        let error = archive::archive(&mut writer, FIRST, 10, &expected).unwrap_err();
        assert!(error.to_string().contains("无法核对原生最新输入"));
        assert_eq!(writer.calls.len(), 3);
    }
}

#[test]
fn archive_unknown_running_and_malformed_native_results_do_not_confirm_success() {
    for status in ["active", "systemError", "unknown"] {
        let mut writer = native();
        writer.replies[1]["thread"]["status"]["type"] = json!(status);
        assert!(archive::archive(&mut writer, FIRST, 10, &json!({})).is_err());
        assert_eq!(writer.calls.len(), 2);
    }
    let mut writer = native();
    writer.replies[1]["thread"]["id"] = json!("wrong");
    assert!(archive::archive(&mut writer, FIRST, 10, &json!({})).is_err());
    assert_eq!(writer.calls.len(), 2);
    let mut writer = native();
    writer.replies[3] = Value::Null;
    assert!(archive::archive(&mut writer, FIRST, 10, &json!({})).is_err());
    let mut writer = native();
    writer.replies[0] = json!({"data":[],"nextCursor":"more"});
    assert!(archive::archive(&mut writer, FIRST, 1, &json!({})).is_err());
    assert_eq!(writer.calls.len(), 1);
}

#[test]
fn archive_mcp_resource_exposes_only_an_explicit_app_action() {
    let tools = mcp::tools();
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "board_archive")
        .unwrap();
    assert_eq!(tool["_meta"]["ui"]["visibility"], json!(["app"]));
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], true);
    assert_eq!(
        tool["inputSchema"]["required"],
        json!(["id", "version", "inputVersion"])
    );
    let resource = mcp::resource();
    let html = resource["text"].as_str().unwrap();
    assert!(html.contains("\"/api/archive\":\"board_archive\""));
    assert!(html.contains("archiveSession(session,archive)"));
    assert!(!html.contains("\"调整状态\""));
    assert!(html.contains("openEditor(session.id,state,session)"));
}
