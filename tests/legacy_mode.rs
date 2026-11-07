use anyhow::{Context, Result};
use codex_session_board::{
    app::App,
    config::{Config, config_id},
    model::STATES,
};
use serde_json::json;

fn config(directory: &std::path::Path) -> Config {
    Config {
        data_dir: directory.join("data"),
        legacy_mode: true,
        desktop_socket: directory.join("unavailable.sock"),
        ..Config::default()
    }
}

fn state_keys(snapshot: &serde_json::Value) -> Result<Vec<&str>> {
    Ok(snapshot["states"]
        .as_object()
        .context("缺少状态列")?
        .keys()
        .map(String::as_str)
        .collect())
}

#[test]
fn legacy_six_states_preserve_completed_notes_titles_and_sorting_in_the_current_store() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let config = config(directory.path());
    let app = App::new(config.clone())?;
    let identifier = uuid::Uuid::new_v4().to_string();
    app.store
        .discover(&[json!({"id":identifier,"name":"完整正文","updatedAt":1})])?;
    app.request(
        "/api/state",
        Some(&json!({"id":identifier,"state":"completed",
        "note":"保留备注","version":0,"inputVersion":0})),
    )?;
    app.request(
        "/api/column-sort",
        Some(&json!({"state":"completed","field":"updated_at",
        "direction":"asc","version":0})),
    )?;
    drop(app);
    let app = App::new(config)?;
    let snapshot = app.snapshot()?;
    assert_eq!(
        state_keys(&snapshot)?,
        STATES.iter().map(|state| state.0).collect::<Vec<_>>()
    );
    assert_eq!(snapshot["sessions"][0]["state"], "completed");
    assert_eq!(snapshot["sessions"][0]["note"], "保留备注");
    assert_eq!(snapshot["columnSorts"]["completed"]["direction"], "asc");
    assert_eq!(app.store.pending_titles(None)?[0]["state"], "completed");
    assert!(snapshot["legacyMode"].as_bool().unwrap());
    assert!(
        app.request(
            "/api/archive",
            Some(&json!({"id":identifier,
        "version":1,"inputVersion":0}))
        )
        .is_err()
    );
    assert_eq!(app.store.sessions()?.len(), 1);
    Ok(())
}

#[test]
fn legacy_mode_rejects_native_connections_and_timers_before_creating_data() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let base = config(directory.path());
    let mut modes = Vec::new();
    for feature in 0..6 {
        let mut changed = base.clone();
        match feature {
            0 => changed.native_sections = true,
            1 => changed.auto_archive_seconds = Some(1),
            2 => changed.proxy_command = vec!["must-not-run".into()],
            3 => changed.clock_command = vec!["must-not-run".into()],
            4 => changed.sidebar_socket = directory.path().join("must-not-connect.sock"),
            _ => changed.host_id = "remote:other".into(),
        }
        modes.push(changed);
    }
    for changed in modes {
        assert!(changed.validate_modes().is_err());
        assert!(App::new(changed).is_err());
        assert!(!base.data_dir.exists());
    }
    let mut modern = base.clone();
    modern.legacy_mode = false;
    assert_ne!(config_id(&base)?, config_id(&modern)?);
    assert_eq!(
        App::new(modern)?.snapshot()?["states"]
            .as_object()
            .unwrap()
            .len(),
        5
    );
    Ok(())
}

#[test]
fn compatibility_tools_and_direct_routes_keep_archive_and_remote_creation_disabled() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(config(directory.path()))?;
    let tools = codex_session_board::mcp::tools_for(true);
    let entries = tools["tools"].as_array().context("缺少工具清单")?;
    assert!(!entries.iter().any(|tool| matches!(tool["name"].as_str(),
        Some("board_archive" | "board_new_reset"))));
    let sort = entries.iter().find(|tool| tool["name"] == "board_sort").context("缺少排序")?;
    assert!(sort["inputSchema"]["properties"]["state"]["enum"]
        .as_array().context("缺少状态枚举")?.contains(&json!("completed")));
    let modern = codex_session_board::mcp::tools();
    assert!(modern["tools"].as_array().context("缺少工具")?
        .iter().any(|tool| tool["name"] == "board_archive"));
    for path in ["/api/archive", "/api/new/reset"] {
        assert!(app.request(path, Some(&json!({}))).is_err());
    }
    assert!(app.request("/api/new",
        Some(&json!({"projectId":"remote:test","requestId":uuid::Uuid::new_v4()}))).is_err());
    assert!(app.store.pending_drafts()?.is_empty());
    Ok(())
}
