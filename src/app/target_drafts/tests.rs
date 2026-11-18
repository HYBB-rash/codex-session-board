use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use crate::rpc::identity_fixture::Fixture;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

const CANONICAL: &str = "019a8120-0000-7000-8000-000000000001";

struct NativeFixture {
    peer: Fixture,
    root: App,
}

fn client(config: &mut Config) -> Result<()> {
    let directory = config.data_dir.parent().context("fixture directory")?;
    let executable = directory.join("client");
    std::fs::write(&executable, "#!/bin/sh\nexit 0\n")?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
    config.open_command = vec![executable.to_string_lossy().into_owned()];
    Ok(())
}

fn responses(home: &str) -> Result<Value> {
    let owner = format!("{:x}", Sha256::digest(br#"["user","account"]"#));
    let state = json!({"electron-renderer-content-owner":owner,
        "local-projects":{"old":{"id":"old","name":"Project","rootPaths":["/project"]}},
        (format!("electron-owner-value:app-server-projects-migration-by-host:{owner}")):{
            (format!("local:{home}")):{"version":1,"projectsMigrated":false}}});
    let project = json!({"id":CANONICAL,"name":"Project","roots":[{"path":"/project"}],"metadata":{}});
    Ok(json!({"account/read":{"userId":"user","workspaceRouting":{"chatgptAccountId":"account"}},
        "fs/getMetadata":{"isFile":true,"isSymlink":false},
        "fs/readFile":{"dataBase64":STANDARD.encode(serde_json::to_vec(&state)?)},
        "project/read":{"project":project},"project/list":{"data":[],"nextCursor":null},
        "project/import":{"project":project,"created":true}}))
}

impl NativeFixture {
    fn new() -> Result<Self> {
        let peer = Fixture::new()?;
        let mut config = peer.config.clone();
        config.sidebar_state_file = config.data_dir.with_extension("registry.json");
        std::fs::write(&config.sidebar_state_file, serde_json::to_vec(&json!({
            "codex-managed-remote-connections":[{
                "hostId":"remote:test","displayName":"test","alias":"example"}],
            "remote-connection-auto-connect-by-host-id":{"remote:test":true},
            "remote-projects":[{"id":"client-project","hostId":"remote:test","remotePath":"/project"}]}))?)?;
        client(&mut config)?;
        peer.update("results", responses(&peer.peer()?.codex_home)?)?;
        peer.update("rpcEvents", json!(true))?;
        let root = App::new(config)?;
        let remote = Arc::new(peer.app()?);
        bind_catalog(&remote)?;
        root.remotes.lock().unwrap().insert("remote:test".into(), remote);
        Ok(Self { peer, root })
    }

    fn request(&self, origin: &str, operation: &str) -> Result<Value> {
        let identifier = if origin == "targetLegacy" { "old" } else { CANONICAL };
        let key = format!("target:{}", json!(["remote:test",origin,identifier]));
        let mut request = json!({"projectId":key,"operationId":operation});
        let result = self.root.request("/api/new", Some(&request))?;
        if result["consent"].is_object() {
            request["consent"] = result["consent"]["token"].clone();
            return self.root.request("/api/new", Some(&request));
        }
        Ok(result)
    }

    fn imports(&self) -> Vec<Value> {
        self.peer.events().lines().filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|event| event["method"] == "project/import").collect()
    }
}

fn bind_catalog(remote: &App) -> Result<()> {
    let reader = remote.connection(Duration::from_secs(3), Role::CatalogReader)?;
    let mut source = reader.catalog_source()?;
    source["hostId"] = json!("remote:test");
    source["accountFingerprint"] = json!(format!("{:x}", Sha256::digest(br#"["user","account"]"#)));
    remote.catalog.lock().unwrap().snapshot = Some(crate::catalog::Snapshot {
        source, captured_at: now(),
        projects: [("targetServer",CANONICAL),("targetLegacy","old")].into_iter()
            .map(|(origin, identifier)| json!({
                "id":format!("target:{}",json!(["remote:test",origin,identifier])),
                "hostId":"remote:test","origin":origin,"nativeId":identifier,"paths":["/project"]
            })).collect(),
        active: Vec::new(), archived: Vec::new(),
        coverage: json!({"projects":"complete","ordinaryActive":"complete",
            "ordinaryArchived":"complete","publishedReferences":"complete",
            "membership":"complete","legacy":"complete"}), issues: Vec::new(),
    });
    remote.catalog.lock().unwrap().live = true;
    Ok(())
}

#[test]
fn canonical_target_request_uses_verified_rpc_and_only_opens_native_input() -> Result<()> {
    let fixture = NativeFixture::new()?;
    let operation = uuid::Uuid::new_v4().to_string();
    let result = fixture.request("targetServer", &operation)?;
    assert_eq!(result["ok"], true);
    assert_eq!(result["target"]["projectId"], CANONICAL);
    assert_eq!(result["target"]["cwd"], "/project");
    let url = url::Url::parse(result["url"].as_str().context("url")?)?;
    let query: HashMap<_,_> = url.query_pairs().into_owned().collect();
    assert_eq!(url.host_str(), Some("settings"));
    assert_eq!(url.path(), "/connections/ssh/add");
    assert_eq!(query.len(), 4);
    assert_eq!(query["name"], "example");
    assert_eq!(query["projectPath"], "/project");
    assert_eq!(query["enabled"], "true");
    assert_eq!(query["activate"], "true");
    assert!(fixture.imports().is_empty());
    assert!(!fixture.peer.events().contains("thread/start"));
    assert!(!fixture.peer.events().contains("turn/start"));
    assert!(fixture.root.store.pending_drafts()?.is_empty());
    Ok(())
}

#[test]
fn legacy_target_rejects_a_moved_directory_before_import() -> Result<()> {
    let fixture = NativeFixture::new()?;
    let mut changed = responses(&fixture.peer.peer()?.codex_home)?;
    let encoded = changed["fs/readFile"]["dataBase64"].as_str().context("fixture state")?;
    let mut state: Value = serde_json::from_slice(&STANDARD.decode(encoded)?)?;
    state["local-projects"]["old"]["rootPaths"] = json!(["/different"]);
    changed["fs/readFile"]["dataBase64"] = json!(STANDARD.encode(serde_json::to_vec(&state)?));
    fixture.peer.update("results", changed)?;
    let operation = uuid::Uuid::new_v4().to_string();
    let error = fixture.request("targetLegacy", &operation).unwrap_err();
    assert!(error.to_string().contains("项目目录已改变"));
    assert!(fixture.imports().is_empty());
    Ok(())
}

#[test]
fn target_directory_creation_rechecks_live_catalog_and_never_starts_a_thread() -> Result<()> {
    let fixture = NativeFixture::new()?;
    let operation = uuid::Uuid::new_v4().to_string();
    missing_alias_leaves_no_operation_record(&fixture, &operation)?;
    let remote = fixture.root.remote_app("remote:test")?;
    remote.catalog.lock().unwrap().live = false;
    assert!(fixture.request("targetServer", &operation).is_err());
    remote.catalog.lock().unwrap().live = true;
    {
        let mut catalog = remote.catalog.lock().unwrap();
        let snapshot = catalog.snapshot.as_mut().unwrap();
        for scope in ["membership", "publishedReferences", "ordinaryArchived", "legacy"] {
            snapshot.coverage[scope] = json!("partial");
        }
        snapshot.issues.push("membership:unrelated:foreignMembership".into());
        catalog.progress = Some(json!({"refreshing":true}));
        assert_eq!(catalog.health()["complete"], false);
    }
    fixture.request("targetServer", &operation)?;
    remote.catalog.lock().unwrap().snapshot.as_mut().unwrap().coverage["projects"] = json!("partial");
    assert!(fixture.request("targetServer", &operation).is_err());
    remote.catalog.lock().unwrap().snapshot.as_mut().unwrap().coverage["projects"] = json!("complete");
    remote.catalog.lock().unwrap().snapshot.as_mut().unwrap().projects[0]["pendingDeletion"] = json!(true);
    assert!(fixture.request("targetServer", &operation).is_err());
    assert!(fixture.imports().is_empty());
    assert!(!fixture.peer.events().contains("thread/start"));
    assert!(!fixture.peer.events().contains("turn/start"));
    Ok(())
}

fn missing_alias_leaves_no_operation_record(fixture: &NativeFixture, operation: &str) -> Result<()> {
    let config = &fixture.root.config;
    let original = std::fs::read(&config.sidebar_state_file)?;
    let mut registry: Value = serde_json::from_slice(&original)?;
    let connection = &mut registry["codex-managed-remote-connections"][0];
    connection["alias"] = Value::Null;
    connection["hostname"] = json!("builder@studio.example.test");
    let registration = serde_json::to_vec(&registry)?;
    std::fs::write(&config.sidebar_state_file, &registration)?;
    let events = fixture.peer.events();
    let error = fixture.request("targetServer", operation).unwrap_err();
    assert!(error.to_string().contains("选择原主机和项目"));
    assert_eq!(fixture.peer.events(), events);
    assert_eq!(std::fs::read(&config.sidebar_state_file)?, registration);
    let database = rusqlite::Connection::open(config.data_dir.join("status.sqlite3"))?;
    let records: i64 = database.query_row(
        "SELECT count(*) FROM settings WHERE key = ?",
        [format!("native_draft_v1:{operation}")],
        |row| row.get(0),
    )?;
    assert_eq!(records, 0);
    std::fs::write(&config.sidebar_state_file, original)?;
    Ok(())
}

#[test]
fn lost_navigation_retry_reuses_prepared_target_without_reimport_or_thread_start() -> Result<()> {
    let fixture = NativeFixture::new()?;
    let remote = fixture.root.remote_app("remote:test")?;
    let canonical = remote.catalog.lock().unwrap().snapshot.as_mut().unwrap().projects.remove(0);
    let executable = &fixture.root.config.open_command[0];
    std::fs::write(executable, "#!/bin/sh\nexit 1\n")?;
    let operation = uuid::Uuid::new_v4().to_string();
    assert_eq!(fixture.request("targetLegacy", &operation)?["pending"], true);
    assert_eq!(fixture.imports().len(), 1);
    let payload = fixture.imports()[0]["params"].clone();
    assert_eq!(payload["idempotencyKey"], "old");
    assert!(payload.get("threads").is_none());
    std::fs::write(executable, "#!/bin/sh\nexit 0\n")?;
    assert_eq!(fixture.request("targetLegacy", &operation)?["ok"], true);
    remote.catalog.lock().unwrap().snapshot.as_mut().unwrap().projects = vec![canonical];
    let result = fixture.request("targetLegacy", &operation)?;
    assert_eq!(result["ok"], true);
    assert_eq!(result["target"]["projectId"], CANONICAL);
    assert_eq!(fixture.imports().len(), 1);
    assert!(!fixture.peer.events().contains("thread/start"));
    assert!(fixture.root.store.pending_drafts()?.is_empty());
    Ok(())
}
