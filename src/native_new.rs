use crate::{config::Config, remote::hosts};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub(crate) fn directory(project: &Value) -> Result<&str> {
    let paths = project["paths"].as_array().context("远程项目缺少目录")?;
    ensure!(paths.len() == 1, "原生新建仅支持单目录项目，请在 Codex 中选择目录");
    let path = paths[0].as_str().context("远程项目目录无效")?;
    ensure!(path.starts_with('/') && !path.chars().any(char::is_control),
        "远程项目目录必须为绝对路径且不含控制字符");
    Ok(path)
}

pub(crate) fn remote_url(config: &Config, project: &Value) -> Result<String> {
    let identifier = project["hostId"].as_str().context("远程项目缺少主机")?;
    let hosts = hosts::registered(config)?;
    let host = hosts.iter().find(|host| host.host_id == identifier)
        .context("远程主机已移除，请刷新")?;
    let alias = host.alias.as_deref().filter(|alias| !alias.trim().is_empty())
        .context("此连接没有 SSH 别名，请先在 Codex 设置 → 连接中登记 SSH 别名，再刷新看板")?;
    ensure!(alias.trim() == alias && !alias.starts_with('-')
        && !alias.chars().any(char::is_control), "SSH 别名无效，请在 Codex 中核对");
    ensure!(hosts.iter().filter(|host| host.alias.as_deref() == Some(alias)).count() == 1,
        "多个原生连接使用同一 SSH 别名，请先在 Codex 中保留唯一连接");
    let mut url = url::Url::parse("codex://settings/connections/ssh/add")?;
    url.query_pairs_mut().append_pair("name", alias)
        .append_pair("projectPath", directory(project)?)
        .append_pair("enabled", "true").append_pair("activate", "true");
    Ok(url.to_string())
}
