use crate::{app::App, model::STATES, rpc::Rpc};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

pub fn layout(writer: &mut dyn Rpc, app: &App) -> Result<HashMap<String, HashSet<String>>> {
    let response = writer.call("list_threads", json!({"limit":1}))?;
    let aliases = read_aliases(app)?;
    ensure!(
        response["unavailableHosts"]
            .as_array()
            .is_some_and(|hosts| !hosts
                .iter()
                .any(|host| host.as_str() == Some(&app.config.host_id))),
        "侧边栏主机不可用"
    );
    let sections = response["sections"].as_array().context("缺少原生分区")?;
    let mut result = HashMap::new();
    for (state, label) in STATES {
        let identifier = app
            .config
            .status_sections
            .get(state)
            .context("缺少状态分区")?;
        let matches: Vec<_> = sections
            .iter()
            .filter(|section| section["sectionId"] == *identifier)
            .collect();
        ensure!(matches.len() == 1, "状态分区缺失或重复: {label}");
        let section = matches[0];
        ensure!(section["name"] == label, "状态分区已改名: {label}");
        let keys = section["itemKeys"].as_array().context("分区内容格式无效")?;
        result.insert(
            state.to_string(),
            keys.iter()
                .map(|key| member_key(app, key, &aliases))
                .collect::<Result<_>>()?,
        );
    }
    Ok(result)
}

fn member_key(app: &App, key: &Value, aliases: &HashMap<String, String>) -> Result<String> {
    let key = key.as_str().context("分区成员无效")?;
    ensure!(
        !key.starts_with(&item_key(app, "client-new-thread:")) || aliases.contains_key(key),
        "客户端别名尚未保存，等待下一轮同步"
    );
    Ok(aliases.get(key).cloned().unwrap_or_else(|| key.to_owned()))
}

fn item_key(app: &App, identifier: &str) -> String {
    format!("codex:thread:{}:{identifier}", app.config.host_id)
}

fn read_aliases(app: &App) -> Result<HashMap<String, String>> {
    let state: Value =
        serde_json::from_reader(std::fs::File::open(&app.config.sidebar_state_file)?)?;
    let atoms = state["electron-persisted-atom-state"]
        .as_object()
        .context("客户端别名数据无效")?;
    let mut aliases = HashMap::new();
    for (key, value) in atoms {
        if let Some(identifier) = key.strip_prefix("thread-client-id-v1:local%3A") {
            uuid::Uuid::parse_str(identifier).context("客户端会话标识无效")?;
            let alias = value.as_str().context("客户端会话别名无效")?;
            let canonical = item_key(app, identifier);
            let previous = aliases.insert(item_key(app, alias), canonical.clone());
            ensure!(
                previous.is_none_or(|previous| previous == canonical),
                "客户端别名存在冲突"
            );
        }
    }
    Ok(aliases)
}

fn candidates(app: &App, only: Option<&str>) -> Result<Vec<Value>> {
    let active = app
        .active_sessions
        .lock()
        .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?
        .clone();
    let active = active.context("等待完整会话发现后同步分区")?;
    Ok(app
        .store
        .sessions()?
        .into_iter()
        .filter(|session| {
            session["id"].as_str().is_some_and(|identifier| {
                active.contains(identifier) && only.is_none_or(|only| only == identifier)
            })
        })
        .collect())
}

pub fn move_one(app: &App, writer: &mut dyn Rpc, identifier: &str) -> Result<Value> {
    ensure!(
        !candidates(app, Some(identifier))?.is_empty(),
        "会话不在当前发现范围"
    );
    uuid::Uuid::parse_str(identifier).context("无效会话 ID")?;
    let (state, _) = app.store.state_revision(identifier)?;
    let target = app
        .config
        .status_sections
        .get(&state)
        .context("缺少目标状态分区")?;
    ensure!(app.is_active(identifier)?, "会话已离开当前发现范围");
    writer.call("move_thread_to_sidebar_section",
        json!({"threadId":identifier,"hostId":app.config.host_id,"source":"codex","sectionId":target}))?;
    Ok(json!({"pending":1,"error":null}))
}

fn ordered_candidates(app: &App, only: Option<&str>) -> Result<Vec<Value>> {
    let mut sessions = candidates(app, only)?;
    if !sessions.is_empty() {
        let offset = app.section_cursor.fetch_add(1, Ordering::Relaxed) % sessions.len();
        sessions.rotate_left(offset);
    }
    Ok(sessions)
}

pub fn reconcile(app: &App, writer: &mut dyn Rpc, only: Option<&str>) -> Result<Value> {
    let sessions = ordered_candidates(app, only)?;
    let before = layout(writer, app)?;
    let mut failures = Vec::new();
    for session in &sessions {
        let identifier = session["id"].as_str().context("缺少会话 ID")?;
        uuid::Uuid::parse_str(identifier).context("无效会话 ID")?;
        let (state, version) = app.store.state_revision(identifier)?;
        if before
            .get(&state)
            .is_some_and(|keys| keys.contains(&item_key(app, identifier)))
        {
            continue;
        }
        let target = app
            .config
            .status_sections
            .get(&state)
            .context("缺少目标状态分区")?;
        if !app.is_active(identifier)? || app.store.state_revision(identifier)? != (state, version)
        {
            continue;
        }
        if let Err(error) = writer.call("move_thread_to_sidebar_section",
            json!({"threadId":identifier,"hostId":app.config.host_id,"source":"codex","sectionId":target})) {
            failures.push(format!("{identifier}: {error}"));
        }
    }
    let after = layout(writer, app)?;
    let pending = candidates(app, only)?
        .iter()
        .filter(|session| {
            let state = session["state"].as_str().unwrap_or("");
            let identifier = session["id"].as_str().unwrap_or("");
            !after
                .get(state)
                .is_some_and(|keys| keys.contains(&item_key(app, identifier)))
        })
        .count();
    Ok(
        json!({"pending":pending,"error":if pending == 0 || failures.is_empty() {Value::Null} else {json!(failures.join("; "))}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use anyhow::bail;

    const FIRST: &str = "00000000-0000-4000-8000-000000000001";
    const SECOND: &str = "00000000-0000-4000-8000-000000000002";

    struct Native<'store> {
        sections: Value,
        calls: Vec<Value>,
        fail: Option<&'static str>,
        lost_receipt: bool,
        concurrent: Option<&'store crate::store::Store>,
    }

    impl Rpc for Native<'_> {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            match method {
                "list_threads" => Ok(json!({"sections":self.sections,"unavailableHosts":[]})),
                "move_thread_to_sidebar_section" => self.move_thread(params),
                _ => bail!("不允许的工具: {method}"),
            }
        }
    }

    impl Native<'_> {
        fn move_thread(&mut self, params: Value) -> Result<Value> {
            self.calls.push(params.clone());
            if self.fail == params["threadId"].as_str() {
                bail!("受控失败");
            }
            let key = format!(
                "codex:thread:local:{}",
                params["threadId"].as_str().unwrap()
            );
            for section in self.sections.as_array_mut().unwrap() {
                section["itemKeys"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|item| item != &key);
                if section["sectionId"] == params["sectionId"] {
                    section["itemKeys"].as_array_mut().unwrap().push(json!(key));
                }
            }
            if let Some(store) = self.concurrent.take() {
                let (_, version) = store.state_revision(FIRST)?;
                store.set_state(FIRST, Some("completed"), "并发的新选择", version, Some(0))?;
            }
            if self.lost_receipt {
                bail!("回执丢失");
            }
            Ok(json!({"ok":true}))
        }
    }

    fn native_sections() -> Value {
        STATES
            .iter()
            .enumerate()
            .map(|(index, (_, label))| {
                json!({"sectionId":uuid::Uuid::from_u128(index as u128 + 100).to_string(),
                "name":label,"itemKeys":[]})
            })
            .collect::<Vec<_>>()
            .into()
    }

    fn fixture() -> Result<(tempfile::TempDir, App, Native<'static>)> {
        let directory = tempfile::tempdir()?;
        let sidebar_state_file = directory.path().join("client-state.json");
        std::fs::write(
            &sidebar_state_file,
            r#"{"electron-persisted-atom-state":{}}"#,
        )?;
        let sections = native_sections();
        let status_sections = STATES
            .iter()
            .zip(sections.as_array().unwrap())
            .map(|((state, _), section)| {
                (
                    state.to_string(),
                    section["sectionId"].as_str().unwrap().into(),
                )
            })
            .collect();
        let app = App::new(Config {
            data_dir: directory.path().into(),
            sync_titles: false,
            sync_sections: true,
            status_sections,
            sidebar_state_file,
            ..Config::default()
        })?;
        app.discover(
            &[
                json!({"id":FIRST,"name":"同名"}),
                json!({"id":SECOND,"name":"同名"}),
            ],
            true,
        )?;
        let native = Native {
            sections,
            calls: Vec::new(),
            fail: None,
            lost_receipt: false,
            concurrent: None,
        };
        Ok((directory, app, native))
    }

    #[test]
    fn title_disabled_moves_by_id_preserves_unrelated_items_and_is_idempotent() -> Result<()> {
        let (_directory, app, mut native) = fixture()?;
        native.sections[0]["itemKeys"] = json!(["codex:project:kept", "chatgpt:conversation:kept"]);
        app.store
            .set_state(FIRST, Some("to_archive"), "保留备注", 0, Some(0))?;
        let report = reconcile(&app, &mut native, None)?;
        assert_eq!(report["pending"], 0);
        assert_eq!(native.calls.len(), 2);
        assert_eq!(native.calls[0]["source"], "codex");
        assert!(native.calls.iter().all(|call| call["hostId"] == "local"));
        assert_eq!(app.store.note(FIRST)?, "保留备注");
        assert!(app.store.pending_titles(None)?.is_empty());
        assert_eq!(native.sections[0]["itemKeys"][0], "codex:project:kept");
        assert_eq!(
            native.sections[0]["itemKeys"][1],
            "chatgpt:conversation:kept"
        );
        reconcile(&app, &mut native, None)?;
        assert_eq!(native.calls.len(), 2);
        Ok(())
    }

    #[test]
    fn failure_does_not_starve_other_threads_and_restart_reconciles_saved_state() -> Result<()> {
        let (directory, app, mut native) = fixture()?;
        native.fail = Some(FIRST);
        let report = reconcile(&app, &mut native, None)?;
        assert_eq!(report["pending"], 1);
        assert!(report["error"].as_str().unwrap().contains("受控失败"));
        let config = app.config.clone();
        drop(app);
        let restarted = App::new(config)?;
        restarted.discover(&[json!({"id":FIRST}), json!({"id":SECOND})], true)?;
        native.fail = None;
        assert_eq!(reconcile(&restarted, &mut native, None)?["pending"], 0);
        assert_eq!(native.calls.len(), 3);
        assert!(directory.path().join("status.sqlite3").exists());
        Ok(())
    }

    #[test]
    fn lost_receipt_is_confirmed_by_readback_without_duplicate_move() -> Result<()> {
        let (_directory, app, mut native) = fixture()?;
        native.lost_receipt = true;
        assert_eq!(reconcile(&app, &mut native, None)?["pending"], 0);
        assert_eq!(native.calls.len(), 2);
        reconcile(&app, &mut native, None)?;
        assert_eq!(native.calls.len(), 2);
        Ok(())
    }

    #[test]
    fn concurrent_new_state_stays_pending_then_converges() -> Result<()> {
        let (_directory, app, mut native) = fixture()?;
        native.concurrent = Some(&app.store);
        assert_eq!(reconcile(&app, &mut native, None)?["pending"], 1);
        assert_eq!(app.store.state_revision(FIRST)?.0, "completed");
        assert_eq!(reconcile(&app, &mut native, None)?["pending"], 0);
        Ok(())
    }

    #[test]
    fn history_state_change_without_version_change_and_missing_threads_are_reconciled() -> Result<()>
    {
        let (_directory, app, mut native) = fixture()?;
        reconcile(&app, &mut native, None)?;
        let before = app.store.state_revision(FIRST)?;
        app.store.observe(
            FIRST,
            &json!({"id":"old","status":"completed","hasFinal":true}),
            None,
        )?;
        let after = app.store.state_revision(FIRST)?;
        assert_eq!(before.1, after.1);
        assert_ne!(before.0, after.0);
        app.discover(&[json!({"id":FIRST})], true)?;
        assert_eq!(reconcile(&app, &mut native, None)?["pending"], 0);
        assert_eq!(native.calls.len(), 3);
        Ok(())
    }

    #[test]
    fn renamed_or_missing_destination_fails_before_moving() -> Result<()> {
        let (_directory, app, mut native) = fixture()?;
        native.sections[0]["name"] = json!("被改名");
        assert!(reconcile(&app, &mut native, None).is_err());
        assert!(native.calls.is_empty());
        Ok(())
    }

    #[test]
    fn native_loaded_thread_aliases_are_read_back_without_repeated_moves() -> Result<()> {
        let (_directory, app, mut native) = fixture()?;
        let alias = "client-new-thread:loaded";
        std::fs::write(
            &app.config.sidebar_state_file,
            serde_json::to_vec(&json!({
                "electron-persisted-atom-state":{format!("thread-client-id-v1:local%3A{FIRST}"):alias}
            }))?,
        )?;
        native.sections[0]["itemKeys"] = json!([item_key(&app, alias)]);
        let report = reconcile(&app, &mut native, None)?;
        assert_eq!(report["pending"], 0);
        assert_eq!(native.calls.len(), 1);
        assert_eq!(native.calls[0]["threadId"], SECOND);
        reconcile(&app, &mut native, None)?;
        assert_eq!(native.calls.len(), 1);
        Ok(())
    }

    #[test]
    fn exhausted_connection_rotates_next_round_past_the_failing_thread() -> Result<()> {
        struct Round<'native> {
            native: &'native mut Native<'static>,
            exhausted: bool,
        }
        impl Rpc for Round<'_> {
            fn call(&mut self, method: &str, params: Value) -> Result<Value> {
                if self.exhausted
                    || method == "move_thread_to_sidebar_section" && params["threadId"] == FIRST
                {
                    self.exhausted = true;
                    bail!("整轮截止时间已过");
                }
                self.native.call(method, params)
            }
        }
        let (_directory, app, mut native) = fixture()?;
        assert!(
            reconcile(
                &app,
                &mut Round {
                    native: &mut native,
                    exhausted: false
                },
                None
            )
            .is_err()
        );
        assert!(
            reconcile(
                &app,
                &mut Round {
                    native: &mut native,
                    exhausted: false
                },
                None
            )
            .is_err()
        );
        assert_eq!(native.calls.len(), 1);
        assert_eq!(native.calls[0]["threadId"], SECOND);
        Ok(())
    }

    #[test]
    fn removal_after_candidate_snapshot_prevents_the_move() -> Result<()> {
        struct Removal<'app> {
            app: &'app App,
            native: Native<'static>,
        }
        impl Rpc for Removal<'_> {
            fn call(&mut self, method: &str, params: Value) -> Result<Value> {
                if method == "list_threads" {
                    self.app.discover(&[json!({"id":SECOND})], true)?;
                }
                self.native.call(method, params)
            }
        }
        let (_directory, app, native) = fixture()?;
        let mut writer = Removal { app: &app, native };
        assert_eq!(reconcile(&app, &mut writer, None)?["pending"], 0);
        assert_eq!(writer.native.calls.len(), 1);
        assert_eq!(writer.native.calls[0]["threadId"], SECOND);
        Ok(())
    }

    #[test]
    fn busy_sidebar_sync_saves_promptly_with_persistent_pending_state() -> Result<()> {
        let (_directory, app, _native) = fixture()?;
        let _guard = app.section_lock.lock().unwrap();
        let started = std::time::Instant::now();
        let response = app.request(
            "/api/state",
            Some(&json!({"id":FIRST,
            "state":"completed","note":"保存成功","version":0,"inputVersion":0})),
        )?;
        assert!(started.elapsed() < std::time::Duration::from_millis(150));
        assert_eq!(response["sectionsPending"], 1);
        assert_eq!(app.store.state_revision(FIRST)?.0, "completed");
        Ok(())
    }

    #[test]
    fn immediate_move_uses_latest_saved_state_and_waits_for_independent_confirmation() -> Result<()>
    {
        let (_directory, app, mut native) = fixture()?;
        app.store
            .set_state(FIRST, Some("completed"), "", 0, Some(0))?;
        let result = move_one(&app, &mut native, FIRST)?;
        assert_eq!(result["pending"], 1);
        assert_eq!(native.calls.len(), 1);
        assert_eq!(
            native.calls[0]["sectionId"],
            app.config.status_sections["completed"]
        );
        assert_eq!(reconcile(&app, &mut native, Some(FIRST))?["pending"], 0);
        app.discover(&[json!({"id":SECOND})], true)?;
        assert!(move_one(&app, &mut native, FIRST).is_err());
        assert_eq!(native.calls.len(), 1);
        Ok(())
    }
}
