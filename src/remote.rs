use crate::{
    app::App,
    rpc::{Rpc, SidebarConnection},
    store::remote::identity,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

fn projects(reader: &mut dyn Rpc) -> Result<Vec<Value>> {
    let response = reader.call("list_projects", json!({}))?;
    let projects = response["projects"]
        .as_array()
        .context("远程项目列表格式无效")?;
    projects
        .iter()
        .filter(|project| project["projectKind"] == "remote")
        .map(|project| {
            let host = project["hostId"]
                .as_str()
                .filter(|host| !host.is_empty() && *host != "local")
                .context("远程项目缺少主机")?;
            let identifier = project["projectId"]
                .as_str()
                .filter(|value| !value.is_empty())
                .context("远程项目缺少 ID")?;
            let label = project["label"].as_str().context("远程项目缺少名称")?;
            let path = project["path"].as_str().context("远程项目缺少路径")?;
            let host_label = project["hostDisplayName"].as_str().unwrap_or(host);
            Ok(
                json!({"id":identity(host,identifier),"nativeId":identifier,"remote":true,
            "hostId":host,"hostLabel":host_label,"label":format!("{label} · {host_label}"),
            "paths":[path]}),
            )
        })
        .collect()
}

fn registered_projects(app: &App) -> Option<HashSet<String>> {
    let bytes = std::fs::read(&app.config.sidebar_state_file).ok()?;
    let state: Value = serde_json::from_slice(&bytes).ok()?;
    state["remote-projects"]
        .as_array()?
        .iter()
        .map(|project| {
            let host = project["hostId"]
                .as_str()
                .filter(|value| !value.is_empty())?;
            let id = project["id"].as_str().filter(|value| !value.is_empty())?;
            Some(identity(host, id))
        })
        .collect()
}

fn merge_projects(app: &App, fresh: Vec<Value>) -> Result<Vec<Value>> {
    let registered = registered_projects(app);
    let mut projects = BTreeMap::new();
    for mut project in app.store.remote_projects()? {
        let identifier = project["id"]
            .as_str()
            .context("缓存项目缺少 ID")?
            .to_owned();
        if registered
            .as_ref()
            .is_none_or(|ids| ids.contains(&identifier))
        {
            project["cached"] = json!(true);
            projects.insert(identifier, project);
        }
    }
    for project in fresh {
        projects.insert(
            project["id"].as_str().context("项目缺少 ID")?.to_owned(),
            project,
        );
    }
    Ok(projects.into_values().collect())
}

pub fn scan_host(app: &App, reader: &mut dyn Rpc, host: &str, label: &str) -> Result<usize> {
    let mut cursor = Value::Null;
    let mut cursors = HashSet::new();
    let mut identifiers = HashSet::new();
    for _ in 0..app.config.max_pages {
        let mut arguments = json!({
            "mode":"history","source":"codex","hostId":host,"archived":false,
            "limit":app.config.page_size.min(50)
        });
        if !cursor.is_null() {
            arguments["cursor"] = cursor;
        }
        let response = reader.call("list_threads", arguments)?;
        let threads = response["threads"]
            .as_array()
            .context("远程会话列表格式无效")?;
        ensure!(
            threads.iter().all(|thread| thread["hostId"] == host),
            "远程会话主机不匹配"
        );
        let unique: Vec<_> = threads
            .iter()
            .filter(|thread| {
                thread["id"]
                    .as_str()
                    .is_none_or(|id| identifiers.insert(id.to_owned()))
            })
            .cloned()
            .collect();
        app.store.discover_remote(host, label, &unique)?;
        cursor = response.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() || cursor == "" {
            return Ok(identifiers.len());
        }
        let next = cursor.as_str().context("远程分页游标无效")?;
        ensure!(
            cursors.insert(next.to_string()),
            "远程分页游标重复，扫描未完成"
        );
    }
    anyhow::bail!("远程会话扫描达到分页上限，已保留读取结果")
}

pub fn scan(app: &App, connect: &mut impl FnMut() -> Result<Box<dyn Rpc>>) -> Result<()> {
    let catalog = merge_projects(app, projects(connect()?.as_mut())?)?;
    app.store.cache_remote_projects(&catalog)?;
    let hosts: BTreeMap<_, _> = catalog
        .iter()
        .map(|project| {
            (
                project["hostId"].as_str().unwrap_or_default(),
                project["hostLabel"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    let mut reports = Vec::new();
    for (host, label) in hosts {
        let result = connect().and_then(|mut reader| scan_host(app, reader.as_mut(), host, label));
        reports.push(
            json!({"hostId":host,"hostLabel":label,"scanComplete":result.is_ok(),
            "count":result.as_ref().ok(),"error":result.err().map(|error|error.to_string())}),
        );
    }
    app.update_health(json!({"remoteVerified":true,"remoteError":null,
        "remoteProjectWarning":catalog.iter().any(|project|project["cached"]==true),
        "remoteHosts":reports,"remoteLastAt":crate::model::now()}))
}

pub fn refresh(app: &App) -> Result<()> {
    let result = scan(app, &mut || {
        Ok(Box::new(SidebarConnection::reader(
            &app.config,
            Duration::from_secs(app.config.rpc_timeout_seconds),
        )?) as Box<dyn Rpc>)
    });
    if let Err(error) = &result {
        app.update_health(json!({"remoteVerified":false,"remoteError":error.to_string()}))?;
    }
    result
}
