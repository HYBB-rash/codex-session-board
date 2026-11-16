use super::*;
use std::collections::VecDeque;
use sha2::Digest;
use std::os::unix::fs::PermissionsExt;

struct Reader {
    responses: VecDeque<Result<Value>>,
    calls: Vec<(String, Value)>,
}

#[test]
fn project_confirmation_uses_the_same_registry_without_optional_git_probes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("state.json");
    let registry = json!({"remote-projects":[{
        "id":"project","hostId":"remote:host","label":"项目","remotePath":"/remote/project"
    }]});
    std::fs::write(&path, serde_json::to_vec(&registry)?)?;
    let config = Config {
        sidebar_state_file: path.clone(),
        sidebar_socket: directory.path().join("does-not-exist.sock"),
        ..Config::default()
    };
    let projects = crate::remote::project_catalog(&config, || panic!("unexpected fallback"))?;
    let thread = json!({"id":uuid::Uuid::new_v4().to_string(),"cwd":"/remote/project"});
    confirm_project(&config, &projects[0], &thread)?;
    let mut changed = registry;
    changed["remote-projects"][0]["remotePath"] = json!("/changed");
    std::fs::write(path, serde_json::to_vec(&changed)?)?;
    assert!(confirm_project(&config, &projects[0], &thread).is_err());
    Ok(())
}

fn isolated() -> Result<(tempfile::TempDir, App)> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        sync_titles: false,
        ..Config::default()
    })?;
    Ok((directory, app))
}

impl Rpc for Reader {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.calls.push((method.into(), params));
        self.responses.pop_front().context("非预期 RPC 调用")?
    }
}

impl Reader {
    fn methods(&self) -> Vec<&str> {
        self.calls.iter().map(|call| call.0.as_str()).collect()
    }
}

fn draft<'app>(remote: &'app App, request: &str) -> Result<Draft<'app>> {
    let (record, _) = remote.store.draft_begin("remote:project", request)?;
    Ok(Draft {
        remote,
        project: json!({"id":"remote:project","paths":["/remote/root"]}),
        record,
        request: request.into(),
    })
}

#[test]
fn lost_empty_draft_receipts_never_replay_thread_start() -> Result<()> {
    assert!(validate_remote_project(&json!({"paths":["relative"]})).is_err());
    assert!(validate_remote_project(&json!({"paths":[]})).is_err());
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let mut reader = Reader {
        responses: VecDeque::from([Err(anyhow::anyhow!("回执丢失"))]),
        calls: Vec::new(),
    };
    for _attempt in 0..2 {
        assert!(
            draft(&app, &request)?
                .thread(&mut reader)
                .is_err()
        );
    }
    assert!(reader.calls.is_empty());
    assert!(
        app.store
            .draft_begin("another-project", &uuid::Uuid::new_v4().to_string())?
            .1
    );
    Ok(())
}

#[test]
fn acknowledged_empty_drafts_only_read_the_same_thread_on_retry() -> Result<()> {
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let identifier = uuid::Uuid::new_v4().to_string();
    let wrong = uuid::Uuid::new_v4().to_string();
    app.store.draft_begin("remote:project", &request)?;
    app.store.draft_update(&request, Some(&identifier), "created")?;
    let mut reader = Reader {
        responses: VecDeque::from([
            Ok(json!({"thread":{"id":identifier,"turns":[],
                "cwd":"/remote/root","historyMode":"legacy"}})),
            Ok(json!({"thread":{"id":wrong,"turns":[]}})),
            Ok(json!({"thread":{"id":identifier,"turns":[{"id":"user-turn"}]}})),
            Ok(json!({"thread":{"id":identifier,"historyMode":"legacy","turns":[]}})),
        ]),
        calls: Vec::new(),
    };
    assert_eq!(
        draft(&app, &request)?.thread(&mut reader)?["id"],
        identifier
    );
    for _attempt in 0..2 {
        assert!(
            draft(&app, &request)?
                .thread(&mut reader)
                .is_err()
        );
    }
    assert_eq!(
        draft(&app, &request)?.thread(&mut reader)?["id"],
        identifier
    );
    assert_eq!(
        reader.methods(),
        ["thread/read", "thread/read", "thread/read", "thread/read"]
    );
    assert!(reader.calls.iter()
        .all(|call| call.1["threadId"] == identifier && call.1["includeTurns"] == true));
    Ok(())
}

#[test]
fn lost_initial_title_receipts_keep_the_created_id_without_replaying_creation() -> Result<()> {
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let identifier = uuid::Uuid::new_v4().to_string();
    app.store.draft_begin("remote:project", &request)?;
    app.store.draft_update(&request, Some(&identifier), "created")?;
    let mut reader = Reader {
        responses: VecDeque::from([
            Err(anyhow::anyhow!("初始标题回执丢失")),
            Ok(json!({"thread":{"id":identifier,"name":"已确认空会话","historyMode":"legacy","turns":[]}})),
        ]),
        calls: Vec::new(),
    };
    assert!(draft(&app,&request)?.thread(&mut reader).is_err());
    assert_eq!(app.store.draft(&request)?["threadId"],identifier);
    assert_eq!(draft(&app,&request)?.thread(&mut reader)?["id"],identifier);
    assert_eq!(reader.methods(),["thread/read","thread/read"]);
    assert!(reader.calls.iter().all(|call| call.1["threadId"] == identifier));
    Ok(())
}

#[test]
fn initial_draft_readback_rejects_new_turns_and_unconfirmed_history() -> Result<()> {
    for (history, turns) in [("legacy",json!([{"id":"new-turn"}])),
        ("paginated",json!([]))] {
        let (_directory,app) = isolated()?;
        let request = uuid::Uuid::new_v4().to_string();
        let identifier = uuid::Uuid::new_v4().to_string();
        app.store.draft_begin("remote:project", &request)?;
        app.store.draft_update(&request, Some(&identifier), "created")?;
        let mut reader = Reader {
            responses: VecDeque::from([
                Ok(json!({"thread":{"id":identifier,"turns":turns,
                    "cwd":"/remote/root","historyMode":history}})),
            ]),
            calls: Vec::new(),
        };
        assert!(draft(&app,&request)?.thread(&mut reader).is_err());
        assert_eq!(app.store.draft(&request)?["stage"],"created");
        assert!(!reader.methods().contains(&"turn/start"));
        assert_eq!(reader.methods(),["thread/read"]);
    }
    Ok(())
}

#[test]
fn releasing_uncertain_drafts_requires_confirmation_and_cannot_clear_another_project() -> Result<()>
{
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    app.store.draft_begin("remote:project", &request)?;
    assert_eq!(app.snapshot()?["pendingDrafts"][0]["requestId"], request);
    for body in [
        json!({"projectId":"remote:project","requestId":request}),
        json!({"projectId":"other","requestId":request,"confirmed":true}),
    ] {
        assert!(app.request("/api/new/reset", Some(&body)).is_err());
    }
    assert_eq!(app.store.draft(&request)?["stage"], "starting");
    app.request(
        "/api/new/reset",
        Some(&json!({"projectId":"remote:project",
        "requestId":request,"confirmed":true})),
    )?;
    assert!(
        app.snapshot()?["pendingDrafts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(app.store.draft(&request)?["stage"], "abandoned");
    assert!(app.store.sessions()?.is_empty());
    assert!(app.store.pending_titles(None)?.is_empty());
    Ok(())
}

#[test]
fn project_confirmation_handles_implicit_membership_but_rejects_ambiguous_or_foreign_hosts() {
    let project = json!({"nativeId":"project","hostId":"remote:a","paths":["/remote/root"]});
    let thread = json!({"id":"thread","cwd":"/remote/root"});
    assert!(project_bound(&json!({}),&project,&thread,true));
    assert!(!project_bound(&json!({}),&project,&thread,false));
    let mut state = json!({"thread-project-assignments":{"thread":{
        "projectKind":"remote","projectId":"project","hostId":"remote:a"}}});
    assert!(project_bound(&state,&project,&thread,false));
    state["thread-project-membership-host-ids"] = json!({"thread":"remote:other"});
    assert!(!project_bound(&state,&project,&thread,true));
    state["thread-project-membership-host-ids"]["thread"] = json!("remote:a");
    assert!(project_bound(&state,&project,&thread,true));
    state["thread-project-assignments"]["thread"]["hostId"] = json!("remote:other");
    assert!(!project_bound(&state,&project,&thread,true));
    assert!(!unique_project_root(&[project.clone(),project.clone()],&project));
    assert!(unique_project_root(std::slice::from_ref(&project),&project));
}

fn native_app() -> Result<(tempfile::TempDir, App, Value)> {
    let (directory, mut app) = isolated()?;
    let path = directory.path();
    std::fs::create_dir(path.join("resources"))?;
    let archive = path.join("resources/app.asar");
    std::fs::write(&archive, b"isolated native candidate")?;
    let registry = json!({"remote-projects":[{"id":"project & 中","hostId":"remote:a",
        "label":"项目","remotePath":"/not-a-local-path"}]});
    app.config.sidebar_state_file = path.join("projects.json");
    std::fs::write(&app.config.sidebar_state_file, serde_json::to_vec(&registry)?)?;
    let executable = path.join("codex");
    std::fs::write(&executable, format!(
        "#!/bin/sh\nsleep 1.1\nprintf '%s\\n' \"$1\" >> '{}'\n", path.join("opens").display()))?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
    app.config.open_command = vec![executable.to_string_lossy().into()];
    app.config.native_new_capability = Some(path.join("capability.json"));
    let capability = json!({"protocol":"codex-remote-draft-v1","archive":archive,
        "sha256":format!("{:x}", sha2::Sha256::digest(b"isolated native candidate")),
        "openCommand":app.config.open_command});
    std::fs::write(app.config.native_new_capability.as_ref().unwrap(),
        serde_json::to_vec(&capability)?)?;
    Ok((directory, app, capability))
}

fn native_body() -> Value {
    json!({"projectId":crate::store::remote::identity("remote:a","project & 中")})
}

fn native_navigation_receipt(directory: &std::path::Path) -> Result<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match std::fs::read_to_string(directory.join("opens")) {
            Ok(record) if record.ends_with('\n') => return Ok(record),
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error.into());
            }
            _ => {}
        }
        ensure!(std::time::Instant::now() < deadline, "原生跳转 fixture 未写入回执");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_remote_new_only_navigates_and_preserves_the_selected_host_and_project() -> Result<()> {
    let (directory, app, _) = native_app()?;
    let response = app.request("/api/new", Some(&native_body()))?;
    assert_eq!(response["native"], true);
    let url = url::Url::parse(response["url"].as_str().unwrap())?;
    assert_eq!(url.path(), "/new-remote");
    assert_eq!(url.query_pairs().collect::<Vec<_>>(),
        [("hostId".into(),"remote:a".into()),("projectId".into(),"project & 中".into())]);
    assert!(app.store.pending_drafts()?.is_empty());
    assert!(app.store.sessions()?.is_empty());
    assert!(app.remotes.lock().unwrap().is_empty());
    assert_eq!(native_navigation_receipt(directory.path())?.lines().collect::<Vec<_>>(),
        [response["url"].as_str().unwrap()]);
    Ok(())
}

#[test]
fn native_remote_new_rejects_missing_or_changed_capabilities_before_navigation() -> Result<()> {
    let (directory, app, capability) = native_app()?;
    let path = app.config.native_new_capability.as_ref().unwrap();
    for (key, value) in [("protocol",json!("unknown")),
        ("sha256",json!("wrong")),("openCommand",json!(["another-client"]))] {
        let mut changed = capability.clone();
        changed[key] = value;
        std::fs::write(path, serde_json::to_vec(&changed)?)?;
        assert!(app.request("/api/new", Some(&native_body())).is_err());
    }
    let unrelated = directory.path().join("unrelated.asar");
    std::fs::copy(capability["archive"].as_str().unwrap(), &unrelated)?;
    let mut changed = capability.clone();
    changed["archive"] = json!(unrelated);
    std::fs::write(path, serde_json::to_vec(&changed)?)?;
    assert!(app.request("/api/new", Some(&native_body())).is_err());
    std::fs::remove_file(path)?;
    assert!(app.request("/api/new", Some(&native_body())).is_err());
    assert!(!directory.path().join("opens").exists());
    assert!(app.store.pending_drafts()?.is_empty());
    Ok(())
}

#[test]
fn native_remote_new_rechecks_project_registration_and_never_replays_old_creation() -> Result<()> {
    let (directory, app, _) = native_app()?;
    let mut wrong = native_body();
    wrong["hostId"] = json!("remote:b");
    assert!(app.request("/api/new", Some(&wrong)).is_err());
    let mut old = native_body();
    old["requestId"] = json!(uuid::Uuid::new_v4().to_string());
    assert!(app.request("/api/new", Some(&old)).is_err());
    let mut registry: Value = serde_json::from_slice(&std::fs::read(&app.config.sidebar_state_file)?)?;
    let mut duplicate = registry["remote-projects"][0].clone();
    duplicate["hostId"] = json!("remote:b");
    registry["remote-projects"].as_array_mut().unwrap().push(duplicate);
    std::fs::write(&app.config.sidebar_state_file, serde_json::to_vec(&registry)?)?;
    assert!(app.request("/api/new", Some(&native_body())).is_err());
    std::fs::write(&app.config.sidebar_state_file, r#"{"remote-projects":[]}"#)?;
    assert!(app.request("/api/new", Some(&native_body())).is_err());
    assert!(!directory.path().join("opens").exists());
    assert!(app.store.pending_drafts()?.is_empty());
    Ok(())
}
