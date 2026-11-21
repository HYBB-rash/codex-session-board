use crate::{
    app::App,
    config, observer,
    rpc::{self, Rpc},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::Ordering;

#[derive(Default, Serialize, Deserialize)]
struct Progress {
    cursor: Option<String>,
    cursors: BTreeSet<String>,
    identifiers: BTreeSet<String>,
    #[serde(default)]
    restoration_errors: BTreeMap<String, String>,
}

impl Progress {
    fn save(&self, app: &App) -> Result<()> {
        let _operation = app.operation()?;
        config::write_json(
            &app.config.data_dir.join("scan-progress.json"),
            &serde_json::to_value(self)?,
            true,
        )
    }

    fn preserve_restoration(&self, app: &App) -> Result<()> {
        let mut saved = Self::load(app)?;
        if saved.restoration_errors != self.restoration_errors {
            saved.restoration_errors = self.restoration_errors.clone();
            saved.save(app)?;
        }
        Ok(())
    }

    fn load(app: &App) -> Result<Self> {
        let path = app.config.data_dir.join("scan-progress.json");
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn page(&self, app: &App, reader: &mut dyn Rpc) -> Result<Value> {
        let mut params = rpc::thread_list_params(app.config.page_size, json!(self.cursor));
        params["sourceKinds"] = json!([]);
        params["currentAccountOnly"] = json!(false);
        let result = reader.call("thread/list", params);
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
            Self {
                restoration_errors: self.restoration_errors.clone(),
                ..Self::default()
            }.save(app)?;
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

    fn reconcile_restoration(
        &mut self,
        app: &App,
        reader: &mut dyn Rpc,
        entries: &[Value],
        archived: &HashMap<String, i64>,
    ) -> Option<String> {
        self.restoration_errors.retain(|identifier, _| archived.contains_key(identifier));
        let candidates: Vec<_> = entries.iter().filter_map(|entry| {
            let identifier = entry["id"].as_str()?;
            Some((identifier.to_owned(), *archived.get(identifier)?))
        }).collect();
        if let Err(error) = restore_active(app, reader, &candidates, &mut self.restoration_errors) {
            let message = format!("归档恢复待核对：{error}");
            for pending in self.restoration_errors.values_mut() {
                *pending = message.clone();
            }
            for (identifier, _) in candidates {
                self.restoration_errors.insert(identifier, message.clone());
            }
        }
        (!self.restoration_errors.is_empty()).then(|| {
            self.restoration_errors.values().cloned().collect::<BTreeSet<_>>()
                .into_iter().collect::<Vec<_>>().join("; ")
        })
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

fn restore_active(
    app: &App,
    reader: &mut dyn Rpc,
    candidates: &[(String, i64)],
    pending: &mut BTreeMap<String, String>,
) -> Result<()> {
    if candidates.is_empty() && pending.is_empty() {
        return Ok(());
    }
    let (listed, complete) =
        rpc::collect_threads(reader, app.config.page_size, app.config.max_pages, true)?;
    ensure!(complete, "远程归档列表尚未完整读取");
    let confirmed = threads(&json!({"data":listed}), &app.config.host_id)?;
    let still_archived: HashSet<_> = confirmed
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect();
    let _operation = app.operation()?;
    pending.retain(|identifier, _| !still_archived.contains(identifier.as_str()));
    for (identifier, version) in candidates {
        ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
        if !still_archived.contains(identifier.as_str()) {
            app.store.restore_confirmed_active(identifier, *version)?;
        }
        pending.remove(identifier);
    }
    Ok(())
}

fn reclaim_probes(app: &App, reader: &mut dyn Rpc, entries: &[Value]) -> Result<HashSet<String>> {
    let host = &app.config.host_id;
    let mut owned = app.mapping_store.native_probes(host)?;
    let candidates: Vec<_> = entries.iter().filter_map(|thread| thread["id"].as_str())
        .filter(|identifier| owned.contains(*identifier)).collect();
    if candidates.is_empty() || !app.config.native_sections { return Ok(owned); }
    let (archived, complete) = rpc::archived_thread_ids(reader,app.config.page_size,app.config.max_pages)?;
    ensure!(complete, "校准会话归档状态尚未完整读取");
    let mut verifier = app.native_reader()?;
    for identifier in candidates {
        if archived.contains(identifier) { continue; }
        let response = verifier.call("thread/read",
            json!({"threadId":identifier,"includeTurns":false}))?;
        ensure!(response["thread"]["id"] == identifier, "校准会话读回身份不匹配");
        if app.mapping_store.reclaim_native_probe(host, identifier)? { owned.remove(identifier); }
    }
    Ok(owned)
}

fn prepare_catalog(app: &App, reader: &mut dyn Rpc)
    -> Result<Option<crate::native_sections::direct::Catalog>> {
    if !app.config.native_sections { return Ok(None); }
    let _operation = app.operation()?;
    Ok(Some(app.native_catalog(reader)?))
}

pub(crate) fn scan_runtime_page(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut observer::RuntimeRevisions,
    since: f64,
) -> Result<()> {
    let mut progress = Progress::load(app)?;
    let catalog = prepare_catalog(app,reader)?;
    let archived = if app.config.native_sections {
        HashMap::new()
    } else {
        app.store.archived_revisions()?
    };
    let generation = app.native_epoch();
    let continued = progress.cursor.is_some();
    let response = progress.page(app, reader)?;
    ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
    let mut entries = threads(&response, &app.config.host_id)?;
    if continued {
        merge_head(app, reader, &mut entries)?;
    }
    let owned = reclaim_probes(app, reader, &entries)?;
    entries.retain(|thread| !owned.contains(thread["id"].as_str().unwrap_or("")));
    progress.identifiers.retain(|identifier| !owned.contains(identifier));
    let complete = progress.advance(&entries, &response["nextCursor"], app.config.max_pages)?;
    app.store.discover(&entries)?;
    if let Some(catalog) = catalog {
        app.cache_native_page(generation, &catalog, &entries)?;
    }
    let restoration_error = progress.reconcile_restoration(app, reader, &entries, &archived);
    let observed = observer::observe_runtime_threads(app, reader, revisions, since, &entries);
    progress.preserve_restoration(app)?;
    observed?;
    let count = progress.identifiers.len();
    if complete {
        *app.active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))? =
            Some(progress.identifiers.into_iter().collect());
        progress = Progress {
            restoration_errors: progress.restoration_errors,
            ..Progress::default()
        };
    }
    progress.save(app)?;
    app.update_health(json!({"scanComplete":complete,"discovered":count,"lastSuccessAt":crate::model::now(),"error":restoration_error}))
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod restore_tests;

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
            assert!(!self.0.is_null(), "unexpected extra thread/list request");
            assert_eq!(params["cursor"], self.0["expected"]);
            if self.0["disconnected"] == true {
                return Err(std::io::Error::from(std::io::ErrorKind::ConnectionReset).into());
            }
            if self.0["error"].is_object() {
                return Err(rpc::RequestError {
                    method: method.into(),
                    details: self.0["error"].clone(),
                }
                .into());
            }
            let next = if self.0["expected"].is_null() {
                Value::Null
            } else {
                json!({"data":self.0["head"].as_array().cloned().unwrap_or_default()})
            };
            Ok(std::mem::replace(&mut self.0, next))
        }
    }

    fn scan(app: &App, response: Value) -> Result<()> {
        let mut reader = Page(response);
        scan_runtime_page(app, &mut reader, &mut HashMap::new(), crate::model::now())?;
        assert!(reader.0.is_null(), "continued scan did not read the head page");
        Ok(())
    }

    #[test]
    fn failed_remote_page_keeps_its_cursor_while_another_host_completes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let host = |name: &str| {
            App::new(Config {
                data_dir: directory.path().join(name),
                host_id: name.into(),
                sync_titles: false,
                ..Config::default()
            })
        };
        let first = host("mac")?;
        let other = host("other")?;
        let known = uuid::Uuid::new_v4().to_string();
        let new = uuid::Uuid::new_v4().to_string();
        let foreign = uuid::Uuid::new_v4().to_string();
        scan(&first, json!({"data":[{"id":known}],"nextCursor":"continue"}))?;
        assert!(scan(&first, json!({"expected":"continue","disconnected":true})).is_err());
        scan(&other, json!({"data":[{"id":known}]}))?;
        assert!(other.is_active(&known)?);
        assert!(!first.is_active(&known)?);
        let mixed = json!({"expected":"continue","data":[{"id":new},{"id":foreign,"hostId":"other"}]});
        assert!(scan(&first, mixed).is_err());
        let invalid = json!({"expected":"continue","data":[{"id":new},{"id":"not-a-uuid"}]});
        assert!(scan(&first, invalid).is_err());
        assert!(!first.store.contains(&new)?);
        assert!(!first.store.contains(&foreign)?);
        assert!(first.store.contains(&known)?);
        scan(&first, json!({"expected":"continue","data":[]}))?;
        assert!(first.is_active(&known)?);
        assert_eq!(first.store.sessions()?.len(), 1);
        assert!(first.store.contains(&known)?);
        Ok(())
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
        scan(&app, first)?;
        assert!(!app.is_active(&identifier)?);
        drop(app);
        let app = App::new(config)?;
        let bad = json!({"expected":"next","data":[],"nextCursor":"next"});
        assert!(scan(&app, bad).is_err());
        assert_eq!(app.store.sessions()?.len(), 1);
        let final_page = json!({"expected":"next","data":[],"nextCursor":null});
        scan(&app, final_page)?;
        assert!(app.is_active(&identifier)?);
        assert_eq!(app.store.sessions()?.len(), 1);
        assert!(app.store.contains(&identifier)?);
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
        scan(&app, first)?;
        let page = json!({"expected":"next","data":[],"nextCursor":"older",
            "head":[{"id":identifier,"name":"新近会话"}]});
        scan(&app, page)?;
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
        scan(&app, first)?;
        let invalid =
            json!({"expected":"expired","error":{"code":-32602,"message":"Invalid cursor"}});
        assert!(scan(&app, invalid).is_err());
        assert!(Progress::load(&app)?.cursor.is_none());
        assert_eq!(app.store.sessions()?.len(), 1);
        let page = json!({"expected":null,"data":[{"id":identifier}],"nextCursor":null});
        scan(&app, page)?;
        assert!(app.is_active(&identifier)?);
        Ok(())
    }
}
