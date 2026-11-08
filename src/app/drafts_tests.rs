use super::*;
use std::collections::VecDeque;

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
    let (record, fresh) = remote.store.draft_begin("remote:project", request)?;
    Ok(Draft {
        remote,
        project: json!({"id":"remote:project","paths":["/remote/root"]}),
        record,
        request: request.into(),
        fresh,
    })
}

#[test]
fn lost_empty_draft_receipts_never_replay_thread_start() -> Result<()> {
    assert!(draft_params(&json!({"paths":["relative"]})).is_err());
    assert!(draft_params(&json!({"paths":[]})).is_err());
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let mut reader = Reader {
        responses: VecDeque::from([Err(anyhow::anyhow!("回执丢失"))]),
        calls: Vec::new(),
    };
    for _attempt in 0..2 {
        assert!(
            draft(&app, &request)?
                .thread(&app.store, &mut reader)
                .is_err()
        );
    }
    assert_eq!(reader.calls.len(), 1);
    assert_eq!(reader.calls[0].0, "thread/start");
    assert!(reader.calls[0].1.get("input").is_none());
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
    let mut reader = Reader {
        responses: VecDeque::from([
            Ok(json!({"thread":{"id":identifier,"turns":[],
                "cwd":"/remote/root","historyMode":"legacy"}})),
            Ok(json!({})),
            Ok(json!({"thread":{"id":identifier,"name":"新会话","turns":[]}})),
            Ok(json!({"thread":{"id":wrong,"turns":[]}})),
            Ok(json!({"thread":{"id":identifier,"turns":[{"id":"user-turn"}]}})),
            Ok(json!({"thread":{"id":identifier,"turns":[]}})),
        ]),
        calls: Vec::new(),
    };
    assert_eq!(
        draft(&app, &request)?.thread(&app.store, &mut reader)?["id"],
        identifier
    );
    for _attempt in 0..2 {
        assert!(
            draft(&app, &request)?
                .thread(&app.store, &mut reader)
                .is_err()
        );
    }
    assert_eq!(
        draft(&app, &request)?.thread(&app.store, &mut reader)?["id"],
        identifier
    );
    assert_eq!(
        reader.methods(),
        ["thread/start", "thread/name/set", "thread/read",
            "thread/read", "thread/read", "thread/read"]
    );
    assert!(reader.calls.iter().skip(2)
        .all(|call| call.1["threadId"] == identifier && call.1["includeTurns"] == true));
    Ok(())
}

#[test]
fn lost_initial_title_receipts_keep_the_created_id_without_replaying_creation() -> Result<()> {
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let identifier = uuid::Uuid::new_v4().to_string();
    let mut reader = Reader {
        responses: VecDeque::from([
            Ok(json!({"thread":{"id":identifier,"turns":[],
                "cwd":"/remote/root","historyMode":"legacy"}})),
            Err(anyhow::anyhow!("初始标题回执丢失")),
            Ok(json!({"thread":{"id":identifier,"name":"已确认空会话","turns":[]}})),
        ]),
        calls: Vec::new(),
    };
    assert!(draft(&app,&request)?.thread(&app.store,&mut reader).is_err());
    assert_eq!(app.store.draft(&request)?["threadId"],identifier);
    assert_eq!(draft(&app,&request)?.thread(&app.store,&mut reader)?["id"],identifier);
    assert_eq!(reader.methods(),["thread/start","thread/name/set","thread/read"]);
    assert_eq!(reader.calls[0].1["historyMode"],"legacy");
    assert_eq!(reader.calls[1].1,json!({"threadId":identifier,"name":"新会话"}));
    Ok(())
}

#[test]
fn initial_draft_readback_rejects_new_turns_and_unconfirmed_history() -> Result<()> {
    for (history, turns) in [("legacy",json!([{"id":"new-turn"}])),
        ("paginated",json!([]))] {
        let (_directory,app) = isolated()?;
        let request = uuid::Uuid::new_v4().to_string();
        let identifier = uuid::Uuid::new_v4().to_string();
        let mut reader = Reader {
            responses: VecDeque::from([
                Ok(json!({"thread":{"id":identifier,"turns":[],
                    "cwd":"/remote/root","historyMode":history}})),
                Ok(json!({})),
                Ok(json!({"thread":{"id":identifier,"name":"新会话","turns":turns}})),
            ]),
            calls: Vec::new(),
        };
        assert!(draft(&app,&request)?.thread(&app.store,&mut reader).is_err());
        assert_eq!(app.store.draft(&request)?["stage"],"created");
        assert!(!reader.methods().contains(&"turn/start"));
        if history!="legacy" {assert_eq!(reader.methods(),["thread/start"]);}
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
