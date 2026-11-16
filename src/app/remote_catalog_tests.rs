use super::*;
use std::sync::Arc;

fn fixture() -> Result<(tempfile::TempDir, App, Arc<App>, Arc<App>)> {
    let directory = tempfile::tempdir()?;
    let root = App::new(Config {
        data_dir: directory.path().join("root"),
        sync_titles: false,
        ..Config::default()
    })?;
    let first = Arc::new(App::new(Config {
        data_dir: directory.path().join("a"),
        host_id: "remote:a".into(),
        sync_titles: false,
        ..Config::default()
    })?);
    let second = Arc::new(App::new(Config {
        data_dir: directory.path().join("b"),
        host_id: "remote:b".into(),
        sync_titles: false,
        ..Config::default()
    })?);
    second.deactivate()?;
    for remote in [&first, &second] {
        remote.catalog_refresh.store(false, Ordering::SeqCst);
        root.remotes
            .lock()
            .unwrap()
            .insert(remote.config.host_id.clone(), remote.clone());
    }
    root.refresh.store(false, Ordering::SeqCst);
    root.remote_refresh.store(false, Ordering::SeqCst);
    Ok((directory, root, first, second))
}

#[test]
fn selected_host_refresh_never_connects_other_or_disabled_hosts() -> Result<()> {
    let (_directory, root, first, second) = fixture()?;
    let response = root.request("/api/refresh", Some(&json!({"hostId":"remote:a"})))?;
    assert_eq!(response["catalogScheduled"], json!(["remote:a"]));
    assert!(first.catalog_refresh.load(Ordering::SeqCst));
    assert!(!second.catalog_refresh.load(Ordering::SeqCst));
    assert!(!root.remote_refresh.load(Ordering::SeqCst));
    assert!(!root.refresh.load(Ordering::SeqCst));
    first.catalog_refresh.store(false, Ordering::SeqCst);
    for host in ["remote:b", "remote:unknown"] {
        assert_eq!(
            root.refresh_hosts(&json!({"hostId":host}))?["catalogScheduled"],
            json!([])
        );
    }
    root.refresh_hosts(&json!({"hostId":"local"}))?;
    assert!(root.refresh.load(Ordering::SeqCst));
    assert!(!root.remote_refresh.load(Ordering::SeqCst));
    assert!(!first.catalog_refresh.load(Ordering::SeqCst));
    assert!(!second.enabled.load(Ordering::SeqCst));
    assert!(root.remote_requested.lock().unwrap().is_empty());
    assert_eq!(root.remotes.lock().unwrap().len(), 2);
    Ok(())
}

#[test]
fn target_creation_requires_operation_and_v2_capability_before_remote_actions() -> Result<()> {
    let (_directory, root, _first, _second) = fixture()?;
    for origin in ["targetServer", "targetLegacy"] {
        let project = format!("target:{}", json!(["remote:a", origin, "native-project"]));
        let error = root
            .request("/api/new", Some(&json!({"projectId":project})))
            .unwrap_err();
        assert!(error.to_string().contains("缺少新建操作 ID"));
        let error = root
            .request("/api/new", Some(&json!({"projectId":project,
                "operationId":uuid::Uuid::new_v4().to_string()})))
            .unwrap_err();
        assert!(error.to_string().contains("尚未配置远程原生新建入口"));
    }
    assert!(root.store.pending_drafts()?.is_empty());
    assert!(root.remote_requested.lock().unwrap().is_empty());
    Ok(())
}

#[test]
fn target_directory_creation_rechecks_live_catalog_and_never_starts_a_thread() -> Result<()> {
    let (directory, mut root, first, _) = fixture()?;
    root.config.sidebar_state_file = directory.path().join("registry.json");
    std::fs::write(&root.config.sidebar_state_file, serde_json::to_vec(&json!({
        "codex-managed-remote-connections":[{"hostId":"remote:a","displayName":"Mac",
            "alias":"fixture-mac"}]}))?)?;
    root.config.open_command = vec!["true".into()];
    let project = format!("target:{}", json!(["remote:a","targetServer","canonical"]));
    {
        let mut state = first.catalog.lock().unwrap();
        state.live = true;
        state.snapshot = Some(crate::catalog::Snapshot {
            source: json!({"hostId":"remote:a"}), captured_at: now(),
            projects: vec![json!({"id":project,"nativeId":"canonical","hostId":"remote:a",
                "origin":"targetServer","paths":["/remote/project & 中文?#%+"]})],
            active: vec![], archived: vec![], issues: vec![],
            coverage: json!({"projects":"complete","ordinaryActive":"complete",
                "ordinaryArchived":"complete","publishedReferences":"complete",
                "membership":"complete","legacy":"complete"}),
        });
    }
    let body = json!({"projectId":project,"hostId":"remote:a"});
    let result = root.request("/api/new", Some(&body))?;
    let url = url::Url::parse(result["url"].as_str().unwrap())?;
    assert_eq!(url.query_pairs().count(), 4);
    assert_eq!(url.query_pairs().find(|pair| pair.0 == "projectPath").unwrap().1,
        "/remote/project & 中文?#%+");
    first.catalog.lock().unwrap().live = false;
    assert!(root.request("/api/new", Some(&body)).is_err());
    assert!(root.store.pending_drafts()?.is_empty());
    assert!(root.store.sessions()?.is_empty());
    Ok(())
}

#[test]
fn draft_receipts_are_scoped_to_their_remote_host_and_survive_completion() -> Result<()> {
    let (_directory, root, _first, _second) = fixture()?;
    let thread = uuid::Uuid::new_v4().to_string();
    let request = uuid::Uuid::new_v4().to_string();
    root.store
        .draft_begin(&identity("remote:a", "project"), &request)?;
    assert!(root.store.catalog_drafts("remote:a")?.is_empty());
    root.store
        .draft_update(&request, Some(&thread), "created")?;
    assert_eq!(root.store.catalog_drafts("remote:a")?.len(), 1);
    assert!(root.store.catalog_drafts("remote:b")?.is_empty());
    root.store.draft_update(&request, None, "complete")?;
    assert_eq!(
        root.store.catalog_drafts("remote:a")?[0]["threadId"],
        thread
    );
    assert!(root.store.pending_drafts()?.is_empty());
    Ok(())
}
