use super::*;
use codex_session_board::{app::App, config::Config, remote, rpc::Rpc};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct Reader(Arc<Mutex<VecDeque<Value>>>);

impl Rpc for Reader {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let response = self.0.lock().unwrap().pop_front().unwrap();
        assert_eq!(response["method"], method);
        if method == "list_threads" {
            assert_eq!(params["mode"], "history");
            assert_eq!(params["source"], "codex");
            assert_eq!(params["hostId"], response["host"]);
            assert_eq!(params["cursor"], response["cursor"]);
            assert_eq!(params["archived"], false);
        }
        if response["error"] == true {
            anyhow::bail!("主机已断线");
        }
        Ok(response["result"].clone())
    }
}

#[test]
fn remote_discovery_paginates_and_isolates_failed_hosts_without_erasing_cached_data() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        ..Config::default()
    })?;
    discover(&app.store, "a", 1)?;
    let replies = Arc::new(Mutex::new(VecDeque::from([
        json!({"method":"list_projects","result":{"projects":[
            {"projectKind":"remote","hostId":"a","projectId":"project","path":"/remote","label":"同名","hostDisplayName":"A"},
            {"projectKind":"remote","hostId":"b","projectId":"project","path":"/remote","label":"同名","hostDisplayName":"B"}
        ]}}),
        json!({"method":"list_threads","host":"a","cursor":null,"error":true}),
        json!({"method":"list_threads","host":"b","cursor":null,"result":{"threads":[
            {"id":THREAD,"hostId":"b","title":"远程 B","projectId":"project","updatedAt":2}
        ],"nextCursor":"next"}}),
        json!({"method":"list_threads","host":"b","cursor":"next","result":{"threads":[],"nextCursor":null}}),
    ])));
    remote::scan(&app, &mut || {
        Ok(Box::new(Reader(replies.clone())) as Box<dyn Rpc>)
    })?;
    assert!(replies.lock().unwrap().is_empty());
    assert_eq!(app.store.remote_sessions()?.len(), 2);
    let projects = app.store.remote_projects()?;
    assert_ne!(projects[0]["id"], projects[1]["id"]);
    assert_eq!(
        app.snapshot()?["health"]["remoteHosts"][0]["scanComplete"],
        false
    );
    assert_eq!(app.snapshot()?["health"]["remoteHosts"][1]["count"], 1);
    Ok(())
}

#[test]
fn remote_partial_pages_and_failed_catalogs_preserve_previous_results() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        max_pages: 2,
        ..Config::default()
    })?;
    let replies = Arc::new(Mutex::new(VecDeque::from([
        json!({"method":"list_threads","host":"mac","cursor":null,"result":{"threads":[
            {"id":THREAD,"hostId":"mac","title":"远程","updatedAt":1}
        ],"nextCursor":"same"}}),
        json!({"method":"list_threads","host":"mac","cursor":"same","result":{"threads":[],"nextCursor":"same"}}),
    ])));
    assert!(remote::scan_host(&app, &mut Reader(replies), "mac", "Mac").is_err());
    assert_eq!(app.store.remote_sessions()?.len(), 1);
    app.store.cache_remote_projects(&[json!({"id":"cached"})])?;
    let replies = Arc::new(Mutex::new(VecDeque::from([
        json!({"method":"list_projects","error":true}),
    ])));
    assert!(
        remote::scan(&app, &mut || Ok(
            Box::new(Reader(replies.clone())) as Box<dyn Rpc>
        ))
        .is_err()
    );
    assert_eq!(app.store.remote_projects()?[0]["id"], "cached");
    Ok(())
}
