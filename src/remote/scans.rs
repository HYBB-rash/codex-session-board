use crate::{
    app::App,
    config, observer,
    rpc::{self, Rpc},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::Ordering;

#[derive(Default, Serialize, Deserialize)]
struct Progress {
    cursor: Option<String>,
    cursors: BTreeSet<String>,
    identifiers: BTreeSet<String>,
}

impl Progress {
    fn load(app: &App) -> Result<Self> {
        let path = app.config.data_dir.join("scan-progress.json");
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn page(&self, app: &App, reader: &mut dyn Rpc) -> Result<Value> {
        let result = reader.call(
            "thread/list",
            rpc::thread_list_params(app.config.page_size, json!(self.cursor)),
        );
        if let Err(error) = &result
            && self.cursor.is_some()
            && error
                .downcast_ref::<rpc::RequestError>()
                .is_some_and(|error| {
                    error.details["message"]
                        .as_str()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains("cursor")
                })
        {
            config::write_json(
                &app.config.data_dir.join("scan-progress.json"),
                &serde_json::to_value(Self::default())?,
                true,
            )?;
            return Err(anyhow::anyhow!(
                "远程分页游标已失效，下轮从首页重读：{error}"
            ));
        }
        result
    }

    fn advance(&mut self, threads: &[Value], next: &Value, max_pages: usize) -> Result<bool> {
        let next = match next {
            Value::Null => None,
            Value::String(value) if value.is_empty() => None,
            Value::String(value) => Some(value.to_owned()),
            _ => anyhow::bail!("远程分页游标无效"),
        };
        if let Some(next) = &next {
            ensure!(self.cursors.len() < max_pages, "远程扫描达到分页上限");
            ensure!(self.cursors.insert(next.clone()), "远程分页游标重复");
        }
        self.cursor = next;
        self.identifiers.extend(
            threads
                .iter()
                .map(|thread| thread["id"].as_str().unwrap_or("").to_owned()),
        );
        Ok(self.cursor.is_none())
    }
}

fn threads(response: &Value, host: &str) -> Result<Vec<Value>> {
    let entries = response["data"]
        .as_array()
        .context("远程会话列表格式无效")?;
    let mut seen = HashSet::new();
    let mut threads = Vec::new();
    for thread in entries {
        let identifier = thread["id"].as_str().context("远程会话缺少 ID")?;
        let uuid = uuid::Uuid::parse_str(identifier).context("远程会话 ID 无效")?;
        ensure!(uuid.to_string() == identifier, "远程会话 ID 格式无效");
        ensure!(
            thread.get("hostId").is_none_or(|value| value == host),
            "远程回包主机不匹配"
        );
        if seen.insert(identifier) {
            threads.push(thread.clone());
        }
    }
    Ok(threads)
}

fn merge_head(app: &App, reader: &mut dyn Rpc, entries: &mut Vec<Value>) -> Result<()> {
    let response = reader.call(
        "thread/list",
        rpc::thread_list_params(app.config.page_size, Value::Null),
    )?;
    let head = threads(&response, &app.config.host_id)?;
    let identifiers: HashSet<_> = head
        .iter()
        .map(|thread| thread["id"].clone().to_string())
        .collect();
    entries.retain(|thread| !identifiers.contains(&thread["id"].clone().to_string()));
    entries.extend(head);
    Ok(())
}

pub(crate) fn scan_runtime_page(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut HashMap<String, String>,
    since: f64,
) -> Result<()> {
    let mut progress = Progress::load(app)?;
    let generation = app.native_epoch();
    let continued = progress.cursor.is_some();
    let response = progress.page(app, reader)?;
    ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
    let mut entries = threads(&response, &app.config.host_id)?;
    if continued {
        merge_head(app, reader, &mut entries)?;
    }
    let complete = progress.advance(&entries, &response["nextCursor"], app.config.max_pages)?;
    app.store.discover(&entries)?;
    if app.config.native_sections {
        app.cache_native_page(generation, reader, &entries)?;
    }
    observer::observe_runtime_threads(app, reader, revisions, since, &entries)?;
    let count = progress.identifiers.len();
    if complete {
        *app.active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))? =
            Some(progress.identifiers.into_iter().collect());
        progress = Progress::default();
    }
    config::write_json(
        &app.config.data_dir.join("scan-progress.json"),
        &serde_json::to_value(progress)?,
        true,
    )?;
    app.update_health(json!({"scanComplete":complete,"discovered":count,"lastSuccessAt":crate::model::now(),"error":null}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    struct Page(Value);

    impl Rpc for Page {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            if method == "thread/turns/list" {
                return Ok(json!({"data":[]}));
            }
            assert_eq!(method, "thread/list");
            if params["cursor"].is_null() && !self.0["expected"].is_null() {
                return Ok(json!({"data":self.0.get("head").cloned().unwrap_or(json!([]))}));
            }
            assert_eq!(params["cursor"], self.0["expected"]);
            if self.0["error"].is_object() {
                return Err(rpc::RequestError {
                    method: method.into(),
                    details: self.0["error"].clone(),
                }
                .into());
            }
            Ok(self.0.clone())
        }
    }

    #[test]
    fn remote_pagination_survives_restart_and_bad_pages_do_not_erase_records() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = Config {
            data_dir: directory.path().into(),
            host_id: "remote:a".into(),
            sync_titles: false,
            ..Config::default()
        };
        let identifier = uuid::Uuid::new_v4().to_string();
        let first =
            json!({"expected":null,"data":[{"id":identifier,"name":"first"}],"nextCursor":"next"});
        let app = App::new(config.clone())?;
        scan_runtime_page(
            &app,
            &mut Page(first),
            &mut HashMap::new(),
            crate::model::now(),
        )?;
        assert!(!app.is_active(&identifier)?);
        drop(app);
        let app = App::new(config)?;
        let bad = json!({"expected":"next","data":[],"nextCursor":"next"});
        assert!(
            scan_runtime_page(
                &app,
                &mut Page(bad),
                &mut HashMap::new(),
                crate::model::now()
            )
            .is_err()
        );
        assert_eq!(app.store.sessions()?.len(), 1);
        let final_page = json!({"expected":"next","data":[],"nextCursor":null});
        scan_runtime_page(
            &app,
            &mut Page(final_page),
            &mut HashMap::new(),
            crate::model::now(),
        )?;
        assert!(app.is_active(&identifier)?);
        assert!(app.health.lock().unwrap()["scanComplete"] == true);
        Ok(())
    }

    #[test]
    fn cross_host_and_invalid_identifiers_are_rejected_atomically() -> Result<()> {
        let identifier = uuid::Uuid::new_v4().to_string();
        assert!(
            threads(
                &json!({"data":[{"id":identifier},{"id":"invalid"}]}),
                "remote:a"
            )
            .is_err()
        );
        assert!(
            threads(
                &json!({"data":[{"id":identifier,"hostId":"remote:b"}]}),
                "remote:a"
            )
            .is_err()
        );
        assert_eq!(
            threads(
                &json!({"data":[{"id":identifier},{"id":identifier}]}),
                "remote:a"
            )?
            .len(),
            1
        );
        Ok(())
    }

    #[test]
    fn recent_threads_are_observed_while_older_pages_are_still_scanning() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            sync_titles: false,
            ..Config::default()
        })?;
        let identifier = uuid::Uuid::new_v4().to_string();
        let first = json!({"expected":null,"data":[],"nextCursor":"next"});
        scan_runtime_page(
            &app,
            &mut Page(first),
            &mut HashMap::new(),
            crate::model::now(),
        )?;
        let page = json!({"expected":"next","data":[],"nextCursor":"older",
            "head":[{"id":identifier,"name":"新近会话"}]});
        scan_runtime_page(
            &app,
            &mut Page(page),
            &mut HashMap::new(),
            crate::model::now(),
        )?;
        assert_eq!(app.store.sessions()?[0]["id"], identifier);
        assert!(app.health.lock().unwrap()["scanComplete"] == false);
        Ok(())
    }

    #[test]
    fn expired_server_cursor_restarts_scanning_without_deleting_saved_data() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            sync_titles: false,
            ..Config::default()
        })?;
        let identifier = uuid::Uuid::new_v4().to_string();
        let first = json!({"expected":null,"data":[{"id":identifier}],"nextCursor":"expired"});
        scan_runtime_page(
            &app,
            &mut Page(first),
            &mut HashMap::new(),
            crate::model::now(),
        )?;
        let invalid =
            json!({"expected":"expired","error":{"code":-32602,"message":"Invalid cursor"}});
        assert!(
            scan_runtime_page(
                &app,
                &mut Page(invalid),
                &mut HashMap::new(),
                crate::model::now()
            )
            .is_err()
        );
        assert!(Progress::load(&app)?.cursor.is_none());
        assert_eq!(app.store.sessions()?.len(), 1);
        let page = json!({"expected":null,"data":[{"id":identifier}],"nextCursor":null});
        scan_runtime_page(
            &app,
            &mut Page(page),
            &mut HashMap::new(),
            crate::model::now(),
        )?;
        assert!(app.is_active(&identifier)?);
        Ok(())
    }
}
