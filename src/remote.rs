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

pub fn scan_host(app: &App, reader: &mut dyn Rpc, host: &str, label: &str) -> Result<usize> {
    let mut cursor = Value::Null;
    let mut cursors = HashSet::new();
    let mut identifiers = HashSet::new();
    for _ in 0..app.config.max_pages {
        let response = reader.call(
            "list_threads",
            json!({
                "mode":"history","source":"codex","hostId":host,"archived":false,
                "limit":app.config.page_size.min(50),"cursor":cursor
            }),
        )?;
        let threads = response["threads"]
            .as_array()
            .context("远程会话列表格式无效")?;
        app.store.discover_remote(host, label, threads)?;
        identifiers.extend(
            threads
                .iter()
                .filter_map(|thread| thread["id"].as_str())
                .map(str::to_owned),
        );
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
    let catalog = projects(connect()?.as_mut())?;
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
