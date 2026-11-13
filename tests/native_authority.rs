#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use codex_session_board::{app::App, config::Config, rpc::Rpc};
use codex_session_board::{native_sections::Observation, store::Store};
use serde_json::json;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
#[path = "native_authority/archive.rs"]
mod archive;
#[path = "native_authority/auto_archive.rs"]
mod auto_archive;
#[path = "native_authority/entrypoints.rs"]
mod entrypoints;
#[path = "native_authority/faults.rs"]
mod faults;
#[path = "native_authority/remote_isolation.rs"]
mod remote_isolation;
#[path = "native_authority/runtime.rs"]
mod runtime;
#[path = "native_authority/transport.rs"]
mod transport;

struct NativeSurface {
    section: &'static str,
    fail: bool,
}

impl Rpc for NativeSurface {
    fn call(&mut self, method: &str, _: serde_json::Value) -> Result<serde_json::Value> {
        anyhow::ensure!(!self.fail, "native disconnected");
        let sections = json!([
            {"id":"backend:review-id","name":"待审阅","appearance":null},
            {"id":"backend:archive-id","name":"待归档","appearance":null}
        ]);
        match method {
            "threadSection/list" => Ok(json!({"data":sections})),
            "thread/list" => Ok(json!({"data":[{"id":FIRST,"section":
                sections.as_array().unwrap().iter().find(|section|
                    section["id"] == format!("backend:{}",self.section)).unwrap()
            }]})),
            _ => anyhow::bail!("unexpected native method"),
        }
    }
}

fn native_app(directory: &std::path::Path) -> Result<App> {
    let mapping = directory.join("mapping.json");
    std::fs::write(
        &mapping,
        json!({"electron-persisted-atom-state":{
            "sidebar-custom-sections-v3":{"account":{"sections":[
                {"id":"review-id","hostSectionIds":{"local":"backend:review-id"}},
                {"id":"archive-id","hostSectionIds":{"local":"backend:archive-id"}}
            ]}}
        }})
        .to_string(),
    )?;
    App::new(Config {
        data_dir: directory.into(),
        native_sections: true,
        sync_titles: false,
        sidebar_state_file: mapping,
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
    body["note"] = json!("字".repeat(2001));
    assert!(
        app.request("/api/state", Some(&body))
            .unwrap_err()
            .to_string()
            .contains("2000")
    );
    assert_eq!(app.store.note(FIRST)?, "");
    assert_eq!(app.snapshot()?["sessions"][0]["state"], "review");
    Ok(())
}

#[test]
fn native_inactive_cached_chats_are_hidden_and_cannot_be_moved() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    let mut native = NativeSurface {
        section: "review-id",
        fail: false,
    };
    app.read_native(&mut native)?;
    app.discover(&[], true)?;
    assert!(app.snapshot()?["sessions"].as_array().unwrap().is_empty());
    let request = json!({"id":FIRST,"state":"to_archive","note":"不应保存","version":1,"inputVersion":0,"sectionVersion":"review-id"});
    assert!(
        app.request("/api/state", Some(&request))
            .unwrap_err()
            .to_string()
            .contains("等待完整原生会话发现")
    );
    assert_eq!(app.store.note(FIRST)?, "");
    Ok(())
}

#[test]
fn missing_direct_snapshot_entries_never_become_unsectioned() -> Result<()> {
    struct Missing(NativeSurface);
    impl Rpc for Missing {
        fn call(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
            if method == "thread/list" {
                return Ok(json!({"data":[]}));
            }
            self.0.call(method, params)
        }
    }
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    app.read_native(&mut NativeSurface {
        section: "review-id",
        fail: false,
    })?;
    app.read_native(&mut Missing(NativeSurface {
        section: "review-id",
        fail: false,
    }))?;
    let snapshot = app.snapshot()?;
    assert_eq!(snapshot["sessions"][0]["sectionVersion"], "review-id");
    assert_eq!(snapshot["sessions"][0]["state"], "review");
    assert!(
        snapshot["health"]["nativeError"]
            .as_str()
            .unwrap()
            .contains("保留")
    );
    Ok(())
}

#[test]
fn native_archive_list_absence_keeps_position_without_an_unread_error() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    let mut surface = Listed { active: true };
    app.read_native(&mut surface)?;
    let cached = app.snapshot()?;
    surface.active = false;
    app.read_native(&mut surface)?;
    let again = app.snapshot()?;
    assert_eq!(again["sessions"][0]["state"], cached["sessions"][0]["state"]);
    assert_eq!(
        again["sessions"][0]["sectionVersion"],
        cached["sessions"][0]["sectionVersion"]
    );
    assert!(again["health"]["nativeError"].is_null());
    Ok(())
}

struct Listed {
    active: bool,
}

impl Rpc for Listed {
    fn call(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let sections = json!([
            {"id":"backend:review-id","name":"待审阅","appearance":null},
            {"id":"backend:archive-id","name":"待归档","appearance":null}
        ]);
        match method {
            "threadSection/list" => Ok(json!({"data":sections})),
            "thread/list" if params["archived"].as_bool() == Some(true) => {
                Ok(json!({"data":[{"id":FIRST}]}))
            }
            "thread/list" if self.active => Ok(json!({"data":[{"id":FIRST,"section":
                sections.as_array().unwrap().iter().find(|section|
                    section["id"] == "backend:review-id").unwrap()
            }]})),
            "thread/list" => Ok(json!({"data":[]})),
            _ => anyhow::bail!("unexpected native method"),
        }
    }
}

#[test]
fn native_unarchive_restores_the_confirmed_card_and_keeps_the_note() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = native_app(directory.path())?;
    app.discover(&[json!({"id":FIRST})], true)?;
    let mut native = NativeSurface {
        section: "archive-id",
        fail: false,
    };
    app.read_native(&mut native)?;
    let (version, input, _) = session_clocks(&app.store)?;
    app.store.set_native_state(
        FIRST,
        Some("to_archive"),
        "留下备注",
        (version, input),
        &Observation {
            state: "to_archive".into(),
            version: "archive-id".into(),
        },
    )?;
    let (version, input, choice) = session_clocks(&app.store)?;
    app.store.confirm_archive(FIRST, (version, input))?;
    assert!(app.snapshot()?["sessions"].as_array().unwrap().is_empty());
    app.read_native(&mut native)?;
    let restored = &app.snapshot()?["sessions"][0];
    assert_eq!(restored["note"], "留下备注");
    assert_eq!(restored["state"], "to_archive");
    assert_eq!(session_clocks(&app.store)?.2, choice);
    Ok(())
}

fn session_clocks(store: &Store) -> Result<(i64, i64, Option<f64>)> {
    Ok(store.connection()?.query_row(
        "SELECT version,input_revision,native_choice_at FROM sessions WHERE id=?",
        [FIRST],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
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
            .is_ok()
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
