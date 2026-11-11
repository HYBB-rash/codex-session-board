#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use codex_session_board::model::{STATES, now, recommend, runtime_state};
use codex_session_board::store::Store;
use rusqlite::types::ValueRef;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
#[path = "support/model_database.rs"]
mod model_database;
#[path = "support/model_replay.rs"]
mod model_replay;
use model_replay::{ReplayCounts, TraceDatabase};

const PYTHON_MODEL: &str = r#"
import json
import runpy
import sys

model = runpy.run_path(sys.argv[1])
graph = model["reachable_paths"]()

def snapshot(flow):
    return {
        "state": flow.state,
        "manual": flow.manual,
        "note": flow.note,
        "version": flow.version,
    }

traces = []
for origin, path in graph:
    for event in model["EVENTS"]:
        current = model["Flow"]()
        steps = []
        for action in (*path, event):
            current = current.step(action)
            steps.append({"event": action, "expected": snapshot(current)})
        traces.append({
            "path": list(path),
            "event": event,
            "origin": [origin.state, origin.phase, origin.manual, origin.note],
            "steps": steps,
        })
print(json.dumps({
    "stateCount": len(graph),
    "events": model["EVENTS"],
    "states": model["STATE_LABELS"],
    "traces": traces,
}, ensure_ascii=False))
"#;

fn acceptance_path(name: &str) -> PathBuf {
    std::env::current_dir()
        .expect("test runner project directory")
        .join("acceptance")
        .join(name)
}

fn thread(identifier: &str, updated: i64) -> Value {
    json!({
        "id": identifier, "name": "示例会话",
        "cwd": "/fixture/project", "updatedAt": updated,
    })
}

fn turn(identifier: &str, status: &str) -> Value {
    json!({
        "id": identifier, "status": status, "userItems": ["u1"],
        "hasFinal": true, "startedAt": 1.0,
    })
}

fn fixture(sync_titles: bool) -> (TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path(), sync_titles).unwrap();
    store.discover(&[thread("sample", 10)]).unwrap();
    (directory, store)
}

fn session(store: &Store) -> Value {
    session_named(store, "sample")
}

fn session_named(store: &Store, identifier: &str) -> Value {
    store
        .sessions()
        .unwrap()
        .into_iter()
        .find(|row| row["id"] == identifier)
        .unwrap()
}

fn integer(value: &Value, field: &str) -> i64 {
    value[field].as_i64().unwrap()
}

fn select(store: &Store, state: Option<&str>, note: &str) {
    let row = session(store);
    store
        .set_state(
            "sample",
            state,
            note,
            integer(&row, "version"),
            Some(integer(&row, "inputVersion")),
        )
        .unwrap();
}

fn selected_at(store: &Store) -> f64 {
    store
        .connection()
        .unwrap()
        .query_row(
            "SELECT selected_at FROM sessions WHERE id='sample'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn set_selected_at(store: &Store, identifier: &str, timestamp: f64) {
    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE sessions SET selected_at=? WHERE id=?",
            rusqlite::params![timestamp, identifier],
        )
        .unwrap();
}

fn observe(store: &Store, latest: &Value) {
    store.observe("sample", latest, None).unwrap();
}

fn runtime(store: &Store, phase: &str, identifier: &str, flags: &[&str]) -> bool {
    let status = if phase == "inProgress" {
        json!({"type": "active", "activeFlags": flags})
    } else {
        json!({"type": "idle"})
    };
    let mut latest = turn(identifier, phase);
    latest["startedAt"] = json!(now() + 1.0);
    latest["hasFinal"] = json!(false);
    store
        .observe_runtime("sample", &status, &latest, None)
        .unwrap()
}

fn row_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut fields = serde_json::Map::new();
    for (index, name) in row.as_ref().column_names().iter().enumerate() {
        let value = match row.get_ref(index)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(number) => json!(number),
            ValueRef::Real(number) => json!(number),
            ValueRef::Text(text) => json!(std::str::from_utf8(text).unwrap()),
            ValueRef::Blob(blob) => json!(blob),
        };
        fields.insert((*name).to_string(), value);
    }
    Ok(Value::Object(fields))
}

fn table_rows(store: &Store, table: &str) -> Vec<Value> {
    let connection = store.connection().unwrap();
    let mut statement = connection
        .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
        .unwrap();
    statement
        .query_map([], row_json)
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn database_snapshot(store: &Store) -> Value {
    json!({
        "sessions": table_rows(store, "sessions"),
        "changes": table_rows(store, "changes"),
        "title_updates": table_rows(store, "title_updates"),
        "settings": table_rows(store, "settings"),
    })
}

fn audit(store: &Store) -> Vec<Value> {
    table_rows(store, "changes")
        .into_iter()
        .map(|row| {
            json!({
                "thread_id": row["thread_id"],
                "state": row["state"],
                "note": row["note"],
            })
        })
        .collect()
}

fn reject_title_inserts(store: &Store) {
    store
        .connection()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_title_insert BEFORE INSERT ON title_updates
             BEGIN SELECT RAISE(ABORT, 'injected title queue failure'); END;",
        )
        .unwrap();
}

mod store_tests {
    use super::*;

    #[test]
    fn test_discovery_is_automatic_idempotent_and_survives_restart() {
        let (directory, store) = fixture(false);
        store
            .discover(&[thread("sample", 10), thread("sample", 10)])
            .unwrap();
        drop(store);
        let store = Store::open(directory.path(), false).unwrap();
        assert_eq!(store.sessions().unwrap().len(), 1);
        assert_eq!(session(&store)["state"], "unknown");
        assert!(store.contains("sample").unwrap());
        assert!(!store.contains("missing").unwrap());
    }

    #[test]
    fn test_success_requires_terminal_answer_to_suggest_review() {
        let mut latest = turn("t1", "completed");
        latest["hasFinal"] = json!(false);
        assert_eq!(recommend(&latest).0, "unknown");
        assert_eq!(recommend(&turn("t1", "completed")).0, "review");
        for phase in ["failed", "interrupted", "inProgress", "newStatus"] {
            assert_eq!(recommend(&turn("t1", phase)).0, "unknown");
        }
        latest["status"] = Value::Null;
        assert_eq!(recommend(&latest).0, "unknown");
    }

    #[test]
    fn test_assistant_updates_never_reopen_manual_completion() {
        let (directory, store) = fixture(false);
        observe(&store, &turn("t1", "inProgress"));
        select(&store, Some("completed"), "验收通过");
        observe(&store, &turn("t1", "completed"));
        store.discover(&[thread("sample", 20)]).unwrap();
        assert_eq!(session(&store)["state"], "completed");
        assert_eq!(session(&store)["followUp"], false);
        drop(store);
        let store = Store::open(directory.path(), false).unwrap();
        assert_eq!(session(&store)["note"], "验收通过");
        assert_eq!(session(&store)["state"], "completed");
    }

    #[test]
    fn test_same_turn_new_user_input_prompts_review_without_overwriting() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("completed"), "");
        let mut latest = turn("t1", "completed");
        latest["userItems"] = json!(["u1", "u2"]);
        observe(&store, &latest);
        assert_eq!(session(&store)["followUp"], true);
        assert_eq!(session(&store)["state"], "completed");
        assert_eq!(session(&store)["version"], 1);
        assert_eq!(session(&store)["inputVersion"], 2);
    }

    #[test]
    fn test_new_turn_requires_user_input_after_selection_boundary() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("to_archive"), "");
        let boundary = selected_at(&store);
        let mut maintenance = turn("maintenance", "completed");
        maintenance["userItems"] = json!([]);
        maintenance["startedAt"] = json!(boundary + 1.0);
        observe(&store, &maintenance);
        assert_eq!(session(&store)["followUp"], false);
        let mut replay = turn("late-replay", "completed");
        replay["startedAt"] = json!(boundary);
        observe(&store, &replay);
        assert_eq!(session(&store)["followUp"], false);
        replay["id"] = json!("t2");
        replay["startedAt"] = json!(boundary + 1.0);
        observe(&store, &replay);
        assert_eq!(session(&store)["followUp"], true);
        assert_eq!(session(&store)["state"], "to_archive");
    }

    #[test]
    fn test_manual_block_survives_success_and_failed_observation() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("blocked"), "等待资料");
        observe(&store, &turn("t2", "inProgress"));
        store
            .observe("sample", &json!({}), Some("connection lost"))
            .unwrap();
        store.discover(&[]).unwrap();
        assert_eq!(session(&store)["state"], "blocked");
        assert_eq!(session(&store)["observationError"], "connection lost");
        assert_eq!(session(&store)["note"], "等待资料");
        assert_eq!(session(&store)["turnStatus"], "inProgress");
        assert_eq!(session(&store)["inputVersion"], 2);
    }

    #[test]
    fn test_block_requires_reason_and_stale_edits_are_rejected() {
        let (_directory, store) = fixture(true);
        let before = database_snapshot(&store);
        assert!(
            store
                .set_state("sample", Some("blocked"), " ", 0, None)
                .is_err()
        );
        assert_eq!(database_snapshot(&store), before);
        select(&store, Some("review"), "");
        let before = database_snapshot(&store);
        let error = store
            .set_state("sample", Some("completed"), "", 0, None)
            .unwrap_err();
        assert!(error.to_string().contains("状态已更新"));
        assert_eq!(database_snapshot(&store), before);
        select(&store, None, "");
        assert_eq!(session(&store)["manual"], false);
        assert_eq!(session(&store)["state"], "unknown");
    }

    #[test]
    fn test_new_activity_invalidates_cached_suggestion_until_history_is_read() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        assert_eq!(session(&store)["state"], "review");
        store.discover(&[thread("sample", 20)]).unwrap();
        assert_eq!(session(&store)["state"], "unknown");
        observe(&store, &turn("t2", "completed"));
        assert_eq!(session(&store)["state"], "review");
    }

    #[test]
    fn test_new_input_notice_survives_automatic_turns_and_restart() {
        let (directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("completed"), "");
        let mut latest = turn("t2", "completed");
        latest["startedAt"] = json!(selected_at(&store) + 1.0);
        observe(&store, &latest);
        assert_eq!(session(&store)["followUp"], true);
        latest["id"] = json!("automatic");
        latest["userItems"] = json!([]);
        observe(&store, &latest);
        assert_eq!(session(&store)["followUp"], true);
        drop(store);
        let store = Store::open(directory.path(), false).unwrap();
        assert_eq!(session(&store)["followUp"], true);
        select(&store, Some("completed"), "新输入已核对");
        assert_eq!(session(&store)["followUp"], false);
    }

    #[test]
    fn new_input_notice_survives_runtime_stages_and_restart_until_manual_ack() {
        let (directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("completed"), "已验收旧结果");
        let mut latest = turn("t1", "completed");
        latest["userItems"] = json!(["u1", "u2"]);
        observe(&store, &latest);
        assert_eq!(session(&store)["followUp"], true);
        let input_version = session(&store)["inputVersion"].clone();
        for (phase, flags, expected) in [
            ("inProgress", vec![], "in_progress"),
            ("inProgress", vec!["waitingOnApproval"], "blocked"),
            ("completed", vec![], "review"),
        ] {
            runtime(&store, phase, "t2", &flags);
            let row = session(&store);
            assert_eq!(row["state"], expected);
            assert_eq!(row["followUp"], true);
            assert_eq!(row["inputVersion"], input_version);
            assert_eq!(row["note"], "已验收旧结果");
        }
        drop(store);
        let store = Store::open(directory.path(), false).unwrap();
        assert_eq!(session(&store)["followUp"], true);
        assert_eq!(session(&store)["state"], "review");
        select(&store, Some("completed"), "新输入已核对");
        assert_eq!(session(&store)["followUp"], false);
        assert_eq!(session(&store)["inputVersion"], input_version);
    }

    #[test]
    fn test_new_input_during_drag_rejects_stale_confirmation() {
        let (_directory, store) = fixture(true);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("completed"), "验收通过");
        let saved = session(&store);
        let mut latest = turn("t1", "completed");
        latest["userItems"] = json!(["u1", "u2"]);
        observe(&store, &latest);
        let before = database_snapshot(&store);
        let error = store
            .set_state(
                "sample",
                Some("to_archive"),
                "验收通过",
                integer(&saved, "version"),
                Some(integer(&saved, "inputVersion")),
            )
            .unwrap_err();
        assert!(error.to_string().contains("新输入"));
        assert_eq!(database_snapshot(&store), before);
        assert_eq!(session(&store)["followUp"], true);
        assert_eq!(session(&store)["state"], "completed");
        assert_eq!(session(&store)["version"], saved["version"]);
    }

    #[test]
    fn test_assistant_only_updates_do_not_invalidate_drag_input_version() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "inProgress"));
        let saved = session(&store);
        observe(&store, &turn("t1", "completed"));
        store
            .set_state(
                "sample",
                Some("review"),
                "保留备注",
                integer(&saved, "version"),
                Some(integer(&saved, "inputVersion")),
            )
            .unwrap();
        assert_eq!(session(&store)["note"], "保留备注");
        assert_eq!(session(&store)["inputVersion"], saved["inputVersion"]);
    }

    #[test]
    fn invalid_state_and_oversized_note_leave_all_tables_unchanged() {
        let (_directory, store) = fixture(true);
        for (state, note) in [
            (Some("invented"), "备注".to_string()),
            (Some("review"), "字".repeat(2001)),
        ] {
            let before = database_snapshot(&store);
            assert!(store.set_state("sample", state, &note, 0, None).is_err());
            assert_eq!(database_snapshot(&store), before);
        }
        select(&store, Some("review"), &"字".repeat(2000));
        assert_eq!(
            session(&store)["note"].as_str().unwrap().chars().count(),
            2000
        );
    }

    #[test]
    fn input_revision_counts_new_inputs_without_counting_assistant_updates() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "inProgress"));
        assert_eq!(session(&store)["inputVersion"], 1);
        observe(&store, &turn("t1", "completed"));
        assert_eq!(session(&store)["inputVersion"], 1);
        let mut latest = turn("t1", "completed");
        latest["userItems"] = json!(["u1", "u2", "u3"]);
        observe(&store, &latest);
        assert_eq!(session(&store)["inputVersion"], 2);
        latest["userItems"] = json!(["u3", "u2", "u1"]);
        observe(&store, &latest);
        assert_eq!(session(&store)["inputVersion"], 2);
        latest["id"] = json!("t2");
        observe(&store, &latest);
        assert_eq!(session(&store)["inputVersion"], 3);
        latest["id"] = json!("maintenance");
        latest["userItems"] = json!([]);
        observe(&store, &latest);
        assert_eq!(session(&store)["inputVersion"], 3);
        assert_eq!(session(&store)["version"], 0);
    }

    #[test]
    fn failed_history_read_preserves_evidence_and_later_success_clears_error() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        let before = session(&store);
        store
            .observe("sample", &json!({}), Some("暂不可读"))
            .unwrap();
        let after = session(&store);
        for field in [
            "state",
            "turnStatus",
            "version",
            "inputVersion",
            "observedAt",
        ] {
            assert_eq!(after[field], before[field], "{field}");
        }
        assert_eq!(after["observationError"], "暂不可读");
        observe(&store, &turn("t1", "completed"));
        assert!(session(&store)["observationError"].is_null());
    }

    #[test]
    fn history_input_without_execution_never_claims_a_runtime_start() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("unexecuted-input", "inProgress"));
        assert_eq!(session(&store)["state"], "unknown");
        assert_eq!(session(&store)["source"], "history");
        observe(&store, &turn("changed-id-only", "inProgress"));
        assert_eq!(session(&store)["state"], "unknown");
        assert_eq!(session(&store)["version"], 0);
    }

    #[test]
    fn discovery_updates_metadata_but_never_removes_missing_records() {
        let (_directory, store) = fixture(false);
        select(&store, Some("completed"), "保留");
        let mut another = thread("another", 30);
        another["name"] = json!("新标题\n不存第二行");
        another["cwd"] = json!("/fixture/second");
        store.discover(&[another]).unwrap();
        let rows = store.sessions().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], "another");
        assert_eq!(rows[0]["title"], "新标题");
        store.discover(&[]).unwrap();
        assert_eq!(session(&store)["state"], "completed");
        assert_eq!(session(&store)["note"], "保留");
    }

    #[test]
    fn history_queue_attempts_rotate_without_starving_unread_sessions() {
        let (_directory, store) = fixture(false);
        store.discover(&[thread("another", 30)]).unwrap();
        let pending = store.pending().unwrap();
        assert_eq!(pending[0]["id"], "another");
        store
            .observe("another", &json!({}), Some("不可读"))
            .unwrap();
        assert_eq!(store.pending().unwrap()[0]["id"], "sample");
        observe(&store, &turn("t1", "completed"));
        assert_eq!(store.pending().unwrap()[0]["id"], "another");
        assert_eq!(store.pending().unwrap().len(), 2);
    }
}

mod runtime_tests {
    use super::*;

    #[test]
    fn test_execution_completion_and_next_reply_follow_runtime() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        assert_eq!(session(&store)["state"], "in_progress");
        assert_eq!(session(&store)["manual"], false);
        runtime(&store, "completed", "t1", &[]);
        assert_eq!(session(&store)["state"], "review");
        select(&store, Some("completed"), "验收通过");
        runtime(&store, "completed", "t1", &[]);
        assert_eq!(session(&store)["state"], "completed");
        runtime(&store, "inProgress", "t2", &[]);
        assert_eq!(session(&store)["state"], "in_progress");
        assert_eq!(session(&store)["note"], "验收通过");
    }

    #[test]
    fn test_repeated_approval_and_resume_in_one_turn() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        for _iteration in 0..2 {
            runtime(&store, "inProgress", "t1", &["waitingOnApproval"]);
            assert_eq!(session(&store)["state"], "blocked");
            assert!(session(&store)["reason"].as_str().unwrap().contains("批准"));
            runtime(&store, "inProgress", "t1", &[]);
            assert_eq!(session(&store)["state"], "in_progress");
        }
        runtime(&store, "completed", "t1", &[]);
        assert_eq!(session(&store)["state"], "review");
    }

    #[test]
    fn test_explicit_interaction_wait_is_blocked_but_plain_final_reply_is_review() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &["waitingOnUserInput"]);
        assert_eq!(session(&store)["state"], "blocked");
        assert!(session(&store)["reason"].as_str().unwrap().contains("回答"));
        runtime(&store, "completed", "t1", &[]);
        assert_eq!(session(&store)["state"], "review");
    }

    #[test]
    fn test_failed_and_interrupted_execution_are_blocked() {
        let (_directory, store) = fixture(false);
        for phase in ["failed", "interrupted"] {
            runtime(&store, "inProgress", phase, &[]);
            runtime(&store, phase, phase, &[]);
            assert_eq!(session(&store)["state"], "blocked");
            assert_eq!(session(&store)["manual"], false);
        }
    }

    #[test]
    fn test_first_observation_does_not_reclassify_old_failure() {
        let (_directory, store) = fixture(true);
        runtime(&store, "failed", "t1", &[]);
        assert_eq!(session(&store)["state"], "unknown");
        assert_eq!(session(&store)["version"], 0);
        assert!(audit(&store).is_empty());
        assert!(store.pending_titles(None).unwrap().is_empty());
        runtime(&store, "inProgress", "new", &[]);
        runtime(&store, "failed", "new", &[]);
        assert_eq!(session(&store)["state"], "blocked");
    }

    #[test]
    fn test_new_execution_that_fails_between_polls_is_still_blocked() {
        let (_directory, store) = fixture(false);
        let mut latest = turn("t1", "failed");
        latest["startedAt"] = json!(1001.0);
        latest["completedAt"] = json!(1002.0);
        store
            .observe_runtime("sample", &json!({"type": "idle"}), &latest, Some(1000.0))
            .unwrap();
        assert_eq!(session(&store)["state"], "blocked");
        assert_eq!(session(&store)["source"], "runtime");
        assert_eq!(session(&store)["version"], 1);
    }

    #[test]
    fn test_manual_stage_override_survives_duplicate_snapshot_and_restart() {
        let (directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        select(&store, Some("review"), "人工检查");
        let saved = session(&store);
        runtime(&store, "inProgress", "t1", &[]);
        drop(store);
        let store = Store::open(directory.path(), false).unwrap();
        runtime(&store, "inProgress", "t1", &[]);
        assert_eq!(session(&store)["state"], "review");
        assert_eq!(session(&store)["version"], saved["version"]);
        assert_eq!(session(&store)["manual"], true);
        runtime(&store, "inProgress", "t1", &["waitingOnApproval"]);
        assert_eq!(session(&store)["state"], "blocked");
        assert_eq!(session(&store)["note"], "人工检查");
    }

    #[test]
    fn test_existing_manual_state_is_preserved_when_runtime_is_first_enabled() {
        let (_directory, store) = fixture(false);
        observe(&store, &turn("t1", "completed"));
        select(&store, Some("to_archive"), "收尾");
        runtime(&store, "completed", "t1", &[]);
        assert_eq!(session(&store)["state"], "to_archive");
        runtime(&store, "inProgress", "new", &[]);
        assert_eq!(session(&store)["state"], "in_progress");
        assert_eq!(session(&store)["note"], "收尾");
    }

    #[test]
    fn test_new_phase_rejects_a_stale_drag_without_erasing_manual_note() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        select(&store, Some("review"), "保留人工备注");
        let saved = session(&store);
        runtime(&store, "inProgress", "t1", &["waitingOnApproval"]);
        let before = database_snapshot(&store);
        let error = store
            .set_state(
                "sample",
                Some("completed"),
                "",
                integer(&saved, "version"),
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("状态已更新"));
        assert_eq!(database_snapshot(&store), before);
        assert_eq!(session(&store)["state"], "blocked");
        assert_eq!(session(&store)["note"], "保留人工备注");
    }

    #[test]
    fn test_duplicate_phase_does_not_invalidate_manual_edit() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        let saved = session(&store);
        runtime(&store, "inProgress", "t1", &[]);
        store
            .set_state(
                "sample",
                Some("completed"),
                "人工确认",
                integer(&saved, "version"),
                None,
            )
            .unwrap();
        runtime(&store, "completed", "t1", &[]);
        assert_eq!(session(&store)["state"], "review");
        assert_eq!(session(&store)["note"], "人工确认");
    }

    #[test]
    fn test_late_terminal_snapshot_does_not_override_a_later_manual_decision() {
        let (_directory, store) = fixture(false);
        for phase in ["completed", "failed", "interrupted"] {
            runtime(&store, "inProgress", phase, &[]);
            select(&store, Some("completed"), "已核对结果");
            let saved = session(&store);
            let mut latest = turn(phase, phase);
            latest["completedAt"] = json!(selected_at(&store) - 1.0);
            store
                .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
                .unwrap();
            assert_eq!(session(&store)["state"], "completed", "{phase}");
            assert_eq!(session(&store)["version"], saved["version"], "{phase}");
            assert_eq!(session(&store)["manual"], true, "{phase}");
        }
    }

    #[test]
    fn test_completion_after_manual_change_still_goes_to_review() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        select(&store, Some("completed"), "");
        let mut latest = turn("t1", "completed");
        latest["completedAt"] = json!(selected_at(&store) + 1.0);
        store
            .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
            .unwrap();
        assert_eq!(session(&store)["state"], "review");
        assert_eq!(session(&store)["manual"], false);
    }

    #[test]
    fn test_stale_history_cannot_override_runtime_and_opening_does_not_write() {
        let (_directory, store) = fixture(false);
        runtime(&store, "inProgress", "t1", &[]);
        let saved = session(&store);
        observe(&store, &turn("t1", "completed"));
        store.discover(&[thread("sample", 20)]).unwrap();
        assert_eq!(session(&store)["state"], "in_progress");
        assert_eq!(session(&store)["version"], saved["version"]);
        let before = database_snapshot(&store);
        let _rows = store.sessions().unwrap();
        assert_eq!(database_snapshot(&store), before);
    }

    #[test]
    fn test_runtime_requires_consistent_active_turn_and_normal_end_needs_no_final() {
        assert!(runtime_state(&json!({"type": "active"}), &turn("t1", "completed")).is_none());
        let mut latest = turn("t1", "completed");
        latest["hasFinal"] = json!(false);
        assert_eq!(
            runtime_state(&json!({"type": "idle"}), &latest).unwrap().0,
            "review"
        );
        assert_eq!(
            runtime_state(&json!({"type": "systemError"}), &latest)
                .unwrap()
                .0,
            "blocked"
        );
    }

    #[test]
    fn old_failure_boundary_uses_observer_start_and_persists_deduplication() {
        for phase in ["failed", "interrupted"] {
            let (directory, store) = fixture(false);
            let mut latest = turn("old", phase);
            latest["startedAt"] = json!(999.0);
            store
                .observe_runtime("sample", &json!({"type": "idle"}), &latest, Some(1000.0))
                .unwrap();
            assert_eq!(session(&store)["state"], "unknown");
            drop(store);
            let store = Store::open(directory.path(), false).unwrap();
            store
                .observe_runtime("sample", &json!({"type": "idle"}), &latest, Some(998.0))
                .unwrap();
            assert_eq!(session(&store)["version"], 0);
            latest["id"] = json!("new");
            latest["startedAt"] = json!(1000.0);
            store
                .observe_runtime("sample", &json!({"type": "idle"}), &latest, Some(1000.0))
                .unwrap();
            assert_eq!(session(&store)["state"], "blocked");
        }
    }

    #[test]
    fn duplicate_flags_are_order_independent_and_leave_audit_and_queue_unchanged() {
        let (_directory, store) = fixture(true);
        runtime(
            &store,
            "inProgress",
            "t1",
            &["waitingOnApproval", "waitingOnUserInput"],
        );
        let before = database_snapshot(&store);
        assert!(!runtime(
            &store,
            "inProgress",
            "t1",
            &["waitingOnUserInput", "waitingOnApproval"],
        ));
        assert_eq!(database_snapshot(&store), before);
    }

    #[test]
    fn irrelevant_or_unidentified_runtime_evidence_does_not_write() {
        let (_directory, store) = fixture(true);
        let before = database_snapshot(&store);
        for (status, latest) in [
            (json!({"type": "notLoaded"}), json!({})),
            (json!({"type": "active"}), turn("t1", "completed")),
            (json!({"type": "idle"}), turn("t1", "inProgress")),
            (json!({"type": "active"}), json!({"status": "inProgress"})),
        ] {
            assert!(
                !store
                    .observe_runtime("sample", &status, &latest, None)
                    .unwrap()
            );
            assert_eq!(database_snapshot(&store), before);
        }
    }

    #[test]
    fn empty_turn_identifier_does_not_create_runtime_state_or_title_work() {
        let (_directory, store) = fixture(true);
        let before = database_snapshot(&store);
        let status = json!({"type": "active"});
        let latest = json!({"id": "", "status": "inProgress"});
        assert!(
            !store
                .observe_runtime("sample", &status, &latest, None)
                .unwrap()
        );
        assert_eq!(database_snapshot(&store), before);
    }
}

fn python_model() -> Value {
    let output = Command::new("python3")
        .args(["-B", "-c", PYTHON_MODEL])
        .arg(acceptance_path("model.py"))
        .output()
        .expect("python3 must execute the frozen acceptance reference model");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn flow_snapshot(store: &Store) -> Value {
    let row = session_named(store, "contract-thread");
    json!({
        "state": row["state"], "manual": row["manual"],
        "note": row["note"], "version": row["version"],
    })
}

fn model_runtime(store: &Store, phase: &str, clock: f64) {
    let mut status = json!({"type": "active"});
    let mut latest = json!({"id": "round-one", "status": "inProgress", "startedAt": 1001.0});
    match phase {
        "running" => {}
        "approval" => status["activeFlags"] = json!(["waitingOnApproval"]),
        "input" => status["activeFlags"] = json!(["waitingOnUserInput"]),
        "error" => status["type"] = json!("systemError"),
        "finished" => {
            status["type"] = json!("idle");
            latest["status"] = json!("completed");
            latest["completedAt"] = json!(clock);
        }
        _ => panic!("unexpected model phase: {phase}"),
    }
    store
        .observe_runtime("contract-thread", &status, &latest, Some(1000.0))
        .unwrap();
}

fn model_step(store: &Store, event: &str, clock: f64) {
    if let Some(state) = event.strip_prefix("manual:") {
        let row = session_named(store, "contract-thread");
        store
            .set_state(
                "contract-thread",
                Some(state),
                "人工备注",
                integer(&row, "version"),
                Some(integer(&row, "inputVersion")),
            )
            .unwrap();
        set_selected_at(store, "contract-thread", clock);
    } else if event == "view" {
        let before = database_snapshot(store);
        let _rows = store.sessions().unwrap();
        assert_eq!(database_snapshot(store), before);
    } else {
        model_runtime(store, event, clock);
    }
}

fn initial_model_store(directory: &Path) -> Store {
    let store = Store::open(directory, false).unwrap();
    store
        .discover(&[json!({
            "id": "contract-thread", "name": "验收会话",
            "cwd": "/fixture", "updatedAt": 1,
        })])
        .unwrap();
    store
        .observe(
            "contract-thread",
            &json!({
                "id": "round-one", "status": "inProgress",
                "userItems": ["input-one"], "startedAt": 1001.0,
            }),
            None,
        )
        .unwrap();
    model_runtime(&store, "running", 1000.0);
    assert_eq!(
        flow_snapshot(&store),
        json!({
            "state": "in_progress", "manual": false, "note": "", "version": 1,
        })
    );
    assert_eq!(store.sessions().unwrap()[0]["inputVersion"], 1);
    store
}

fn complete_snapshot(store: &Store) -> model_database::Snapshot {
    let database = store.connection().unwrap();
    model_database::snapshot(&database).unwrap()
}

fn trace_database(directory: &Path) -> TraceDatabase {
    let store = Store::open(directory, false).unwrap();
    let database = store.connection().unwrap();
    TraceDatabase::capture(&database, directory).unwrap()
}

fn replay_model_trace(directory: &Path, trace: &Value) -> ReplayCounts {
    let mut store = initial_model_store(directory);
    let mut counts = ReplayCounts::default();
    let steps = trace["steps"].as_array().unwrap();
    for (index, step) in steps.iter().enumerate() {
        let event = step["event"].as_str().unwrap();
        let before = matches!(event, "restart" | "view").then(|| complete_snapshot(&store));
        if event == "restart" {
            model_database::mark_restart(&store.connection().unwrap()).unwrap();
            drop(store);
            store = Store::open(directory, false).unwrap();
            assert!(!model_database::restart_marker_exists(&store.connection().unwrap()).unwrap());
        } else {
            model_step(&store, event, 1000.0 + 10.0 * (index + 1) as f64);
        }
        assert_eq!(
            flow_snapshot(&store),
            step["expected"],
            "trace={trace}, step={index}"
        );
        if let Some(before) = before {
            assert_eq!(complete_snapshot(&store), before, "{event}");
        }
        counts.record(event);
    }
    counts.traces = 1;
    counts
}

mod state_contract_tests {
    use super::*;

    #[test]
    fn test_state_names_and_display_order() {
        let contract: Value =
            serde_json::from_slice(&std::fs::read(acceptance_path("requirements.json")).unwrap())
                .unwrap();
        let exported = python_model();
        assert_eq!(contract["contract_id"], "codex-session-board/acceptance-v1");
        assert_eq!(contract["requirements"].as_array().unwrap().len(), 31);
        assert_eq!(contract["states"], exported["states"]);
        assert_eq!(serde_json::to_value(STATES).unwrap(), exported["states"]);
    }

    #[test]
    fn frozen_model_and_requirements_match_locked_baseline() {
        let lock: Value =
            serde_json::from_slice(&std::fs::read(acceptance_path("contract.lock.json")).unwrap())
                .unwrap();
        for name in ["model.py", "requirements.json"] {
            let bytes = std::fs::read(acceptance_path(name)).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                lock["files"][name],
                "{name}"
            );
        }
    }

    #[test]
    fn test_runtime_evidence_table() {
        let samples = [
            ("active", "inProgress", Some("in_progress")),
            ("systemError", "inProgress", Some("blocked")),
            ("idle", "failed", Some("blocked")),
            ("idle", "interrupted", Some("blocked")),
            ("idle", "completed", Some("review")),
            ("notLoaded", "", None),
            ("active", "completed", None),
            ("idle", "inProgress", None),
        ];
        for (kind, phase, expected) in samples {
            let status = json!({"type": kind});
            let latest = json!({"status": phase});
            let actual = runtime_state(&status, &latest).map(|decision| decision.0);
            assert_eq!(actual, expected, "status={status}, turn={latest}");
            assert!(!matches!(actual, Some("completed" | "to_archive")));
        }
        for flag in ["waitingOnApproval", "waitingOnUserInput"] {
            let status = json!({"type": "active", "activeFlags": [flag]});
            let actual = runtime_state(&status, &json!({"status": "inProgress"}));
            assert_eq!(actual.unwrap().0, "blocked", "{flag}");
        }
    }

    #[test]
    fn test_all_abstract_state_edges_match_store() {
        let model = python_model();
        let traces = model["traces"].as_array().unwrap();
        assert_eq!(model["stateCount"], 40);
        assert_eq!(model["events"].as_array().unwrap().len(), 13);
        assert_eq!(traces.len(), 520);
        let origins: BTreeSet<_> = traces
            .iter()
            .map(|trace| trace["origin"].to_string())
            .collect();
        assert_eq!(origins.len(), 40);
        let directory = tempfile::tempdir().unwrap();
        let database = trace_database(directory.path());
        let mut counts = ReplayCounts::default();
        for origin in &origins {
            let expected_origin: Value = serde_json::from_str(origin).unwrap();
            let events: BTreeSet<_> = traces
                .iter()
                .filter(|trace| trace["origin"] == expected_origin)
                .map(|trace| trace["event"].as_str().unwrap())
                .collect();
            assert_eq!(events.len(), 13);
        }
        for trace in traces {
            database.reset().unwrap();
            counts += replay_model_trace(directory.path(), trace);
        }
        database.reset().unwrap();
        assert!(counts.steps >= 520);
        assert_eq!(counts, ReplayCounts { traces: 520, steps: 1417, restarts: 40, views: 40 });
        eprintln!("frozen model: 40 reachable states, 520 edges, {} actions compared", counts.steps);
    }

    #[test]
    fn model_trace_reset_clears_all_tables_and_rejects_changed_schema() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path(), true).unwrap();
        let database = TraceDatabase::capture(&store.connection().unwrap(), directory.path()).unwrap();
        model_database::mark_restart(&store.connection().unwrap()).unwrap();
        assert!(model_database::restart_marker_exists(&store.connection().unwrap()).unwrap());
        store.connection().unwrap().execute_batch(
            "INSERT INTO sessions(id,title,cwd,updated_at,seen_at) VALUES('noise','noise','',0,0);
             INSERT INTO changes(thread_id,changed_at,state,note) VALUES('noise',0,'review','noise');
             INSERT INTO title_updates(thread_id,version,state,requested_at) VALUES('noise',1,'review',0);
             UPDATE settings SET value='-12345' WHERE key='title_sync_started_at';
             INSERT INTO remote_sessions(id,host_id,thread_id,host_label,title,cwd,updated_at)
                 VALUES('noise','host','thread','host','noise','',0);
             INSERT INTO remote_projects(id,value) VALUES('noise','{}');"
        ).unwrap();
        assert!(complete_snapshot(&store).tables.values().all(|rows| !rows.is_empty()));
        drop(store);
        database.reset().unwrap();
        let store = Store::open(directory.path(), true).unwrap();
        assert!(!model_database::restart_marker_exists(&store.connection().unwrap()).unwrap());
        assert!(store.title_epoch.is_some());
        assert_ne!(store.title_epoch, Some(-12345.0));
        store.connection().unwrap().execute_batch(
            "CREATE TRIGGER noise AFTER UPDATE ON sessions
             BEGIN UPDATE sessions SET note='noise' WHERE id=new.id; END;"
        ).unwrap();
        let before = complete_snapshot(&store);
        drop(store);
        assert!(database.reset().unwrap_err().to_string().contains("schema"));
        let connection = rusqlite::Connection::open(directory.path().join("status.sqlite3")).unwrap();
        assert_eq!(model_database::snapshot(&connection).unwrap(), before);
    }
}

mod isolation_contract_tests {
    use super::*;

    fn two_chats() -> (TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path(), true).unwrap();
        store
            .discover(&[
                json!({"id": "first", "name": "第一会话", "updatedAt": 1}),
                json!({"id": "second", "name": "第二会话", "updatedAt": 1}),
            ])
            .unwrap();
        (directory, store)
    }

    #[test]
    fn test_view_and_single_chat_change_are_isolated() {
        let (_directory, store) = two_chats();
        let other_directory = tempfile::tempdir().unwrap();
        let other = Store::open(other_directory.path(), true).unwrap();
        other
            .discover(&[json!({"id": "first", "name": "另一数据库", "updatedAt": 1})])
            .unwrap();
        let before_other = database_snapshot(&other);
        let before_second = session_named(&store, "second");
        store
            .set_state("first", Some("to_archive"), "保留人工备注", 0, None)
            .unwrap();
        assert_eq!(session_named(&store, "second"), before_second);
        assert_eq!(database_snapshot(&other), before_other);
        let jobs = store.pending_titles(None).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["thread_id"], "first");
        assert!(store.title_synced("first", 1, "[待归档] 第一会话").unwrap());
        assert_eq!(session_named(&store, "second"), before_second);
        let before_view = database_snapshot(&store);
        let _rows = store.sessions().unwrap();
        assert_eq!(database_snapshot(&store), before_view);
        assert_eq!(
            audit(&store),
            vec![json!({
                "thread_id": "first", "state": "to_archive", "note": "保留人工备注",
            })]
        );
        assert_eq!(database_snapshot(&other), before_other);
    }

    #[test]
    fn test_title_queue_failure_rolls_back_state_and_audit() {
        let (_directory, store) = two_chats();
        reject_title_inserts(&store);
        let before = database_snapshot(&store);
        let error = store
            .set_state("first", Some("review"), "备注", 0, None)
            .unwrap_err();
        assert!(error.to_string().contains("injected title queue failure"));
        assert_eq!(database_snapshot(&store), before);
        assert!(store.connection().unwrap().is_autocommit());
        assert!(store.pending_titles(None).unwrap().is_empty());
        assert!(audit(&store).is_empty());
    }

    #[test]
    fn runtime_queue_failure_rolls_back_deduplication_state_and_audit() {
        let (_directory, store) = fixture(true);
        reject_title_inserts(&store);
        let before = database_snapshot(&store);
        let error = store
            .observe_runtime(
                "sample",
                &json!({"type": "active"}),
                &turn("t1", "inProgress"),
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("injected title queue failure"));
        assert_eq!(database_snapshot(&store), before);
        assert!(store.connection().unwrap().is_autocommit());
        store
            .connection()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_title_insert")
            .unwrap();
        assert!(runtime(&store, "inProgress", "t1", &[]));
        assert_eq!(session(&store)["version"], 1);
        assert_eq!(audit(&store).len(), 1);
    }

    #[test]
    fn title_queue_replacement_failure_keeps_previous_committed_job() {
        let (_directory, store) = fixture(true);
        select(&store, Some("review"), "先前备注");
        store.title_failed("sample", 1, "先前失败").unwrap();
        reject_title_inserts(&store);
        let before = database_snapshot(&store);
        assert!(
            store
                .set_state("sample", Some("completed"), "新备注", 1, None)
                .is_err(),
        );
        assert_eq!(database_snapshot(&store), before);
        assert!(store.title_current("sample", 1).unwrap());
    }

    #[test]
    fn committed_pending_title_survives_restart_without_a_callback() {
        let (directory, store) = two_chats();
        store
            .set_state("first", Some("to_archive"), "保留备注", 0, None)
            .unwrap();
        let before = database_snapshot(&store);
        drop(store);
        let store = Store::open(directory.path(), true).unwrap();
        assert_eq!(database_snapshot(&store), before);
        assert!(store.title_current("first", 1).unwrap());
        assert!(store.title_synced("first", 1, "[待归档] 第一会话").unwrap());
        assert!(store.pending_titles(None).unwrap().is_empty());
        let row = session_named(&store, "first");
        assert_eq!(row["state"], "to_archive");
        assert_eq!(row["note"], "保留备注");
        assert_eq!(row["title"], "[待归档] 第一会话");
    }

    #[test]
    fn test_disabled_title_sync_saves_without_native_writes() {
        let (_directory, store) = fixture(false);
        select(&store, Some("completed"), "保留备注");
        assert_eq!(session(&store)["state"], "completed");
        assert_eq!(session(&store)["note"], "保留备注");
        assert_eq!(session(&store)["title"], "示例会话");
        assert!(store.pending_titles(None).unwrap().is_empty());
        assert_eq!(store.title_epoch, None);
    }

    #[test]
    fn disabled_sync_clears_existing_jobs_on_next_state_save() {
        let (directory, store) = fixture(true);
        select(&store, Some("review"), "旧备注");
        drop(store);
        let store = Store::open(directory.path(), false).unwrap();
        select(&store, Some("completed"), "新备注");
        assert!(store.pending_titles(None).unwrap().is_empty());
        assert_eq!(session(&store)["state"], "completed");
    }

    #[test]
    fn discovery_batch_failure_does_not_leave_partial_inserts() {
        let (_directory, store) = fixture(false);
        let before = database_snapshot(&store);
        assert!(
            store
                .discover(&[thread("new", 20), json!({"name": "缺少 ID"})])
                .is_err(),
        );
        assert_eq!(database_snapshot(&store), before);
    }
}

mod title_store_tests {
    use super::*;

    #[test]
    fn failure_keeps_saved_state_and_pending_work_survives_restart() {
        let (directory, store) = fixture(true);
        select(&store, Some("completed"), "保留备注");
        store.title_failed("sample", 1, "连接超时").unwrap();
        let row = session(&store);
        assert_eq!(row["state"], "completed");
        assert_eq!(row["note"], "保留备注");
        assert_eq!(row["titleSyncPending"], true);
        assert_eq!(row["titleSyncError"], "连接超时");
        let jobs = store.pending_titles(None).unwrap();
        drop(store);
        let store = Store::open(directory.path(), true).unwrap();
        assert_eq!(store.pending_titles(None).unwrap(), jobs);
        assert!(
            store
                .title_synced("sample", 1, "[已完成] 原始标题")
                .unwrap()
        );
        assert_eq!(session(&store)["titleSyncPending"], false);
        assert!(session(&store)["titleSyncError"].is_null());
    }

    #[test]
    fn old_ack_and_old_failure_cannot_delete_or_damage_a_newer_job() {
        let (_directory, store) = fixture(true);
        select(&store, Some("in_progress"), "保留备注");
        assert!(store.title_current("sample", 1).unwrap());
        select(&store, Some("review"), "保留备注");
        let before = database_snapshot(&store);
        assert!(!store.title_current("sample", 1).unwrap());
        assert!(store.title_current("sample", 2).unwrap());
        assert!(!store.title_synced("sample", 1, "[进行中] 旧标题").unwrap());
        store.title_failed("sample", 1, "旧错误").unwrap();
        assert_eq!(database_snapshot(&store), before);
        assert!(store.title_synced("sample", 2, "[待审阅] 新标题").unwrap());
        assert_eq!(session(&store)["title"], "[待审阅] 新标题");
        let before = database_snapshot(&store);
        assert!(!store.title_synced("sample", 2, "重复回执").unwrap());
        assert_eq!(database_snapshot(&store), before);
    }

    #[test]
    fn runtime_changes_queue_titles_but_duplicate_stages_do_not() {
        let (_directory, store) = fixture(true);
        for (flags, state) in [
            (vec![], "in_progress"),
            (vec!["waitingOnApproval"], "blocked"),
            (vec![], "in_progress"),
        ] {
            assert!(runtime(&store, "inProgress", "t1", &flags));
            let row = session(&store);
            let jobs = store.pending_titles(None).unwrap();
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0]["state"], state);
            assert_eq!(jobs[0]["version"], row["version"]);
            assert!(
                store
                    .title_synced("sample", integer(&row, "version"), "标题")
                    .unwrap()
            );
        }
        let before = database_snapshot(&store);
        assert!(!runtime(&store, "inProgress", "t1", &[]));
        assert_eq!(database_snapshot(&store), before);
        assert!(runtime(&store, "completed", "t1", &[]));
        assert_eq!(store.pending_titles(None).unwrap()[0]["state"], "review");
    }

    #[test]
    fn discovery_history_and_initial_old_completion_do_not_bulk_rename() {
        let (_directory, store) = fixture(true);
        store
            .discover(&[thread("sample", 10), thread("another", 10)])
            .unwrap();
        observe(&store, &turn("t1", "completed"));
        assert!(
            !store
                .observe_runtime(
                    "sample",
                    &json!({"type": "idle"}),
                    &turn("t1", "completed"),
                    Some(now()),
                )
                .unwrap()
        );
        assert!(store.pending_titles(None).unwrap().is_empty());
        assert_eq!(session(&store)["title"], "示例会话");
    }

    #[test]
    fn late_old_turn_change_does_not_rename_when_title_sync_is_first_enabled() {
        let (_directory, store) = fixture(true);
        for identifier in ["old-cached", "old-latest"] {
            let mut latest = turn(identifier, "completed");
            latest["completedAt"] = json!(store.title_epoch.unwrap() - 1.0);
            assert!(
                !store
                    .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
                    .unwrap()
            );
        }
        assert!(store.pending_titles(None).unwrap().is_empty());
        assert_eq!(session(&store)["title"], "示例会话");
    }

    #[test]
    fn old_terminal_without_completed_at_uses_started_at_title_epoch() {
        for phase in ["completed", "failed", "interrupted"] {
            let (_directory, store) = fixture(true);
            let epoch = store.title_epoch.unwrap();
            for identifier in ["old-cached", "old-latest"] {
                let mut latest = turn(identifier, phase);
                latest["startedAt"] = json!(epoch - 1.0);
                assert!(
                    !store
                        .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
                        .unwrap()
                );
                assert!(store.pending_titles(None).unwrap().is_empty(), "{phase}");
            }
            assert_eq!(session(&store)["title"], "示例会话");
        }
    }

    #[test]
    fn title_activation_epoch_survives_restart_and_catches_new_completed_turn() {
        let (directory, store) = fixture(true);
        let epoch = store.title_epoch.unwrap();
        let mut latest = turn("old", "completed");
        latest["completedAt"] = json!(epoch - 1.0);
        store
            .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
            .unwrap();
        drop(store);
        let store = Store::open(directory.path(), true).unwrap();
        assert_eq!(store.title_epoch, Some(epoch));
        latest["id"] = json!("new");
        latest["completedAt"] = json!(epoch + 1.0);
        assert!(
            store
                .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
                .unwrap()
        );
        assert_eq!(store.pending_titles(None).unwrap()[0]["state"], "review");
    }

    #[test]
    fn enabling_sync_after_disabled_use_does_not_queue_existing_history() {
        let (directory, store) = fixture(false);
        select(&store, Some("completed"), "已有人工选择");
        let before = session(&store);
        drop(store);
        let store = Store::open(directory.path(), true).unwrap();
        assert!(store.title_epoch.is_some());
        assert!(store.pending_titles(None).unwrap().is_empty());
        assert_eq!(session(&store), before);
    }

    #[test]
    fn existing_pending_job_is_refreshed_even_for_old_terminal_snapshot() {
        let (_directory, store) = fixture(true);
        let epoch = store.title_epoch.unwrap();
        runtime(&store, "inProgress", "t1", &[]);
        let mut latest = turn("t1", "completed");
        latest["completedAt"] = json!(epoch - 1.0);
        assert!(
            store
                .observe_runtime("sample", &json!({"type": "idle"}), &latest, None)
                .unwrap()
        );
        let jobs = store.pending_titles(None).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["state"], "review");
        assert_eq!(jobs[0]["version"], session(&store)["version"]);
    }

    #[test]
    fn failed_jobs_rotate_behind_unattempted_jobs_and_filter_by_identifier() {
        let (_directory, store) = fixture(true);
        store.discover(&[thread("another", 10)]).unwrap();
        select(&store, Some("review"), "");
        store
            .set_state("another", Some("completed"), "", 0, None)
            .unwrap();
        store.title_failed("sample", 1, "离线").unwrap();
        assert_eq!(
            store.pending_titles(None).unwrap()[0]["thread_id"],
            "another"
        );
        let selected = store.pending_titles(Some("sample")).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0]["thread_id"], "sample");
        assert!(store.pending_titles(Some("missing")).unwrap().is_empty());
        store.title_failed("another", 1, "离线").unwrap();
        assert_eq!(
            store.pending_titles(None).unwrap()[0]["thread_id"],
            "sample"
        );
    }

    #[test]
    fn newer_state_resets_failed_job_attempt_and_error() {
        let (_directory, store) = fixture(true);
        select(&store, Some("review"), "");
        store.title_failed("sample", 1, &"错".repeat(501)).unwrap();
        assert_eq!(
            session(&store)["titleSyncError"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            500,
        );
        select(&store, Some("completed"), "");
        let jobs = store.pending_titles(None).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["version"], 2);
        assert_eq!(jobs[0]["state"], "completed");
        assert!(jobs[0]["error"].is_null());
        assert!(jobs[0]["attempted_at"].is_null());
        assert_eq!(session(&store)["titleSyncPending"], true);
        assert!(session(&store)["titleSyncError"].is_null());
    }
}
