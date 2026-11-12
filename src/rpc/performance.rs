use super::*;
use crate::{app::App, config::Config, remote::hosts::{self, Host}};
use std::sync::Arc;
use std::os::unix::fs::PermissionsExt;

const SAMPLES: usize = 12;
const BATCHES: usize = 3;

fn distribution(values: &[f64]) -> Result<Value> {
    ensure!(!values.is_empty(), "性能样本为空");
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Ok(json!({"count":sorted.len(),"medianMs":sorted[sorted.len()/2],
        "p95Ms":sorted[(sorted.len()*95).div_ceil(100)-1],
        "totalMs":sorted.iter().sum::<f64>(),"samplesMs":values}))
}

fn isolated(settings: &Config, host: &Host, directory: &Path) -> Result<(App, Arc<App>)> {
    let mut config = settings.clone();
    config.data_dir = directory.join("root");
    config.sidebar_state_file = directory.join("registry.json");
    config.native_sections = false;
    config.sync_titles = false;
    config.auto_archive_seconds = None;
    config.remote_auto_archive_seconds = None;
    crate::config::write_json(&config.sidebar_state_file, &json!({
        "codex-managed-remote-connections":[{"hostId":host.host_id,"displayName":host.display_name,
            "alias":host.alias,"hostname":host.hostname,"sshPort":host.ssh_port,"identity":host.identity}]
    }), false)?;
    let root = App::new(config.clone())?;
    config.host_id = host.host_id.clone();
    config.data_dir = host.directory(&root.config);
    config.remote_binding = Some(Binding::prepare(host, &root.config)?);
    let remote = Arc::new(App::new(config)?);
    root.install_remote(host.host_id.clone(), Arc::clone(&remote), false)?;
    Ok((root, remote))
}

fn timed(reader: &mut dyn Rpc, identifier: &str) -> Result<(Value, f64, usize)> {
    let started = Instant::now();
    let response = reader.call("thread/turns/list", json!({
        "threadId":identifier,"limit":1,"sortDirection":"desc","itemsView":"full"
    }))?;
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let size = serde_json::to_vec(&response)?.len();
    Ok((response, elapsed, size))
}

fn compare(full: &mut dyn Rpc, compact: &mut dyn Rpc, threads: &[Value]) -> Result<Value> {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let (mut full_bytes, mut summary_bytes) = (0, 0);
    for batch in 1..=BATCHES {
        for entry in threads {
            let identifier = entry["id"].as_str().context("会话 ID 缺失")?;
            let (original, summary) = if before.len() % 2 == 0 {
                (timed(full, identifier)?, timed(compact, identifier)?)
            } else {
                let summary = timed(compact, identifier)?;
                (timed(full, identifier)?, summary)
            };
            let turn = &original.0["data"][0];
            if !matches!(turn["status"].as_str(), Some("completed" | "failed" | "interrupted")) {
                continue;
            }
            ensure!(summarize_turn(turn) == summarize_turn(&summary.0["data"][0]),
                "摘要与完整结果的状态或输入证据不同");
            before.push(original.1);
            after.push(summary.1);
            full_bytes += original.2;
            summary_bytes += summary.2;
            if before.len() == SAMPLES * batch { break; }
        }
        ensure!(before.len() == SAMPLES * batch, "缺少 12 个已结束轮次的真实样本");
    }
    Ok(json!({"full":distribution(&before)?,"summary":distribution(&after)?,
        "fullBytes":full_bytes,"summaryBytes":summary_bytes,"projectionEqual":true,
        "batchCount":BATCHES,"readsPerBatch":SAMPLES}))
}

fn state_saves(root: &App, remote: &App, host: &Host, identifier: &str) -> Result<Value> {
    let mut values = Vec::new();
    for sample in 0..20 {
        let row = remote.store.sessions()?.into_iter()
            .find(|row| row["id"] == identifier).context("隔离会话缺失")?;
        let body = json!({"id":crate::store::remote::identity(&host.host_id, identifier),
            "state":if sample % 2 == 0 {"review"} else {"unknown"},
            "note":"isolated performance check","version":row["version"],"inputVersion":row["inputVersion"]});
        let started = Instant::now();
        let response = root.request("/api/state", Some(&body))?;
        values.push(started.elapsed().as_secs_f64() * 1000.0);
        ensure!(response["ok"] == true, "状态保存未确认");
        let saved = remote.store.sessions()?.into_iter().find(|row| row["id"] == identifier)
            .context("状态读回缺失")?;
        ensure!(saved["state"] == body["state"] && saved["note"] == body["note"], "状态读回不匹配");
    }
    distribution(&values)
}

fn enforce(report: &Value) -> Result<()> {
    ensure!(report["stateSave"]["medianMs"].as_f64().context("缺少状态耗时")? < 200.0
        && report["stateSave"]["p95Ms"].as_f64().context("缺少状态 p95")? < 350.0,
        "真实性能门禁失败：状态保存中位数须 <200ms，p95 须 <350ms");
    ensure!(report["turnRead"]["summary"]["totalMs"].as_f64().context("缺少读取耗时")?
        / (BATCHES as f64) < 250.0,
        "真实性能门禁失败：三批完整采样的每 12 次摘要读取平均须 <250ms");
    ensure!(report["turnRead"]["summaryBytes"].as_u64().context("缺少摘要大小")? * 10
        < report["turnRead"]["fullBytes"].as_u64().context("缺少完整大小")?,
        "真实性能门禁失败：摘要传输量须低于完整结果的 10%");
    Ok(())
}

pub fn run(settings: &Config, host_id: &str) -> Result<Value> {
    let host = hosts::registered(settings)?.into_iter()
        .find(|host| host.host_id == host_id).context("主机未登记")?;
    let directory = tempfile::Builder::new().prefix("csb-perf-")
        .permissions(std::fs::Permissions::from_mode(0o700)).tempdir_in("/tmp")?;
    let (root, remote) = isolated(settings, &host, directory.path())?;
    let timeout = Duration::from_secs(settings.rpc_timeout_seconds);
    let mut compact = remote.connection(timeout, Role::RemoteObserver)?;
    let mut full = remote.connection(timeout, Role::ArchiveWriter)?;
    let entries = full.call("thread/list", thread_list_params(100, Value::Null))?;
    let threads = entries["data"].as_array().context("真实会话列表无效")?;
    let identifier = threads.first().and_then(|entry| entry["id"].as_str()).context("没有真实会话")?;
    remote.store.discover(threads)?;
    let mut report = json!({"hostId":host_id,"version":env!("CARGO_PKG_VERSION"),
        "scope":"real SSH and RPC reads; state writes only in temporary board database",
        "turnRead":compare(&mut full, &mut compact, threads)?,
        "stateSave":state_saves(&root, &remote, &host, identifier)?});
    let result = enforce(&report);
    report["passed"] = json!(result.is_ok());
    report["failure"] = result.err().map(|error| json!(error.to_string())).unwrap_or(Value::Null);
    root.shutdown()?;
    Ok(report)
}
