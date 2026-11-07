use super::*;
use crate::{config::Config, rpc::identity_fixture::Fixture};
use std::sync::atomic::Ordering;

const THREAD: &str = "11111111-1111-4111-8111-111111111111";

fn cached(fixture: &Fixture, id: &str, archived: bool) -> Result<Arc<App>> {
    let mut host = fixture.host.clone();
    host.host_id = id.into();
    hosts::source::Binding::prepare(&host, &fixture.config)?;
    let app = Arc::new(App::new(Config {
        data_dir: host.directory(&fixture.config),
        host_id: id.into(),
        sync_titles: false,
        ..Config::default()
    })?);
    app.store
        .discover(&[json!({"id":THREAD,"name":"新标题","updatedAt":10})])?;
    app.store
        .set_state(THREAD, Some("to_archive"), id, 0, Some(0))?;
    if archived {
        app.store.archive(THREAD, 1, 0, |_| Ok(()))?;
    }
    app.deactivate()?;
    Ok(app)
}

fn legacy(root: &App, host: &str) -> Result<()> {
    root.store.discover_remote(
        host,
        host,
        &[json!({"id":THREAD,"hostId":host,"title":"旧标题","updatedAt":1})],
    )?;
    root.store.save_remote(&json!({
        "id":identity(host,THREAD),"state":"review","note":"旧备注",
        "version":0,"inputVersion":0
    }))?;
    Ok(())
}

#[test]
fn cold_cache_restores_removed_hosts_latest_notes_and_archive_tombstones_without_ssh() -> Result<()>
{
    let fixture = Fixture::new()?;
    let first = cached(&fixture, "remote:first", true)?;
    let second = cached(&fixture, "remote:second", false)?;
    let root = App::new(fixture.config.clone())?;
    for host in ["remote:first", "remote:second"] {
        legacy(&root, host)?;
    }
    drop(root);
    let before = fixture.events();
    let reopened = App::new(fixture.config.clone())?;
    let snapshot = reopened.snapshot()?;
    let rows = snapshot["sessions"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["hostId"], "remote:second");
    assert_eq!(rows[0]["note"], "remote:second");
    assert_eq!(rows[0]["title"], "新标题");
    assert_eq!(rows[0]["readOnly"], true);
    assert_eq!(rows[0]["canArchive"], false);
    assert_eq!(fixture.events(), before);
    assert!(
        reopened
            .request(
                "/api/open",
                Some(&json!({"id":identity("remote:second",THREAD)}))
            )
            .is_err()
    );
    assert!(first.store.has_session(THREAD)?);
    assert!(!first.store.contains(THREAD)?);
    assert_eq!(second.store.note(THREAD)?, "remote:second");
    Ok(())
}

#[test]
fn invalid_cached_source_does_not_hide_another_host_or_overwrite_files() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = cached(&fixture, "remote:first", false)?;
    let second = cached(&fixture, "remote:second", false)?;
    let source = first.config.data_dir.join("source.json");
    std::fs::write(&source, "{broken")?;
    let before = fixture.events();
    let root = App::new(fixture.config.clone())?;
    let snapshot = root.snapshot()?;
    assert_eq!(snapshot["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["sessions"][0]["hostId"], "remote:second");
    assert_eq!(
        snapshot["health"]["remoteCacheErrors"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(std::fs::read_to_string(source)?, "{broken");
    assert_eq!(first.store.note(THREAD)?, "remote:first");
    assert_eq!(second.store.note(THREAD)?, "remote:second");
    assert_eq!(fixture.events(), before);
    Ok(())
}

#[test]
fn unreadable_host_database_does_not_break_other_hosts_and_recovers_without_reclassification() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = cached(&fixture, "remote:first", false)?;
    let _second = cached(&fixture, "remote:second", false)?;
    let root = App::new(fixture.config.clone())?;
    assert_eq!(root.snapshot()?["sessions"].as_array().unwrap().len(), 2);
    first.store.connection()?.execute("ALTER TABLE sessions RENAME TO unavailable_sessions", [])?;
    let snapshot = root.snapshot()?;
    assert_eq!(snapshot["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["sessions"][0]["hostId"], "remote:second");
    assert!(snapshot["health"]["remoteConnections"].as_array().unwrap()
        .iter().any(|health| health["hostId"] == "remote:first"
            && health["runtimeError"].is_string()));
    first.store.connection()?.execute("ALTER TABLE unavailable_sessions RENAME TO sessions", [])?;
    assert_eq!(root.snapshot()?["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(first.store.note(THREAD)?, "remote:first");
    Ok(())
}

fn registry(fixture: &Fixture) -> Result<Config> {
    let mut config = fixture.config.clone();
    config.sidebar_state_file = config.data_dir.with_file_name("registry.json");
    crate::config::write_json(
        &config.sidebar_state_file,
        &json!({
            "codex-managed-remote-connections":[{
                "hostId":fixture.host.host_id,"displayName":"test","alias":"example"
            }],
            "remote-connection-auto-connect-by-host-id":{(fixture.host.host_id.clone()):false}
        }),
        false,
    )?;
    Ok(config)
}

#[test]
fn explicit_connection_overrides_startup_preference_without_changing_desktop_configuration()
-> Result<()> {
    let fixture = Fixture::new()?;
    let config = registry(&fixture)?;
    let before = std::fs::read(&config.sidebar_state_file)?;
    let root = App::new(config)?;
    root.refresh_remote_instances()?;
    assert!(
        !fixture
            .events()
            .lines()
            .any(|line| matches!(line, "proxy" | "clock"))
    );
    assert_eq!(
        root.request(
            "/api/connect",
            Some(&json!({"hostId":fixture.host.host_id}))
        )?["ok"],
        true
    );
    let remote = root.remote_app(&fixture.host.host_id)?;
    assert!(remote.enabled.load(Ordering::SeqCst));
    remote
        .store
        .discover(&[json!({"id":THREAD,"name":"test"})])?;
    root.request(
        "/api/state",
        Some(&json!({
            "id":identity(&fixture.host.host_id, THREAD),"state":"review",
            "note":"显式连接后保存","version":0,"inputVersion":0
        })),
    )?;
    assert_eq!(remote.store.note(THREAD)?, "显式连接后保存");
    assert_eq!(std::fs::read(&root.config.sidebar_state_file)?, before);
    remote.deactivate()?;
    Ok(())
}

#[test]
fn retirement_rechecks_a_new_explicit_request_under_the_host_lock() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = Arc::new(App::new(fixture.config.clone())?);
    let remote = cached(&fixture, "remote:first", false)?;
    remote.enabled.store(true, Ordering::SeqCst);
    let operation = root.thread_lock("connect:remote:first")?;
    let guard = operation.lock().unwrap();
    let worker_root = Arc::clone(&root);
    let worker_remote = Arc::clone(&remote);
    let worker =
        std::thread::spawn(move || worker_root.retire_remote("remote:first", &worker_remote));
    root.remote_requested
        .lock()
        .unwrap()
        .insert("remote:first".into());
    drop(guard);
    worker.join().unwrap()?;
    assert!(remote.enabled.load(Ordering::SeqCst));
    root.remote_requested.lock().unwrap().clear();
    root.retire_remote("remote:first", &remote)?;
    assert!(!remote.enabled.load(Ordering::SeqCst));
    assert!(root.connect_remote(&json!({"hostId":"removed"})).is_err());
    assert!(root.operation().is_ok());
    Ok(())
}
