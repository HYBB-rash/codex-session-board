use super::*;
use codex_session_board::{app::App, config::Config};

fn app(directory: &std::path::Path) -> Result<App> {
    App::new(Config {
        data_dir: directory.join("data"),
        native_sections: false,
        sync_titles: true,
        open_command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf '%s\\n' \"$1\" >> \"$0\"".into(),
            directory.join("opens").to_string_lossy().into(),
        ],
        ..Config::default()
    })
}

#[test]
fn remote_routes_bypass_local_authority_and_preserve_host_project_identity() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    app.discover(&[], true)?;
    for host in ["mac & other", "other"] {
        app.store.discover_remote(
            host,
            host,
            &[json!({
                "id":THREAD,"hostId":host,"title":"同名","projectId":"same","updatedAt":1
            })],
        )?;
    }
    app.store.cache_remote_projects(&[json!({
        "id":identity("mac & other","same"),"label":"同名 · Mac","paths":["/remote"],
        "hostId":"mac & other","hostLabel":"Mac","remote":true
    })])?;
    let snapshot = app.snapshot()?;
    assert_eq!(snapshot["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(
        snapshot["projectCatalog"]["remoteHosts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    check_host(&app, "mac & other")?;
    check_host(&app, "other")?;
    assert!(app.store.pending_titles(None)?.is_empty());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("opens"))?
            .lines()
            .count(),
        2
    );
    Ok(())
}

fn check_host(app: &App, host: &str) -> Result<()> {
    let identifier = identity(host, THREAD);
    let response = app.request(
        "/api/state",
        Some(&json!({
            "id":identifier,"state":"review","note":host,"version":0,"inputVersion":0
        })),
    )?;
    assert_eq!(response["titleEnabled"], false);
    let opened = app.request("/api/open", Some(&json!({"id":identifier})))?;
    let url = url::Url::parse(opened["url"].as_str().unwrap())?;
    assert_eq!(url.path(), format!("/{THREAD}"));
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "hostId")
            .unwrap()
            .1,
        host
    );
    Ok(())
}

#[test]
fn unsupported_remote_actions_and_unknown_ids_fail_before_external_io() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    discover(&app.store, "mac", 1)?;
    let identifier = identity("mac", THREAD);
    assert!(
        app.request(
            "/api/archive",
            Some(&json!({
                "id":identifier,"version":0,"inputVersion":0
            }))
        )
        .unwrap_err()
        .to_string()
        .contains("远程会话")
    );
    assert!(
        app.request(
            "/api/new",
            Some(&json!({"projectId":identity("mac","project")}))
        )
        .is_err()
    );
    assert!(
        app.request("/api/open", Some(&json!({"id":identity("unknown",THREAD)})))
            .is_err()
    );
    assert!(!directory.path().join("opens").exists());
    app.request(
        "/api/state",
        Some(&json!({
            "id":identifier,"state":"to_archive","note":"稍后检查","version":0,"inputVersion":0
        })),
    )?;
    let row = &app.snapshot()?["sessions"][0];
    assert_eq!(row["state"], "to_archive");
    assert_eq!(row["canArchive"], false);
    assert_eq!(row["projectId"], serde_json::Value::Null);
    Ok(())
}
