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
