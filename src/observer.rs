use crate::{
    app::App,
    model::now,
    rpc::{self, DesktopConnection, RequestError, Role, Rpc, StdioReader},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

pub fn scan_history(app: &App, reader: &mut dyn Rpc) -> Result<()> {
    app.update_health(json!({"refreshing":true,"lastScanAt":now()}))?;
    let result = history_pass(app, reader);
    app.update_health(json!({"refreshing":false}))?;
    if let Err(error) = &result {
        app.update_health(json!({"error":error.to_string(),"scanComplete":false}))?;
    }
    result
}

fn history_pass(app: &App, reader: &mut dyn Rpc) -> Result<()> {
    let (threads, complete) =
        rpc::list_threads(reader, app.config.page_size, app.config.max_pages)?;
    app.store.discover(&threads)?;
    let ids: HashSet<_> = threads
        .iter()
        .filter_map(|thread| thread["id"].as_str())
        .collect();
    let candidates: Vec<_> = app
        .store
        .pending()?
        .into_iter()
        .filter(|row| ids.contains(row["id"].as_str().unwrap_or("")))
        .collect();
    for row in history_candidates(&candidates, app.config.history_per_poll) {
        let identifier = row["id"].as_str().context("会话缺少 ID")?;
        match rpc::latest_turn(reader, identifier) {
            Ok(turn) => app.store.observe(identifier, &turn, None)?,
            Err(error) => {
                app.store
                    .observe(identifier, &json!({}), Some(&error.to_string()))?;
                if error.downcast_ref::<RequestError>().is_none() {
                    return Err(error);
                }
            }
        }
    }
    app.update_health(json!({"lastSuccessAt":now(),"error":null,"scanComplete":complete,"discovered":threads.len()}))
}

pub fn history_candidates(rows: &[Value], quota: usize) -> Vec<Value> {
    let mut changed: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["observed_revision"].is_number()
                && row["updated_at"].as_f64() > row["observed_revision"].as_f64()
                && row["observation_error"].is_null()
        })
        .cloned()
        .collect();
    changed.sort_by(|first, second| {
        second["updated_at"]
            .as_i64()
            .cmp(&first["updated_at"].as_i64())
    });
    changed.truncate(quota / 2);
    let chosen: HashSet<_> = changed
        .iter()
        .map(|row| row["id"].clone().to_string())
        .collect();
    changed.extend(
        rows.iter()
            .filter(|row| !chosen.contains(&row["id"].to_string()))
            .cloned(),
    );
    changed.truncate(quota);
    changed
}

pub fn scan_runtime(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut HashMap<String, String>,
    since: f64,
) -> Result<()> {
    let (threads, complete) =
        rpc::list_threads(reader, app.config.page_size, app.config.max_pages)?;
    ensure!(complete, "桌面运行状态分页未完成");
    app.discover(&threads, complete)?;
    let mut unreadable = 0;
    let mut native_errors = Vec::new();
    for thread in &threads {
        let identifier = thread["id"].as_str().context("会话缺少 ID")?;
        let status = &thread["status"];
        let revision = json!([thread["updatedAt"], status]).to_string();
        if status["type"] != "active" && revisions.get(identifier) == Some(&revision) {
            continue;
        }
        match rpc::latest_turn(reader, identifier) {
            Ok(turn) => {
                app.store.observe(identifier, &turn, None)?;
                if crate::model::runtime_state(status, &turn).is_some() {
                    if let Some(error) = observe_phase(app, identifier, status, &turn, since)? {
                        native_errors.push(error);
                        continue;
                    }
                    revisions.insert(identifier.to_string(), revision);
                }
            }
            Err(error) if error.downcast_ref::<RequestError>().is_some() => unreadable += 1,
            Err(error) => return Err(error),
        }
    }
    app.update_health(json!({"runtimeVerified":true,"runtimeLastAt":now(),
        "nativeStageError":if native_errors.is_empty() {Value::Null}else{json!(native_errors.join("; "))},
        "runtimeError":if unreadable > 0 { json!(format!("{unreadable} 个会话的执行状态暂不可读")) } else {Value::Null}}))
}

fn observe_phase(
    app: &App,
    identifier: &str,
    status: &Value,
    turn: &Value,
    since: f64,
) -> Result<Option<String>> {
    match app.observe_runtime(identifier, status, turn, since) {
        Ok(()) => Ok(None),
        Err(error) if app.config.native_sections => Ok(Some(error.to_string())),
        Err(error) => Err(error),
    }
}

fn pause(app: &App, seconds: u64) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if app.refresh.swap(false, Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn history_worker(app: Arc<App>) {
    let mut reader: Option<StdioReader> = None;
    loop {
        let result = (|| {
            if reader.is_none() {
                reader = Some(StdioReader::connect(
                    &app.config.codex_command,
                    Duration::from_secs(app.config.rpc_timeout_seconds),
                )?);
            }
            scan_history(&app, reader.as_mut().context("读取器未连接")?)
        })();
        if let Err(error) = result {
            let _ = app.update_health(
                json!({"error":error.to_string(),"scanComplete":false,"refreshing":false}),
            );
            reader = None;
        }
        pause(&app, app.config.poll_seconds);
    }
}

fn runtime_worker(app: Arc<App>) {
    let mut reader: Option<DesktopConnection> = None;
    let mut revisions = HashMap::new();
    let since = now();
    loop {
        let result = (|| {
            if reader.is_none() {
                reader = Some(app.connection(
                    Duration::from_secs(app.config.rpc_timeout_seconds),
                    Role::Reader,
                )?);
                revisions.clear();
            }
            scan_runtime(
                &app,
                reader.as_mut().context("读取器未连接")?,
                &mut revisions,
                since,
            )
        })();
        if let Err(error) = result {
            let _ = app
                .update_health(json!({"runtimeVerified":false,"runtimeError":error.to_string()}));
            reader = None;
        }
        std::thread::sleep(Duration::from_secs(app.config.runtime_poll_seconds));
    }
}

fn title_worker(app: Arc<App>) {
    loop {
        if let Ok(jobs) = app.store.pending_titles(None) {
            for job in jobs.iter().take(20) {
                if let Some(identifier) = job["thread_id"].as_str() {
                    let _ = app.sync_title(identifier);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(app.config.poll_seconds));
    }
}

fn project_worker(app: Arc<App>) {
    loop {
        let _ = crate::projects::refresh(&app);
        std::thread::sleep(Duration::from_secs(app.config.runtime_poll_seconds));
    }
}

fn remote_worker(app: Arc<App>) {
    loop {
        let _ = crate::remote::refresh(&app);
        let deadline = Instant::now() + Duration::from_secs(app.config.poll_seconds);
        while Instant::now() < deadline {
            if app.remote_refresh.swap(false, Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

pub fn start(app: &Arc<App>) {
    crate::auto_archive::start(app);
    if !app.config.sidebar_socket.as_os_str().is_empty() && !app.config.sidebar_caller.is_empty() {
        let remote = Arc::clone(app);
        std::thread::spawn(move || remote_worker(remote));
    }
    let projects = Arc::clone(app);
    std::thread::spawn(move || project_worker(projects));
    let history = Arc::clone(app);
    std::thread::spawn(move || history_worker(history));
    if !app.config.desktop_socket.as_os_str().is_empty() {
        let runtime = Arc::clone(app);
        std::thread::spawn(move || runtime_worker(runtime));
    }
    if app.config.sync_titles {
        let titles = Arc::clone(app);
        std::thread::spawn(move || title_worker(titles));
    }
    if app.config.native_sections {
        let native = Arc::clone(app);
        std::thread::spawn(move || {
            loop {
                if let Err(error) = native.refresh_native() {
                    let _ = native.update_health(json!({"nativeError":error.to_string()}));
                }
                std::thread::sleep(Duration::from_secs(native.config.runtime_poll_seconds));
            }
        });
    }
}
