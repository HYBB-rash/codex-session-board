use super::*;
use crate::config::Config;
use std::collections::VecDeque;

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
const SECOND: &str = "22222222-2222-4222-8222-222222222222";

struct Surface<'a> {
    app: &'a App,
    active: Vec<Value>,
    active_pages: VecDeque<Value>,
    pages: VecDeque<Value>,
    archive_queries: usize,
    action: Option<fn(&App) -> Result<()>>,
    turn_error: bool,
}

impl<'a> Surface<'a> {
    fn new(app: &'a App, active: &[&str], pages: Vec<Value>) -> Self {
        Self {
            app,
            active: active.iter().map(|id| json!({"id":id})).collect(),
            active_pages: VecDeque::new(),
            pages: pages.into(),
            archive_queries: 0,
            action: None,
            turn_error: false,
        }
    }

    fn scan(&mut self) -> Result<()> {
        scan_runtime_page(self.app, self, &mut HashMap::new(), crate::model::now())
    }
}

impl Rpc for Surface<'_> {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "thread/list" if params["archived"] == true => {
                self.archive_queries += 1;
                if let Some(action) = self.action.take() {
                    action(self.app)?;
                }
                let page = self.pages.pop_front().context("缺少测试归档页")?;
                if page["error"] == true {
                    anyhow::bail!("归档列表暂不可读");
                }
                Ok(page)
            }
            "thread/list" => {
                let page = self.active_pages.pop_front().unwrap_or_else(|| json!({"data":self.active}));
                if page["error"].is_object() {
                    return Err(rpc::RequestError { method: method.into(), details: page["error"].clone() }.into());
                }
                Ok(page)
            }
            "thread/turns/list" if self.turn_error => anyhow::bail!("Message too long"),
            "thread/turns/list" => Ok(json!({"data":[]})),
            _ => anyhow::bail!("意外的写入请求：{method}"),
        }
    }
}

fn app(directory: &std::path::Path) -> Result<App> {
    App::new(Config {
        data_dir: directory.into(),
        host_id: "remote:a".into(),
        sync_titles: true,
        ..Config::default()
    })
}

fn archive(app: &App) -> Result<()> {
    app.store.discover(&[json!({"id":FIRST,"name":"验收会话"})])?;
    app.store
        .set_state(FIRST, Some("to_archive"), "恢复后保留", 0, Some(0))?;
    app.store.confirm_archive(FIRST, (1, 0))
}

#[test]
fn fresh_unarchive_survives_restart_without_losing_notes_or_queuing_titles() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = app(directory.path())?;
    archive(&original)?;
    drop(original);
    let restored = app(directory.path())?;
    let mut reader = Surface::new(&restored, &[FIRST], vec![json!({"data":[]})]);
    reader.scan()?;
    let card = &restored.store.sessions()?[0];
    assert_eq!(card["state"], "to_archive");
    assert_eq!(card["note"], "恢复后保留");
    assert_eq!(card["manual"], true);
    assert_eq!(card["version"], 3);
    assert_eq!(card["inputVersion"], 0);
    assert_eq!(card["titleSyncPending"], false);
    assert!(restored.store.set_state(FIRST, Some("review"), "", 1, Some(0)).is_err());
    reader.scan()?;
    assert_eq!(restored.store.sessions()?[0]["version"], 3);
    assert_eq!(reader.archive_queries, 1);
    assert!(restored.health.lock().unwrap()["error"].is_null());
    Ok(())
}

#[test]
fn stale_discovery_and_still_archived_remote_entries_cannot_restore_cards() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    app.store.discover(&[json!({"id":FIRST})])?;
    assert!(app.store.sessions()?.is_empty());
    Surface::new(&app, &[FIRST], vec![json!({"data":[{"id":FIRST}]})]).scan()?;
    assert!(app.store.sessions()?.is_empty());
    assert_eq!(app.store.archived_revisions()?[FIRST], 2);
    Ok(())
}

#[test]
fn absent_active_entries_and_saved_progress_never_count_as_restoration_proof() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    config::write_json(
        &app.config.data_dir.join("scan-progress.json"),
        &json!({"cursor":"old","cursors":["old"],"identifiers":[FIRST]}),
        true,
    )?;
    let mut reader = Surface::new(&app, &[], vec![]);
    reader.scan()?;
    assert!(app.store.sessions()?.is_empty());
    assert_eq!(reader.archive_queries, 0);
    Ok(())
}

#[test]
fn incomplete_or_invalid_archived_proof_is_visible_and_does_not_block_other_cards() -> Result<()> {
    let invalid = [
        json!({"error":true}),
        json!({"data":[],"nextCursor":"unread"}),
        json!({"data":[{"id":FIRST,"hostId":"remote:b"}]}),
        json!({"data":[{"id":"invalid"}]}),
        json!({"data":[],"nextCursor":7}),
    ];
    for page in invalid {
        let directory = tempfile::tempdir()?;
        let mut config = app(directory.path())?.config;
        config.max_pages = 1;
        let app = App::new(config)?;
        archive(&app)?;
        Surface::new(&app, &[FIRST, SECOND], vec![page]).scan()?;
        assert!(!app.store.contains(FIRST)?);
        assert!(app.store.contains(SECOND)?);
        assert!(app.health.lock().unwrap()["error"].as_str().unwrap().contains("归档恢复待核对"));
        Surface::new(&app, &[FIRST, SECOND], vec![json!({"data":[]})]).scan()?;
        assert!(app.store.contains(FIRST)?);
        assert!(app.health.lock().unwrap()["error"].is_null());
    }
    Ok(())
}

#[test]
fn archived_pagination_requires_completion_and_rejects_repeated_cursors() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    let first = json!({"data":[],"nextCursor":"next"});
    Surface::new(&app, &[FIRST], vec![first.clone(), first.clone()]).scan()?;
    assert!(!app.store.contains(FIRST)?);
    assert!(app.health.lock().unwrap()["error"].as_str().unwrap().contains("重复"));
    let mut reader = Surface::new(&app, &[FIRST], vec![first, json!({"data":[{"id":FIRST}]})]);
    reader.scan()?;
    assert_eq!(reader.archive_queries, 2);
    assert!(!app.store.contains(FIRST)?);
    Ok(())
}

#[test]
fn a_newer_archive_cannot_be_undone_by_an_inflight_restoration_scan() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    let mut reader = Surface::new(&app, &[FIRST], vec![json!({"data":[]})]);
    reader.action = Some(|app| {
        app.store.restore_confirmed_active(FIRST, 2)?;
        app.store.confirm_archive(FIRST, (3, 0))
    });
    reader.scan()?;
    assert!(!app.store.contains(FIRST)?);
    assert_eq!(app.store.archived_revisions()?[FIRST], 4);
    Ok(())
}

#[test]
fn a_retired_source_cannot_restore_cards_after_its_reply_arrives() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    let mut reader = Surface::new(&app, &[FIRST], vec![json!({"data":[]})]);
    reader.action = Some(App::deactivate);
    assert!(reader.scan().is_err());
    assert!(!app.store.contains(FIRST)?);
    assert_eq!(app.store.archived_revisions()?[FIRST], 2);
    assert!(!app.config.data_dir.join("scan-progress.json").exists());
    Ok(())
}

#[test]
fn identical_remote_identifiers_keep_independent_archive_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let first = app(&directory.path().join("first"))?;
    let mut config = first.config.clone();
    config.data_dir = directory.path().join("second");
    config.host_id = "remote:b".into();
    let second = App::new(config)?;
    archive(&first)?;
    archive(&second)?;
    Surface::new(&first, &[FIRST], vec![json!({"data":[]})]).scan()?;
    assert!(first.store.contains(FIRST)?);
    assert!(!second.store.contains(FIRST)?);
    assert_eq!(second.store.archived_revisions()?[FIRST], 2);
    Ok(())
}

fn pending_restoration(directory: &std::path::Path) -> Result<Config> {
    let source = std::sync::Arc::new(app(&directory.join("remote"))?);
    archive(&source)?;
    let config = source.config.clone();
    let root = App::new(Config {
        data_dir: directory.join("root"),
        ..Config::default()
    })?;
    root.remotes.lock().unwrap().insert("remote:a".into(), source.clone());
    let mut reader = Surface::new(&source, &[], vec![json!({"error":true})]);
    reader.active_pages = [
        json!({"data":[],"nextCursor":"middle"}),
        json!({"data":[{"id":FIRST}],"nextCursor":"tail"}),
        json!({"data":[]}),
    ].into();
    reader.scan()?;
    reader.scan()?;
    assert!(root.snapshot()?["health"]["remoteConnections"][0]["error"]
        .as_str().unwrap().contains("归档恢复待核对"));
    reader.active_pages = [
        json!({"data":[],"nextCursor":"last"}),
        json!({"data":[]}),
    ].into();
    reader.pages.push_back(json!({"data":[]}));
    reader.scan()?;
    assert!(root.snapshot()?["health"]["remoteConnections"][0]["error"].is_string());
    Ok(config)
}

#[test]
fn restoration_warnings_reach_root_and_survive_unrelated_pages_restart_and_expired_cursor() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let reopened = App::new(pending_restoration(directory.path())?)?;
    let mut reader = Surface::new(&reopened, &[], vec![]);
    reader.active_pages = [json!({"error":{"message":"Invalid cursor"}})].into();
    assert!(reader.scan().is_err());
    let progress = Progress::load(&reopened)?;
    assert!(progress.cursor.is_none());
    assert!(progress.restoration_errors.contains_key(FIRST));
    reader.pages.push_back(json!({"data":[]}));
    reader.scan()?;
    assert!(!reopened.store.contains(FIRST)?);
    assert!(reopened.health.lock().unwrap()["error"].is_string());
    assert!(Progress::load(&reopened)?.restoration_errors.contains_key(FIRST));
    reader.pages.push_back(json!({"data":[{"id":FIRST}]}));
    reader.scan()?;
    assert!(!reopened.store.contains(FIRST)?);
    assert!(reopened.health.lock().unwrap()["error"].is_null());
    assert!(Progress::load(&reopened)?.restoration_errors.is_empty());
    Ok(())
}

#[test]
fn restoration_warning_is_durable_when_a_later_runtime_read_disconnects() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    let mut reader = Surface::new(&app, &[FIRST, SECOND], vec![json!({"error":true})]);
    reader.turn_error = true;
    assert!(reader.scan().is_err());
    let progress = Progress::load(&app)?;
    assert!(progress.cursor.is_none());
    assert!(progress.restoration_errors.contains_key(FIRST));
    assert!(!app.store.contains(FIRST)?);
    drop(reader);
    let config = app.config.clone();
    drop(app);
    let reopened = App::new(config)?;
    Surface::new(&reopened, &[], vec![json!({"data":[]})]).scan()?;
    assert!(reopened.health.lock().unwrap()["error"].is_string());
    assert!(!reopened.store.contains(FIRST)?);
    Ok(())
}

#[test]
fn unconfirmed_archived_cards_do_not_fetch_potentially_oversized_turns() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = app(directory.path())?;
    archive(&app)?;
    let mut reader = Surface::new(&app, &[FIRST], vec![json!({"error":true})]);
    reader.turn_error = true;
    reader.scan()?;
    assert!(!app.store.contains(FIRST)?);
    assert!(app.health.lock().unwrap()["error"].is_string());
    assert_eq!(app.health.lock().unwrap()["runtimeVerified"], true);
    Ok(())
}
