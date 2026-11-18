use super::{App, Plan, digest};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

const CONNECTIONS: &str = "codex-managed-remote-connections";
const PROJECTS: &str = "remote-projects";
use crate::remote::hosts::auto_connect_settings;

pub(super) fn read(app: &App) -> Result<Value> {
    let state: Value = serde_json::from_slice(&std::fs::read(&app.config.sidebar_state_file)?)?;
    ensure!(state.is_object() && [CONNECTIONS, PROJECTS].iter().all(|key|
        state[key].is_null() || state[key].is_array()), "Codex 登记格式无效");
    Ok(state)
}

pub(super) fn fingerprint(state: &Value, excluding: &str) -> Result<String> {
    let rows = |key| state[key].as_array().into_iter().flatten()
        .filter(|row| row["hostId"] != excluding).cloned().collect::<Vec<_>>();
    let (owner, settings) = auto_connect_settings(state)?;
    let mut auto = settings.as_object().cloned().unwrap_or_default();
    auto.remove(excluding);
    let sorted: std::collections::BTreeMap<_, _> = auto.into_iter().collect();
    Ok(digest(&serde_json::to_vec(&json!([
        owner.unwrap_or("signed-out"), rows(CONNECTIONS), rows(PROJECTS), sorted
    ]))?))
}

pub(super) fn reserve(state: &Value, alias: &str) -> Result<()> {
    let identifier = identifier(alias);
    let (_, auto) = auto_connect_settings(state)?;
    ensure!(auto.get(&identifier).is_none() &&
        [CONNECTIONS, PROJECTS].iter().all(|key| rows(state, key, &identifier).next().is_none()),
        "该别名仍有历史 Codex 登记，请先在 Codex 核对");
    Ok(())
}

pub(super) fn ready(app: &App, plan: &Plan, allow_partial: bool) -> Result<bool> {
    let state = read(app)?;
    let identifier = identifier(&plan.alias);
    ensure!(fingerprint(&state, &identifier)? == plan.registry,
        "其他 Codex 登记已发生变更，请重新预览");
    if !allow_partial {
        reserve(&state, &plan.alias)?;
        return Ok(false);
    }
    let matching = state[CONNECTIONS].as_array().into_iter().flatten()
        .filter(|row| row["hostId"] == identifier || row["alias"] == plan.alias)
        .collect::<Vec<_>>();
    let present = match matching.as_slice() {
        [] => false,
        [row] => {
            ensure!(row["hostId"] == identifier && row["alias"] == plan.alias
                && row["hostname"].is_null() && row["sshPort"].is_null() && row["identity"].is_null(),
                "Codex 已登记不同的同名连接，请在客户端核对");
            true
        }
        _ => anyhow::bail!("Codex 出现重复连接，请在客户端核对"),
    };
    let (_, auto) = auto_connect_settings(&state)?;
    let auto = &auto[&identifier];
    let projects = rows(&state, PROJECTS, &identifier).collect::<Vec<_>>();
    ensure!((auto.is_null() || *auto == true) && projects.len() <= 1
        && projects.iter().all(|row| row["remotePath"] == plan.target.directory),
        "Codex 中的自动连接或目录与已同意的改动不同");
    Ok(present && *auto == true && projects.len() == 1)
}

fn identifier(alias: &str) -> String {
    format!("remote-ssh-discovered:{alias}")
}

fn rows<'state>(state: &'state Value, key: &str, identifier: &'state str) -> impl Iterator<Item = &'state Value> {
    state.get(key).and_then(Value::as_array).into_iter().flatten()
        .filter(move |row| row["hostId"] == identifier)
}
