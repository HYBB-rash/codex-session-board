use crate::{
    app::App,
    model::now,
    rpc::{self, RequestError, Rpc, StdioReader, runtime::RuntimeConnection},
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
    observe_runtime_threads(app, reader, revisions, since, &threads)
}

enum Seen {
    Same,
    Unread,
    Phase(String),
    Saved(String, String),
}

pub(crate) fn frame_too_large(error: &anyhow::Error) -> bool {
    error.to_string().contains("Message too long")
}

fn limit_note(updated_at: i64) -> String {
    format!("轮次响应超过单帧上限:{updated_at}")
}

fn frame_blocked(note: Option<&str>, updated_at: i64) -> bool {
    note.and_then(|note| note.strip_prefix("轮次响应超过单帧上限:"))
        .and_then(|value| value.parse().ok())
        == Some(updated_at)
}

fn runtime_pause(app: &App, error: &anyhow::Error) -> bool {
    if frame_too_large(error) {
        return false;
    }
    let _ = app.update_health(
        json!({"runtimeVerified":false,"runtimeError":error.to_string(),"scanComplete":false}),
    );
    true
}

pub(crate) fn observe_runtime_threads(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut HashMap<String, String>,
    since: f64,
    threads: &[Value],
) -> Result<()> {
    let mut unreadable = 0;
    let mut native_errors = Vec::new();
    for thread in threads {
        match observe_one(app, reader, revisions, since, thread)? {
            Seen::Unread => unreadable += 1,
            Seen::Phase(error) => {
                if !native_errors.contains(&error) {
                    native_errors.push(error);
                }
            }
            Seen::Saved(identifier, revision) => {
                revisions.insert(identifier, revision);
            }
            Seen::Same => {}
        }
    }
    app.update_health(json!({"runtimeVerified":true,"runtimeLastAt":now(),
        "nativeStageError":if native_errors.is_empty() {Value::Null}else{json!(native_errors.join("; "))},
        "runtimeError":if unreadable > 0 { json!(format!("{unreadable} 个会话的执行状态暂不可读")) } else {Value::Null}}))
}

fn observe_one(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &HashMap<String, String>,
    since: f64,
    thread: &Value,
) -> Result<Seen> {
    ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
    let identifier = thread["id"].as_str().context("会话缺少 ID")?;
    if !app.store.contains(identifier)? {
        return Ok(Seen::Same);
    }
    let updated_at = thread["updatedAt"].as_i64().unwrap_or(0);
    if frame_blocked(
        app.store.observation_error(identifier)?.as_deref(),
        updated_at,
    ) {
        return Ok(Seen::Same);
    }
    let status = &thread["status"];
    let revision = json!([thread["updatedAt"], status]).to_string();
    if status["type"] != "active" && revisions.get(identifier) == Some(&revision) {
        return Ok(Seen::Same);
    }
    match rpc::latest_turn(reader, identifier) {
        Ok(turn) => apply_observed_turn(app, identifier, status, &turn, since, revision),
        Err(error) if error.downcast_ref::<RequestError>().is_some() => Ok(Seen::Unread),
        Err(error) if frame_too_large(&error) => {
            app.store
                .observe(identifier, &json!({}), Some(&limit_note(updated_at)))?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn apply_observed_turn(
    app: &App,
    identifier: &str,
    status: &Value,
    turn: &Value,
    since: f64,
    revision: String,
) -> Result<Seen> {
    let _operation = app.operation()?;
    ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
    app.store.observe(identifier, turn, None)?;
    if crate::model::runtime_state(status, turn).is_none() {
        return Ok(Seen::Same);
    }
    if let Some(error) = observe_phase(app, identifier, status, turn, since)? {
        return Ok(Seen::Phase(error));
    }
    Ok(Seen::Saved(identifier.to_owned(), revision))
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
    let mut reader: Option<RuntimeConnection> = None;
    let mut revisions = HashMap::new();
    let since = now();
    while app.enabled.load(Ordering::SeqCst) {
        let result = (|| {
            if reader.is_none() {
                reader = Some(RuntimeConnection::connect(&app)?);
                revisions.clear();
            }
            let scan = if app.config.proxy_command.is_empty() {
                scan_runtime
            } else {
                crate::remote::scan_runtime_page
            };
            scan(
                &app,
                reader.as_mut().context("读取器未连接")?,
                &mut revisions,
                since,
            )
        })();
        if let Err(error) = result {
            reader = None;
            let pause = runtime_pause(&app, &error);
            if !frame_too_large(&error) && !app.config.remote_retry {
                break;
            }
            if pause {
                std::thread::sleep(Duration::from_secs(app.config.runtime_poll_seconds));
            }
            continue;
        }
        std::thread::sleep(Duration::from_secs(app.config.runtime_poll_seconds));
    }
}

fn title_worker(app: Arc<App>) {
    while app.enabled.load(Ordering::SeqCst) {
        if let Ok(jobs) = app.store.pending_titles(None) {
            for job in jobs.iter().take(20) {
                if let Some(identifier) = job["thread_id"].as_str()
                    && let Ok(_operation) = app.operation()
                {
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
    start_host(app);
}

pub(crate) fn start_host(app: &Arc<App>) {
    if !app.config.desktop_socket.as_os_str().is_empty() || !app.config.proxy_command.is_empty() {
        let runtime = Arc::clone(app);
        std::thread::spawn(move || runtime_worker(runtime));
    }
    if app.config.sync_titles {
        let titles = Arc::clone(app);
        std::thread::spawn(move || title_worker(titles));
    }
    if app.config.native_sections && app.config.proxy_command.is_empty() {
        let native = Arc::clone(app);
        std::thread::spawn(move || {
            while native.enabled.load(Ordering::SeqCst) {
                if let Err(error) = native.refresh_native() {
                    let _ = native.update_health(json!({"nativeError":error.to_string()}));
                }
                std::thread::sleep(Duration::from_secs(native.config.runtime_poll_seconds));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    struct Script {
        calls: Vec<String>,
    }

    impl Rpc for Script {
        fn call(&mut self, _method: &str, params: Value) -> Result<Value> {
            let identifier = params["threadId"].as_str().unwrap_or_default().to_string();
            self.calls.push(identifier.clone());
            if identifier == "fat" {
                anyhow::bail!("Space limit exceeded: Message too long: 30 > 16");
            }
            Ok(json!({"data":[{"id":"turn","status":"completed","items":[]}]}))
        }
    }

    fn thread(identifier: &str, updated_at: i64) -> Value {
        json!({"id":identifier,"updatedAt":updated_at,"status":{"type":"idle"},"name":identifier})
    }

    #[test]
    fn oversized_turn_blocks_only_that_session_until_it_changes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().join("data"),
            sync_titles: false,
            native_sections: false,
            ..Config::default()
        })?;
        let threads = vec![thread("fat", 5), thread("thin", 6)];
        app.store.discover(&threads)?;
        let mut revisions = HashMap::new();
        let mut reader = Script { calls: Vec::new() };
        let error = observe_runtime_threads(&app, &mut reader, &mut revisions, 0.0, &threads)
            .err()
            .context("应当因超限失败")?;
        assert!(frame_too_large(&error), "{error:#}");
        let note = app
            .store
            .observation_error("fat")?
            .context("缺少超限记录")?;
        assert!(note.contains('5'));
        reader.calls.clear();
        observe_runtime_threads(&app, &mut reader, &mut revisions, 0.0, &threads)?;
        assert_eq!(reader.calls, ["thin"]);
        let health = app
            .health
            .lock()
            .map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?;
        assert_eq!(health["runtimeVerified"], true);
        Ok(())
    }
}
