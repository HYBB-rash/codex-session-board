use anyhow::{Context, Result, ensure};
use crate::{app::App, config::{self, Config}, remote::hosts::{self, Host}};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};

fn isolated(settings: &Config, host: &Host, directory: &Path, socket: &Path) -> Result<Config> {
    let mut settings = settings.clone();
    let mut state: Value = serde_json::from_slice(&std::fs::read(&settings.sidebar_state_file)?)?;
    let atom = state["electron-renderer-content-owner"].as_str()
        .map(|owner| format!("electron-owner-atoms:{owner}"))
        .unwrap_or_else(|| "electron-persisted-atom-state".into());
    for account in state[&atom]["sidebar-custom-sections-v3"].as_object_mut()
        .context("缺少账号分区")?.values_mut() {
        for section in account["sections"].as_array_mut().context("缺少分区列表")? {
            if let Some(hosts) = section["hostSectionIds"].as_object_mut() {
                hosts.remove(&host.host_id);
            }
        }
    }
    settings.data_dir = directory.join("root");
    settings.sidebar_state_file = directory.join("state.json");
    settings.sidebar_socket = socket.into();
    if !settings.sidebar_state_file.exists() {
        config::write_json(&settings.sidebar_state_file,&state,false)?;
    }
    settings.sync_titles = false;
    settings.auto_archive_seconds = None;
    settings.remote_auto_archive_seconds = None;
    settings.native_sections = true;
    settings.rpc_timeout_seconds = 30;
    settings.ssh_command = vec!["ssh".into(),"-o".into(),
        format!("IdentityAgent={}",std::env::var("SSH_AUTH_SOCK")?),
        "-o".into(),"IdentitiesOnly=no".into(),"-o".into(),"BatchMode=yes".into()];
    settings.proxy_command = host.proxy(&settings)?;
    settings.remote_binding = Some(hosts::source::Binding::prepare(host,&settings)?);
    settings.data_dir = host.directory(&settings);
    settings.host_id = host.host_id.clone();
    Ok(settings)
}

fn inspect(app: &App) -> Result<Value> {
    ensure!(app.store.sessions()?.is_empty(),"实验数据库不得包含业务会话");
    let database = app.store.connection()?;
    let saved: String = database.query_row("SELECT value FROM settings WHERE key=?",
        [format!("native_mapping:v1:{}",app.config.host_id)],|row| row.get(0))?;
    let saved: Value = serde_json::from_str(&saved)?;
    let logical = saved["logical"].as_object().context("映射无效")?;
    for (_,_,target) in app.config.section_bindings() {
        ensure!(logical.values().filter(|value| *value == target).count() == 1,"映射未完整确认");
    }
    ensure!(saved["probe"].is_null(),"专用空会话尚未清理");
    let count: i64 = database.query_row("SELECT count(*) FROM settings WHERE key LIKE 'native_probe:%'",
        [],|row| row.get(0))?;
    ensure!(count == 1,"首次校准只应创建一个专用空会话");
    Ok(json!({"mapping":logical,"probeCount":count,"businessRows":0,"cleanupConfirmed":true}))
}

pub fn run(settings: &Config, host_id: &str, directory: &Path, socket: &Path, cleanup: Option<&str>) -> Result<Value> {
    let host = hosts::registered(settings)?.into_iter()
        .find(|host| host.host_id == host_id).context("主机未登记")?;
    std::fs::create_dir_all(directory)?;
    let mut config = isolated(settings,&host,directory,socket)?;
    if let Some(identifier) = cleanup {
        let app = App::new(config)?;
        ensure!(app.store.sessions()?.is_empty(),"实验数据库不得包含业务会话");
        let result = app.cleanup_native_probe(identifier);
        app.shutdown()?;
        result?;
        return Ok(json!({"cleanupConfirmed":true,"thread":identifier,"businessRows":0}));
    }
    let before = std::fs::read(&config.sidebar_state_file)?;
    let mut reports = Vec::new();
    for round in ["cold","restart"] {
        if round == "restart" {
            config = isolated(settings,&host,directory,socket)?;
        }
        let app = App::new(config.clone())?;
        ensure!(app.store.sessions()?.is_empty(),"实验数据库不得包含业务会话");
        let started = Instant::now();
        let result = app.refresh_native().and_then(|()| inspect(&app));
        let elapsed = started.elapsed().as_secs_f64()*1000.0;
        app.shutdown()?;
        reports.push(json!({"round":round,"elapsedMs":elapsed,"result":result?}));
    }
    ensure!(std::fs::read(&config.sidebar_state_file)? == before,"实验未依赖客户端文件写回");
    Ok(json!({"passed":true,"rounds":reports}))
}
