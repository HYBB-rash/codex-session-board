#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use codex_session_board::{app::App, config::Config, rpc::Rpc};
use codex_session_board::{native_sections::Observation, store::Store};
use serde_json::json;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
#[path = "native_authority/runtime.rs"]
mod runtime;
#[path = "native_authority/transport.rs"]
mod transport;
#[path = "native_authority/archive.rs"]
mod archive;

struct NativeSurface {
    section: &'static str,
    fail: bool,
}

impl Rpc for NativeSurface {
    fn call(&mut self, method: &str, _: serde_json::Value) -> Result<serde_json::Value> {
        anyhow::ensure!(method == "list_threads", "read-only native surface");
        anyhow::ensure!(!self.fail, "native disconnected");
        Ok(json!({"unavailableHosts":[],"sections":[
            {"sectionId":"review-id","name":"待审阅","itemKeys":if self.section=="review-id"{vec![format!("codex:thread:local:{FIRST}")]}else{vec![]}},
            {"sectionId":"archive-id","name":"待归档","itemKeys":if self.section=="archive-id"{vec![format!("codex:thread:local:{FIRST}")]}else{vec![]}}
        ]}))
    }
}

fn native_app(directory: &std::path::Path) -> Result<App> {
    App::new(Config {
        data_dir: directory.into(),
        native_sections: true,
        sync_titles: false,
        status_sections: [
            ("review".into(), "review-id".into()),
            ("to_archive".into(), "archive-id".into()),
        ]
        .into(),
        ..Config::default()
    })
}

#[test]
fn app_external_moves_survive_history_failure_and_restart_without_correction() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    app.store
        .set_state(FIRST, Some("unknown"), "原有备注", 0, Some(0))?;
    let mut native = NativeSurface {
        section: "review-id",
        fail: false,
    };
    app.read_native(&mut native)?;
    assert_eq!(app.snapshot()?["sessions"][0]["state"], "review");
    native.section = "archive-id";
    app.read_native(&mut native)?;
    let old = json!({"id":"old","status":"completed","startedAt":1,"completedAt":2});
    app.observe_runtime(FIRST, &json!({"type":"idle"}), &old, 3.0)?;
    app.store
        .observe(FIRST, &old, Some("history disconnected"))?;
    native.fail = true;
    assert!(app.read_native(&mut native).is_err());
    assert_eq!(app.snapshot()?["sessions"][0]["state"], "to_archive");
    drop(app);
    let reopened = native_app(directory.path())?;
    assert_eq!(reopened.snapshot()?["sessions"][0]["state"], "to_archive");
    assert_eq!(reopened.store.note(FIRST)?, "原有备注");
    Ok(())
}

#[test]
fn native_endpoint_requires_position_and_validates_notes_before_remote_writes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    let mut native = NativeSurface {
        section: "review-id",
        fail: false,
    };
    app.read_native(&mut native)?;
    let mut body = json!({"id":FIRST,"state":"blocked","note":"","version":1,"inputVersion":0});
    assert!(
        app.request("/api/state", Some(&body))
            .unwrap_err()
            .to_string()
            .contains("缺少原生位置")
    );
    body["sectionVersion"] = json!("review-id");
    assert!(
        app.request("/api/state", Some(&body))
            .unwrap_err()
            .to_string()
            .contains("填写阻塞原因")
    );
    assert_eq!(app.store.note(FIRST)?, "");
    assert_eq!(app.snapshot()?["sessions"][0]["state"], "review");
    Ok(())
}

#[test]
fn native_cache_wins_over_local_choices_without_erasing_notes_or_confirmation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.discover(&[json!({"id":FIRST})])?;
    store.set_state(FIRST, Some("to_archive"), "保留备注", 0, Some(0))?;
    store.native_sections = true;
    assert_eq!(store.sessions()?[0]["state"], "unknown");
    store.cache_native(
        FIRST,
        &Observation {
            state: "review".into(),
            version: "native-review".into(),
        },
    )?;
    let session = &store.sessions()?[0];
    assert_eq!(session["state"], "review");
    assert_eq!(session["note"], "保留备注");
    assert_eq!(session["manual"], true);
    store.cache_native(
        FIRST,
        &Observation {
            state: "blocked".into(),
            version: "native-blocked".into(),
        },
    )?;
    drop(store);
    let mut reopened = Store::open(directory.path(), false)?;
    reopened.native_sections = true;
    assert_eq!(reopened.sessions()?[0]["state"], "blocked");
    assert_eq!(reopened.note(FIRST)?, "保留备注");
    Ok(())
}

#[test]
fn native_confirmations_commit_notes_and_reject_stale_or_invalid_edits() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path(), false)?;
    store.native_sections = true;
    store.discover(&[json!({"id":FIRST})])?;
    let before = Observation {
        state: "review".into(),
        version: "native-review".into(),
    };
    store.cache_native(FIRST, &before)?;
    assert!(
        store
            .validate_state(FIRST, Some("blocked"), "", (1, 0))
            .is_err()
    );
    assert!(
        store
            .validate_state(FIRST, Some("review"), "备注", (0, 0))
            .is_err()
    );
    assert!(
        store
            .validate_state(FIRST, Some("review"), "备注", (1, 1))
            .is_err()
    );
    let after = Observation {
        state: "to_archive".into(),
        version: "native-archive".into(),
    };
    store.set_native_state(FIRST, Some("to_archive"), "已确认", (1, 0), &after)?;
    assert_eq!(store.sessions()?[0]["state"], "to_archive");
    assert_eq!(store.sessions()?[0]["sectionVersion"], "native-archive");
    assert_eq!(store.note(FIRST)?, "已确认");
    assert!(
        store
            .set_native_state(FIRST, Some("review"), "过期", (1, 0), &before)
            .is_err()
    );
    assert_eq!(store.note(FIRST)?, "已确认");
    Ok(())
}
