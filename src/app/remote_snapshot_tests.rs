use super::*;
use crate::native_sections::Observation;
use crate::store::remote::identity;

fn fixture() -> Result<(tempfile::TempDir, App, App, String)> {
    let directory = tempfile::tempdir()?;
    let root = App::new(Config {
        data_dir: directory.path().join("root"),
        native_sections: true,
        sync_titles: false,
        ..Config::default()
    })?;
    let remote = App::new(Config {
        data_dir: directory.path().join("remote"),
        host_id: "remote:a".into(),
        native_sections: true,
        sync_titles: false,
        ..Config::default()
    })?;
    let thread = uuid::Uuid::new_v4().to_string();
    remote
        .store
        .discover(&[json!({"id":thread,"name":"已保存的空会话","updatedAt":1})])?;
    root.store.discover_remote(
        "remote:a",
        "远程",
        &[json!({
            "id":thread,"hostId":"remote:a","title":"旧缓存","updatedAt":1,
        })],
    )?;
    remote.discover(&[], true)?;
    remote.update_health(json!({"runtimeVerified":true}))?;
    Ok((directory, root, remote, thread))
}

fn merged(root: &App, remote: &App) -> Result<Vec<Value>> {
    let mut rows = root.store.remote_sessions()?;
    let legacy = rows.clone();
    root.append_managed_host(remote, &mut rows, &legacy, &mut Vec::new())?;
    root.annotate_connections(&mut rows);
    Ok(rows)
}

fn creation_projects(root: &App, remote: &App) -> Result<Vec<Value>> {
    let mut projects = Vec::new();
    root.append_managed_host(remote, &mut Vec::new(), &[], &mut projects)?;
    Ok(projects)
}

#[test]
fn native_project_creation_requires_a_live_connected_non_deleting_target() -> Result<()> {
    let (directory, mut root, remote, _thread) = fixture()?;
    root.config.sidebar_state_file = directory.path().join("registry.json");
    std::fs::write(&root.config.sidebar_state_file, serde_json::to_vec(&json!({
        "codex-managed-remote-connections":[{"hostId":"remote:a","displayName":"Mac",
            "alias":"example"}]}))?)?;
    let project = json!({"id":"target:[\"remote:a\",\"targetServer\",\"p\"]",
        "nativeId":"p","hostId":"remote:a","origin":"targetServer","paths":["/target"],
        "canCreate":false,"canMove":false,"canOpenProject":false});
    remote.catalog.lock().unwrap().snapshot = Some(crate::catalog::Snapshot {
        source: json!({}),
        captured_at: crate::model::now(),
        projects: vec![project],
        active: Vec::new(),
        archived: Vec::new(),
        coverage: json!({"projects":"complete","ordinaryActive":"complete",
            "ordinaryArchived":"complete","publishedReferences":"complete",
            "membership":"complete","legacy":"complete"}),
        issues: Vec::new(),
    });
    assert_eq!(creation_projects(&root, &remote)?[0]["canCreate"], false);
    remote.catalog.lock().unwrap().live = true;
    assert_eq!(creation_projects(&root, &remote)?[0]["canCreate"], true);
    for scope in ["membership", "publishedReferences", "ordinaryArchived", "legacy"] {
        remote.catalog.lock().unwrap().snapshot.as_mut().unwrap().coverage[scope] = json!("partial");
        assert_eq!(creation_projects(&root, &remote)?[0]["canCreate"], true);
        assert_eq!(remote.catalog.lock().unwrap().health()["complete"], false);
    }
    remote.catalog.lock().unwrap().progress = Some(json!({"refreshing":true}));
    assert_eq!(creation_projects(&root, &remote)?[0]["canCreate"], true);
    {
        let mut catalog = remote.catalog.lock().unwrap();
        catalog.snapshot.as_mut().unwrap().projects[0]["pendingDeletion"] = json!(true);
    }
    assert_eq!(creation_projects(&root, &remote)?[0]["canCreate"], false);
    assert!(creation_projects(&root, &remote)?[0]["createError"].as_str().unwrap().contains("删除"));
    remote.catalog.lock().unwrap().snapshot.as_mut().unwrap()
        .projects[0]["pendingDeletion"] = json!(false);
    remote.deactivate()?;
    assert_eq!(creation_projects(&root, &remote)?[0]["canCreate"], false);
    Ok(())
}

#[test]
fn unpublished_empty_chat_survives_complete_scan_and_restart_as_read_only() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    remote
        .store
        .set_state(&thread, Some("review"), "保留的备注", 0, Some(0))?;
    let rows = merged(&root, &remote)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], identity("remote:a", &thread));
    assert_eq!(rows[0]["title"], "已保存的空会话");
    assert_eq!(rows[0]["note"], "保留的备注");
    assert_eq!(rows[0]["state"], "unknown");
    assert_eq!(rows[0]["inputVersion"], 0);
    assert_eq!(rows[0]["sectionVersion"], Value::Null);
    assert_eq!(rows[0]["readOnly"], true);
    assert_eq!(rows[0]["canArchive"], false);
    assert_eq!(rows[0]["presence"], "cachedUnknown");
    assert_eq!(rows[0]["catalogCurrent"], false);
    let remote_config = remote.config.clone();
    let root_config = root.config.clone();
    drop(remote);
    drop(root);
    let root = App::new(root_config)?;
    let remote = App::new(remote_config)?;
    remote.discover(&[], true)?;
    assert_eq!(merged(&root, &remote)?, rows);
    Ok(())
}

#[test]
fn published_chat_uses_confirmed_position_and_existing_missing_record_filter() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    remote.discover(
        &[json!({"id":thread,"name":"已保存的空会话","updatedAt":1})],
        true,
    )?;
    remote.store.cache_native(
        &thread,
        &Observation {
            state: "review".into(),
            version: "confirmed-section".into(),
        },
    )?;
    let rows = merged(&root, &remote)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "review");
    assert_eq!(rows[0]["sectionVersion"], "confirmed-section");
    assert_eq!(rows[0]["readOnly"], false);
    remote.discover(&[], true)?;
    let history = merged(&root, &remote)?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["state"], "review");
    assert_eq!(history[0]["presence"], "cachedUnknown");
    assert_eq!(history[0]["catalogCurrent"], false);
    assert_eq!(history[0]["readOnly"], true);
    assert!(remote.store.contains(&thread)?);
    Ok(())
}

#[test]
fn archived_unpublished_chat_never_reappears_from_the_root_cache() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    let legacy = Store::open(&remote.config.data_dir, false)?;
    legacy.set_state(&thread, Some("to_archive"), "", 0, Some(0))?;
    let row = &legacy.sessions()?[0];
    legacy.archive(&thread, row["version"].as_i64().unwrap(), 0, |_| Ok(()))?;
    let archived = merged(&root, &remote)?;
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0]["archived"], true);
    assert_eq!(archived[0]["catalogCurrent"], false);
    assert_eq!(archived[0]["canArchive"], false);
    remote.discover(
        &[json!({"id":thread,"name":"重复发现","updatedAt":2})],
        true,
    )?;
    let rediscovered = merged(&root, &remote)?;
    assert_eq!(rediscovered.len(), 1);
    assert_eq!(rediscovered[0]["archived"], true);
    assert_eq!(rediscovered[0]["readOnly"], true);
    assert_eq!(rediscovered[0]["catalogCurrent"], false);
    assert!(remote.store.has_session(&thread)?);
    assert!(!remote.store.contains(&thread)?);
    assert!(
        root.store
            .remote_session(&identity("remote:a", &thread))
            .is_ok()
    );
    Ok(())
}

#[test]
fn disabled_host_keeps_unpublished_cache_without_enabling_operations() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    remote.deactivate()?;
    let rows = merged(&root, &remote)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["threadId"], thread);
    assert_eq!(rows[0]["readOnly"], true);
    assert_eq!(rows[0]["canArchive"], false);
    assert!(remote.operation().is_err());
    Ok(())
}
