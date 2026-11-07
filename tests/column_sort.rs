use anyhow::Result;
use codex_session_board::{app::App, config::Config, mcp, store::Store};
use serde_json::json;

#[test]
fn column_sort_defaults_persistence_and_independent_conflicts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    let defaults = store.column_sorts()?;
    assert_eq!(defaults.as_object().map(|value| value.len()), Some(5));
    assert_eq!(defaults["to_archive"]["direction"], "asc");
    assert_eq!(defaults["review"]["direction"], "desc");
    let change = json!({"state":"review","field":"updated_at","direction":"asc","version":0});
    assert_eq!(store.save_column_sort(&change)?["preference"]["version"], 1);
    assert!(store.save_column_sort(&change).is_err());
    store.save_column_sort(
        &json!({"state":"unknown","field":"updated_at","direction":"asc","version":0}),
    )?;
    drop(store);
    let reopened = Store::open(directory.path(), true)?;
    assert_eq!(reopened.column_sorts()?["review"]["direction"], "asc");
    assert_eq!(reopened.column_sorts()?["unknown"]["version"], 1);
    assert_eq!(reopened.column_sorts()?["blocked"]["version"], 0);
    Ok(())
}

#[test]
fn column_sort_rejects_invalid_requests_without_touching_sessions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    store.discover(&[json!({"id":"session","name":"原任务","updatedAt":1})])?;
    let before = store.sessions()?;
    let defaults = store.column_sorts()?;
    let valid = json!({"state":"review","field":"updated_at","direction":"asc","version":0});
    for (key, value) in [
        ("state", json!("completed")),
        ("field", json!("priority")),
        ("direction", json!("reverse")),
        ("version", json!(-1)),
        ("version", json!(0.5)),
        ("version", json!("0")),
        ("extra", json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(store.save_column_sort(&invalid).is_err(), "{invalid}");
    }
    assert!(store.save_column_sort(&json!({})).is_err());
    assert_eq!(store.column_sorts()?, defaults);
    store.save_column_sort(&valid)?;
    assert_eq!(store.sessions()?, before);
    assert!(store.pending_titles(None)?.is_empty());
    Ok(())
}

#[test]
fn column_sort_app_and_mcp_expose_separate_settings_operation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        ..Config::default()
    })?;
    let change = json!({"state":"to_archive","field":"updated_at","direction":"desc","version":0});
    assert_eq!(app.request("/api/column-sort", Some(&change))?["ok"], true);
    assert_eq!(
        app.snapshot()?["columnSorts"]["to_archive"]["direction"],
        "desc"
    );
    let tool = mcp::tools()["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == "board_sort"))
        .cloned();
    assert!(tool.is_some());
    assert!(
        mcp::resource()["text"]
            .as_str()
            .is_some_and(|html| html.contains("\"/api/column-sort\":\"board_sort\""))
    );
    Ok(())
}
