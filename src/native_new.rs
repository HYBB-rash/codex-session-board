use crate::{config::Config, remote::hosts};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) fn directory(project: &Value) -> Result<&str> {
    let paths = project["paths"].as_array().context("远程项目缺少目录")?;
    ensure!(paths.len() == 1, "原生新建仅支持单目录项目，请在 Codex 中选择目录");
    let path = paths[0].as_str().context("远程项目目录无效")?;
    ensure!(path.starts_with('/') && !path.chars().any(char::is_control),
        "远程项目目录必须为绝对路径且不含控制字符");
    Ok(path)
}

pub(crate) fn registered_alias(config: &Config, identifier: &str) -> Result<String> {
    let state = read_registry(config)?;
    registered_alias_in(&state, identifier)
}

fn registered_alias_in(state: &Value, identifier: &str) -> Result<String> {
    let hosts = hosts::registered_in(state)?;
    let host = hosts.iter().find(|host| host.host_id == identifier)
        .context("远程主机已移除或未登记，请刷新")?;
    let alias = host.alias.as_deref().filter(|alias| !alias.trim().is_empty())
        .context("这条 Codex 连接未绑定 SSH 别名。请在 Codex 侧栏选择原主机和项目，再新建会话。通过别名添加可能产生第二条连接。")?;
    ensure!(alias.trim() == alias && !alias.starts_with('-')
        && !alias.chars().any(char::is_control), "SSH 别名无效，请在 Codex 中核对");
    ensure!(hosts.iter().filter(|host| host.alias.as_deref() == Some(alias)).count() == 1,
        "多个原生连接使用同一 SSH 别名，请先在 Codex 中保留唯一连接");
    Ok(alias.to_owned())
}

pub(crate) fn remote_url(config: &Config, project: &Value) -> Result<String> {
    let identifier = project["hostId"].as_str().context("远程项目缺少主机")?;
    let alias = registered_alias(config, identifier)?;
    alias_url(&alias, directory(project)?)
}

pub(crate) enum Navigation {
    Preview(Value),
    Ready(ApprovedNavigation),
}

pub(crate) struct ApprovedNavigation {
    token: String,
    url: String,
}

impl ApprovedNavigation {
    pub(crate) fn verify(&self, config: &Config, project: &Value) -> Result<()> {
        let current = NavigationChanges::read(config, project)?;
        ensure!(current.token == self.token, "Codex 连接或账号已改变，请重新核对新建授权");
        Ok(())
    }

    pub(crate) fn url(self) -> String {
        self.url
    }
}

struct NavigationChanges {
    alias: String,
    path: String,
    token: String,
    changes: Vec<String>,
}

impl NavigationChanges {
    fn read(config: &Config, project: &Value) -> Result<Self> {
        let identifier = project["hostId"].as_str().context("远程项目缺少主机")?;
        let path = directory(project)?;
        let state = read_registry(config)?;
        let alias = registered_alias_in(&state, identifier)?;
        let (owner, auto_connect) = hosts::auto_connect_settings(&state)?;
        let auto = &auto_connect[identifier];
        let projects: Vec<_> = state["remote-projects"].as_array().into_iter().flatten()
            .filter(|row| row["hostId"] == identifier).collect();
        let mut changes = Vec::new();
        if project["origin"] == "targetLegacy" {
            changes.push("将远端旧项目登记到远端项目目录".to_owned());
        }
        if !projects.iter().any(|row| row["remotePath"] == path) {
            changes.push(format!("在 Codex 中为 {alias} 登记目录 {path}"));
        }
        if *auto != true {
            changes.push(format!("开启 {alias} 的自动连接"));
        }
        let token = format!("{:x}", Sha256::digest(serde_json::to_vec(
            &json!([owner,identifier,alias,path,auto,projects,project["id"]]))?));
        Ok(Self { alias, path: path.to_owned(), token, changes })
    }
}

pub(crate) fn navigation(config: &Config, project: &Value, acknowledged: &Value) -> Result<Navigation> {
    let plan = NavigationChanges::read(config, project)?;
    if plan.changes.is_empty() || *acknowledged == plan.token {
        let url = alias_url(&plan.alias, &plan.path)?;
        return Ok(Navigation::Ready(ApprovedNavigation { token: plan.token, url }));
    }
    Ok(Navigation::Preview(json!({"consent":{"token":plan.token,
        "summary":format!("{} · {}",plan.alias,plan.path),"changes":plan.changes}})))
}

fn read_registry(config: &Config) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(&config.sidebar_state_file)?)?)
}

pub(crate) fn alias_url(alias: &str, path: &str) -> Result<String> {
    let mut url = url::Url::parse("codex://settings/connections/ssh/add")?;
    url.query_pairs_mut().append_pair("name", alias)
        .append_pair("projectPath", path)
        .append_pair("enabled", "true").append_pair("activate", "true");
    Ok(url.to_string())
}
