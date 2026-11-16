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
    let archive = directory.join("resources/app.asar");
    std::fs::create_dir_all(archive.parent().context("archive directory")?)?;
    std::fs::write(&archive, b"isolated candidate")?;
    std::fs::write(&executable, "#!/bin/sh\nexit 0\n")?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
    config.open_command = vec![executable.to_string_lossy().into_owned()];
    let capability = directory.join("capability.json");
    config.native_new_capability = Some(capability.clone());
    std::fs::write(capability, serde_json::to_vec(&json!({
        "protocol":"codex-remote-draft-v2","archive":archive,
        "sha256":format!("{:x}",Sha256::digest(b"isolated candidate")),
        "openCommand":config.open_command
    }))?)?;
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
                "hostId":"remote:test","displayName":"test","alias":"example"}]}))?)?;
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
        self.root.request("/api/new", Some(&json!({"projectId":key,"operationId":operation})))
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
        source, captured_at: now(), projects: Vec::new(), active: Vec::new(),
        archived: Vec::new(), coverage: json!({}), issues: Vec::new(),
    });
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
    assert_eq!(query.len(), 3);
    assert_eq!(query["hostId"], "remote:test");
    assert_eq!(query["projectId"], CANONICAL);
    assert_eq!(query["operationId"], operation);
    assert!(fixture.imports().is_empty());
    assert!(!fixture.peer.events().contains("thread/start"));
    assert!(!fixture.peer.events().contains("turn/start"));
    assert!(fixture.root.store.pending_drafts()?.is_empty());
    Ok(())
}

#[test]
fn lost_navigation_retry_reuses_prepared_target_without_reimport_or_thread_start() -> Result<()> {
    let fixture = NativeFixture::new()?;
    let executable = &fixture.root.config.open_command[0];
    std::fs::write(executable, "#!/bin/sh\nexit 1\n")?;
    let operation = uuid::Uuid::new_v4().to_string();
    assert_eq!(fixture.request("targetLegacy", &operation)?["pending"], true);
    assert_eq!(fixture.imports().len(), 1);
    let payload = fixture.imports()[0]["params"].clone();
    assert_eq!(payload["idempotencyKey"], "old");
    assert!(payload.get("threads").is_none());
    std::fs::write(executable, "#!/bin/sh\nexit 0\n")?;
    let result = fixture.request("targetLegacy", &operation)?;
    assert_eq!(result["ok"], true);
    assert_eq!(result["target"]["projectId"], CANONICAL);
    assert_eq!(fixture.imports().len(), 1);
    assert!(!fixture.peer.events().contains("thread/start"));
    assert!(fixture.root.store.pending_drafts()?.is_empty());
    Ok(())
}
