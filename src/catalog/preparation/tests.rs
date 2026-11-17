use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

const CANONICAL: &str = "019a8120-0000-7000-8000-000000000001";

struct Backend {
    state: Value,
    account: Value,
    projects: Vec<Value>,
    imports: Vec<Value>,
    on_list: Option<fn(&mut Self)>,
    on_state: Option<fn(&mut Self)>,
}

impl Backend {
    fn new() -> Self {
        let owner = input::digest(br#"["user","account"]"#);
        Self {
            account: json!({"userId":"user","workspaceRouting":{"chatgptAccountId":"account"}}),
            state: json!({"electron-renderer-content-owner":owner,
                "local-projects":{"old":{"id":"old","name":"Project","rootPaths":["/old","/second"]}},
                "project-appearances":{"old":{"color":"green","marker":{"kind":"emoji","emoji":"🌱"}}},
                (format!("electron-owner-value:app-server-projects-migration-by-host:{owner}")):{
                    "local:/target/codex":{"version":1,"projectsMigrated":false}}}),
            projects: vec![], imports: vec![], on_list: None, on_state: None,
        }
    }

    fn source() -> Value {
        json!({"hostId":"remote:target","codexHome":"/target/codex","peerFingerprint":"peer"})
    }

    fn read(&mut self) -> Result<Fresh> {
        Fresh::read(self, &Self::source())
    }

    fn pending(&mut self) {
        let owner = self.state["electron-renderer-content-owner"].clone();
        self.state[format!("electron-owner-value:app-server-pending-project-deletions-by-host:{}",
            owner.as_str().unwrap_or(""))] = json!({"local:/target/codex":{"old":{"projectId":CANONICAL}}});
    }
}

impl Rpc for Backend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "account/read" => Ok(self.account.clone()),
            "fs/getMetadata" => Ok(json!({"isFile":true,"isSymlink":false})),
            "fs/readFile" => {
                if let Some(change) = self.on_state.take() { change(self); }
                Ok(json!({"dataBase64":STANDARD.encode(serde_json::to_vec(&self.state)?)}))
            }
            "project/list" => {
                let response = json!({"data":self.projects,"nextCursor":null});
                if let Some(change) = self.on_list.take() { change(self); }
                Ok(response)
            }
            "project/read" => {
                let project = self.projects.iter().find(|project| project["id"] == params["projectId"])
                    .context("project not found")?;
                Ok(json!({"project":project}))
            }
            "project/import" => {
                self.imports.push(params.clone());
                if self.projects.is_empty() {
                    self.projects.push(json!({"id":CANONICAL,"name":params["name"],
                        "roots":params["roots"],"metadata":params["metadata"]}));
                }
                Ok(json!({"project":self.projects[0],"created":self.imports.len() == 1}))
            }
            _ => anyhow::bail!("Unexpected method {method}"),
        }
    }
}

#[test]
fn action_import_uses_exact_key_complete_metadata_without_thread_migration() -> Result<()> {
    let mut backend = Backend::new();
    let fresh = backend.read()?;
    assert!(backend.imports.is_empty());
    let project = fresh.prepare_legacy(&mut backend, "old", 1)?;
    assert_eq!(project["id"], CANONICAL);
    assert_eq!(backend.imports.len(), 1);
    let payload = &backend.imports[0];
    assert_eq!(payload["idempotencyKey"], "old");
    assert!(payload.get("threads").is_none());
    assert_eq!(payload.as_object().context("payload")?.len(), 4);
    assert_eq!(payload["roots"], json!([{"path":"/old"},{"path":"/second"}]));
    assert_eq!(payload["metadata"]["appearance.color"], "green");
    fresh.verify_after(&mut backend, &Backend::source(), Some("old"), &project)?;
    Ok(())
}

#[test]
fn same_key_retry_preserves_canonical_metadata_and_uses_fresh_roots() -> Result<()> {
    let mut backend = Backend::new();
    let fresh = backend.read()?;
    let mut project = fresh.prepare_legacy(&mut backend, "old", 1)?;
    project["roots"] = json!([{"path":"/changed"},{"path":"/other"}]);
    project["metadata"]["unrelated"] = json!("preserve");
    backend.projects[0] = project.clone();
    let retry = backend.read()?.prepare_legacy(&mut backend, "old", 1)?;
    assert_eq!(retry, project);
    assert_eq!(backend.imports.len(), 2);
    Ok(())
}

#[test]
fn direct_server_read_does_not_require_legacy_mapping_or_import() -> Result<()> {
    let mut backend = Backend::new();
    backend.state["local-projects"] = json!({});
    backend.projects.push(json!({"id":CANONICAL,"roots":[{"path":"/fresh"}],"metadata":{}}));
    let project = backend.read()?.server(&mut backend, CANONICAL)?;
    assert_eq!(roots(&project)?, vec![json!({"path":"/fresh"})]);
    assert!(backend.imports.is_empty());
    Ok(())
}

#[test]
fn existing_mapping_uses_current_backend_roots_without_overwrite() -> Result<()> {
    let mut backend = Backend::new();
    backend.state["app-server-project-id-by-legacy-project-id-by-host"] =
        json!({"local:/target/codex":{"old":CANONICAL}});
    backend.projects.push(json!({"id":CANONICAL,"name":"Renamed","roots":[{"path":"/fresh"}],
        "metadata":{"custom":"untouched"}}));
    let project = backend.read()?.prepare_legacy(&mut backend, "old", 1)?;
    assert_eq!(project, backend.projects[0]);
    assert!(backend.imports.is_empty());
    Ok(())
}

#[test]
fn missing_legacy_and_foreign_owner_reject_before_import() -> Result<()> {
    for foreign in [false,true] {
        let mut backend = Backend::new();
        if foreign {
            backend.state["local-projects"]["old"]["chatGptBacking"] =
                json!({"userId":"other","accountId":"account","projectId":"g-p-test"});
        } else {
            backend.state["local-projects"] = json!({});
        }
        assert!(backend.read()?.prepare_legacy(&mut backend, "old", 1).is_err());
        assert!(backend.imports.is_empty());
    }
    Ok(())
}

#[test]
fn missing_canonical_mapping_and_pending_deletion_never_import() -> Result<()> {
    for pending in [false, true] {
        let mut backend = Backend::new();
        if pending {
            backend.pending();
        } else {
            backend.state["app-server-project-id-by-legacy-project-id-by-host"] =
                json!({"local:/target/codex":{"old":CANONICAL}});
        }
        assert!(backend.read()?.prepare_legacy(&mut backend, "old", 1).is_err());
        assert!(backend.imports.is_empty());
    }
    Ok(())
}

#[test]
fn conflicting_mapping_and_alias_never_import() -> Result<()> {
    let mut backend = Backend::new();
    backend.state["local-projects"]["other"] =
        json!({"id":"other","name":"Other","rootPaths":["/other"]});
    backend.state["app-server-project-id-by-legacy-project-id-by-host"] =
        json!({"local:/target/codex":{"old":CANONICAL,"other":CANONICAL}});
    backend.projects.push(json!({"id":CANONICAL,"roots":[{"path":"/old"}],"metadata":{}}));
    assert!(backend.read()?.prepare_legacy(&mut backend, "old", 1).is_err());
    assert!(backend.imports.is_empty());
    backend.state["app-server-project-id-by-legacy-project-id-by-host"]
        ["local:/target/codex"] = json!({"old":CANONICAL});
    backend.projects[0]["metadata"]["codex.localProjectId"] = json!("other");
    assert!(backend.read()?.prepare_legacy(&mut backend, "old", 1).is_err());
    assert!(backend.imports.is_empty());
    Ok(())
}

#[test]
fn owner_account_source_and_deletion_changes_fail_before_opening() -> Result<()> {
    for change in ["owner","account","source","pending","roots"] {
        let mut backend = Backend::new();
        let fresh = backend.read()?;
        let project = fresh.prepare_legacy(&mut backend, "old", 1)?;
        let mut source = Backend::source();
        match change {
            "owner" => backend.state["electron-renderer-content-owner"] = json!("other"),
            "account" => backend.account["userId"] = json!("other"),
            "source" => source["peerFingerprint"] = json!("other"),
            "pending" => backend.pending(),
            "roots" => backend.state["local-projects"]["old"]["rootPaths"] = json!(["/changed"]),
            _ => unreachable!(),
        }
        assert!(fresh.verify_after(&mut backend, &source, Some("old"), &project).is_err(), "{change}");
    }
    Ok(())
}

#[test]
fn changes_during_project_scan_reject_before_import() -> Result<()> {
    let changes: [fn(&mut Backend); 5] = [
        |backend| { backend.state["local-projects"] = json!({}); },
        Backend::pending,
        |backend| {
            backend.account["userId"] = json!("other");
            backend.state["electron-renderer-content-owner"] =
                json!(input::digest(br#"["other","account"]"#));
        },
        |backend| { backend.state["local-projects"]["old"]["rootPaths"] = json!(["/changed"]); },
        |backend| {
            let owner = input::digest(br#"["user","account"]"#);
            backend.state[format!("electron-owner-value:app-server-projects-migration-by-host:{owner}")]
                ["local:/target/codex"]["projectsMigrated"] = json!(true);
        },
    ];
    for change in changes {
        let mut backend = Backend::new();
        let fresh = backend.read()?;
        backend.on_list = Some(change);
        assert!(fresh.prepare_legacy(&mut backend, "old", 1).is_err());
        assert!(backend.imports.is_empty());
    }
    Ok(())
}

#[test]
fn mapping_completed_during_scan_reuses_new_canonical_without_import() -> Result<()> {
    let mut backend = Backend::new();
    let fresh = backend.read()?;
    backend.on_list = Some(|backend| {
        backend.state["app-server-project-id-by-legacy-project-id-by-host"] =
            json!({"local:/target/codex":{"old":CANONICAL}});
        backend.projects.push(json!({"id":CANONICAL,"roots":[{"path":"/native"}],"metadata":{}}));
    });
    let project = fresh.prepare_legacy(&mut backend, "old", 1)?;
    assert_eq!(roots(&project)?, vec![json!({"path":"/native"})]);
    assert!(backend.imports.is_empty());
    Ok(())
}

#[test]
fn final_read_returns_current_roots_after_state_revalidation() -> Result<()> {
    for legacy in [None,Some("old")] {
        let mut backend = Backend::new();
        let fresh = backend.read()?;
        let project = fresh.prepare_legacy(&mut backend, "old", 1)?;
        backend.on_state = Some(|backend| {
            backend.projects[0]["roots"] = json!([{"path":"/latest"},{"path":"/latest-second"}]);
        });
        let current = fresh.verify_after(&mut backend, &Backend::source(), legacy, &project)?;
        assert_eq!(roots(&current)?, vec![json!({"path":"/latest"}),json!({"path":"/latest-second"})]);
    }
    Ok(())
}

#[test]
fn final_read_rejects_deleted_canonical_and_foreign_backing() -> Result<()> {
    for change in [|backend: &mut Backend| { backend.projects.clear(); },
        |backend: &mut Backend| {
            backend.projects[0]["metadata"]["codex.chatGptBacking"] =
                json!(r#"{"accountId":"account","userId":"other","projectId":"g-p-test"}"#);
        }] {
        let mut backend = Backend::new();
        let fresh = backend.read()?;
        let project = fresh.prepare_legacy(&mut backend, "old", 1)?;
        backend.on_state = Some(change);
        assert!(fresh.verify_after(&mut backend, &Backend::source(), Some("old"), &project).is_err());
    }
    Ok(())
}

#[test]
fn unrelated_legacy_thread_and_project_diagnostics_do_not_block_target_actions() -> Result<()> {
    let mut backend = Backend::new();
    backend.state["thread-project-assignments"] = json!({"stale":{"projectKind":false}});
    backend.state["thread-workspace-root-hints"] = json!(false);
    backend.state["local-projects"]["unrelated"] = json!({"id":false,"rootPaths":false});
    backend.state["project-appearances"]["unrelated"] = json!(false);
    backend.projects.push(json!({"id":CANONICAL,"roots":[{"path":"/target"}],"metadata":{}}));
    let fresh = backend.read()?;
    fresh.server(&mut backend, CANONICAL)?;
    fresh.legacy("old")?;
    backend.state["local-projects"] = json!(false);
    backend.state["project-appearances"] = json!(false);
    let fresh = backend.read()?;
    fresh.server(&mut backend, CANONICAL)?;
    assert!(fresh.legacy("old").is_err());
    assert!(backend.imports.is_empty());
    Ok(())
}

#[test]
fn malformed_project_identity_deletion_and_checkpoint_fields_remain_rejected() -> Result<()> {
    let owner = input::digest(br#"["user","account"]"#);
    for (field,value) in [
        ("app-server-project-id-by-legacy-project-id-by-host".to_owned(),
            json!({"local:/target/codex":{"old":false}})),
        (format!("electron-owner-value:app-server-pending-project-deletions-by-host:{owner}"),
            json!({"local:/target/codex":{"old":{}}})),
        (format!("electron-owner-value:app-server-projects-migration-by-host:{owner}"),
            json!({"local:/target/codex":{"version":"1","projectsMigrated":false}})),
        (format!("electron-owner-value:app-server-projects-migration-by-host:{owner}"),
            json!({"local:/target/codex":{"version":1,"projectsMigrated":"false"}})),
    ] {
        let mut backend = Backend::new();
        backend.state[&field] = value;
        backend.projects.push(json!({"id":CANONICAL,"roots":[{"path":"/target"}],"metadata":{}}));
        if field.contains("pending-project-deletions") {
            assert!(backend.read().is_err());
        } else {
            let fresh = backend.read()?;
            fresh.server(&mut backend, CANONICAL)?;
            assert!(fresh.prepare_legacy(&mut backend, "old", 1).is_err());
        }
        assert!(backend.imports.is_empty());
    }
    Ok(())
}
