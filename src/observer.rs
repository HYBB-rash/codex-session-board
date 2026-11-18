use crate::workers::pause;
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
    let (mut threads, complete) =
        rpc::list_threads(reader, app.config.page_size, app.config.max_pages)?;
    app.retain_business_threads(&mut threads)?;
    if app.config.four_columns {
        let _operation = app.operation()?;
        let mut undiscovered = Vec::new();
        for thread in &threads {
            if !app.store.has_session(thread["id"].as_str().context("会话缺少 ID")?)? {
                undiscovered.push(thread.clone());
            }
        }
        app.store.discover(&undiscovered)?;
    } else {
        app.store.discover(&threads)?;
        observe_history_turns(app, reader, &threads)?;
    }
    app.update_health(json!({"lastSuccessAt":now(),"error":null,"scanComplete":complete,"discovered":threads.len()}))
}

fn observe_history_turns(app: &App, reader: &mut dyn Rpc, threads: &[Value]) -> Result<()> {
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
    Ok(())
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
    revisions: &mut RuntimeRevisions,
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
    Unconfirmed(String),
    Phase(String),
    Saved(String, RuntimeRevision),
}

pub type RuntimeRevisions = HashMap<String, RuntimeRevision>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeRevision {
    metadata: String,
    turn_id: Value,
    turn_status: Value,
    checked_at: Instant,
}

impl RuntimeRevision {
    fn new(metadata: String, turn: &Value, checked_at: Instant) -> Self {
        Self {
            metadata,
            turn_id: turn["id"].clone(),
            turn_status: turn["status"].clone(),
            checked_at,
        }
    }

    fn unchanged_turn(&self, page: &Value) -> bool {
        let Some(turns) = page["data"].as_array() else { return false };
        match turns.as_slice() {
            [] => self.turn_id.is_null() && page["nextCursor"].is_null(),
            [turn] => turn["id"].is_string() && turn["id"] == self.turn_id
                && turn["status"] == self.turn_status,
            _ => false,
        }
    }
}

#[derive(Default)]
pub(crate) struct EvidenceGaps(HashMap<String, (std::time::Instant, usize)>);

impl EvidenceGaps {
    fn record(&mut self, unconfirmed: HashSet<String>, observed_at: std::time::Instant) -> usize {
        self.0.retain(|identifier, _| unconfirmed.contains(identifier));
        unconfirmed.into_iter().filter(|identifier| {
            let (started, count) = self.0.entry(identifier.clone()).or_insert((observed_at, 0));
            *count = count.saturating_add(1);
            *count >= 2 && observed_at.duration_since(*started) >= Duration::from_secs(30)
        }).count()
    }
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

fn runtime_pause(app: &App, error: &anyhow::Error) -> Result<bool> {
    app.preview_turns.clear()?;
    app.evidence_gaps.lock().map_err(|_| anyhow::anyhow!("执行信号锁已损坏"))?.0.clear();
    if frame_too_large(error) {
        return Ok(false);
    }
    app.catalog.lock().map_err(|_| anyhow::anyhow!("目录锁已损坏"))?.live = false;
    app.update_health(
        json!({"runtimeVerified":false,"runtimeError":error.to_string(),"scanComplete":false}),
    )?;
    Ok(true)
}

pub(crate) fn observe_runtime_threads(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut RuntimeRevisions,
    since: f64,
    threads: &[Value],
) -> Result<()> {
    observe_runtime_threads_at(app, reader, revisions, since, threads, Instant::now())
}

pub(crate) fn observe_runtime_threads_at(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut RuntimeRevisions,
    since: f64,
    threads: &[Value],
    observed_at: Instant,
) -> Result<()> {
    invalidate_changed_previews(app, revisions, threads)?;
    let rechecks = idle_rechecks(app, revisions, threads, observed_at);
    let mut unreadable = 0;
    let mut native_errors = Vec::new();
    let mut unconfirmed = HashSet::new();
    for thread in threads {
        let identifier = thread["id"].as_str().context("会话缺少 ID")?;
        match observe_one(app, reader, revisions, since, thread, observed_at, rechecks.contains(identifier))? {
            Seen::Unread => {
                app.preview_turns.remove(identifier)?;
                revisions.remove(identifier);
                unreadable += 1;
            }
            Seen::Unconfirmed(identifier) => { unconfirmed.insert(identifier); }
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
    let delayed = app.evidence_gaps.lock().map_err(|_| anyhow::anyhow!("执行信号锁已损坏"))?
        .record(unconfirmed, std::time::Instant::now());
    let warnings = [
        (unreadable > 0).then(|| format!("{unreadable} 个会话的执行状态暂不可读")),
        (delayed > 0).then(|| format!("{delayed} 个会话持续无法确认执行状态，已保留原位置")),
    ].into_iter().flatten().collect::<Vec<_>>();
    app.update_health(json!({"runtimeVerified":true,"runtimeLastAt":now(),
        "nativeStageError":if native_errors.is_empty() {Value::Null}else{json!(native_errors.join("; "))},
        "runtimeError":if warnings.is_empty() {Value::Null} else {json!(warnings.join("。"))}}))
}

fn invalidate_changed_previews(app: &App, revisions: &mut RuntimeRevisions, threads: &[Value]) -> Result<()> {
    for thread in threads {
        let identifier = thread["id"].as_str().context("会话缺少 ID")?;
        if !matches!(thread["status"]["type"].as_str(), Some("idle" | "notLoaded")) {
            revisions.remove(identifier);
            app.preview_turns.remove(identifier)?;
        } else if cached_revision(revisions, identifier, &thread_revision(thread)).is_none() {
            app.preview_turns.remove(identifier)?;
        }
    }
    Ok(())
}

fn idle_rechecks<'a>(
    app: &App,
    revisions: &RuntimeRevisions,
    threads: &'a [Value],
    observed_at: Instant,
) -> HashSet<&'a str> {
    let interval = Duration::from_secs(app.config.runtime_poll_seconds);
    let recently_updated_after = now() - interval.as_secs_f64();
    let mut due: Vec<_> = threads.iter().filter_map(|thread| {
        let identifier = thread["id"].as_str()?;
        let cached = cached_revision(revisions, identifier, &thread_revision(thread))?;
        let recent = thread["updatedAt"].as_f64().is_some_and(|time| time >= recently_updated_after);
        (observed_at.saturating_duration_since(cached.checked_at) >= interval)
            .then_some((cached.checked_at, identifier, recent))
    }).collect();
    due.sort_unstable();
    let mut due = due.into_iter();
    let oldest = due.next();
    let next = due.min_by_key(|(checked_at, identifier, recent)| (!recent, *checked_at, *identifier));
    oldest.into_iter().chain(next).map(|(_, identifier, _)| identifier).collect()
}

fn cached_revision<'a>(
    revisions: &'a RuntimeRevisions, identifier: &str, metadata: &str,
) -> Option<&'a RuntimeRevision> {
    revisions.get(identifier).filter(|cached| cached.metadata == metadata)
}

fn thread_revision(thread: &Value) -> String {
    json!([thread["updatedAt"], thread["status"]]).to_string()
}

fn observe_one(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut RuntimeRevisions,
    since: f64,
    thread: &Value,
    observed_at: Instant,
    recheck: bool,
) -> Result<Seen> {
    ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
    let identifier = thread["id"].as_str().context("会话缺少 ID")?;
    if app.is_native_probe(identifier)? { return Ok(Seen::Same); }
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
    let metadata = thread_revision(thread);
    let recheck_at = recheck.then_some(observed_at);
    match needs_turn_read(app, reader, revisions, identifier, &metadata, recheck_at) {
        Ok(false) => return Ok(Seen::Same),
        Err(error) if error.downcast_ref::<RequestError>().is_some() => return Ok(Seen::Unread),
        result => { result?; }
    }
    match read_runtime_turn(reader, identifier, status, app.config.four_columns) {
        Ok(turn) => {
            app.preview_turns.observe(identifier, status, &turn)?;
            let revision = RuntimeRevision::new(metadata, &turn, observed_at);
            let seen = apply_observed_turn(app, identifier, status, &turn, since, revision)?;
            Ok(if matches!(seen, Seen::Same) && unresolved_runtime(app, thread, &turn, since) {
                Seen::Unconfirmed(identifier.to_owned())
            } else {
                seen
            })
        }
        Err(error) if error.downcast_ref::<RequestError>().is_some() => Ok(Seen::Unread),
        Err(error) if frame_too_large(&error) => {
            app.store
                .observe(identifier, &json!({}), Some(&limit_note(updated_at)))?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn unresolved_runtime(app: &App, thread: &Value, turn: &Value, since: f64) -> bool {
    let status = &thread["status"];
    app.config.four_columns && crate::model::verified_runtime_state(status, turn).is_none()
        && (status["type"] == "active" || thread["updatedAt"].as_f64().is_some_and(|time| time >= since)
            && turn["userItems"].as_array().is_some_and(|items| !items.is_empty()))
}

fn needs_turn_read(
    app: &App,
    reader: &mut dyn Rpc,
    revisions: &mut RuntimeRevisions,
    identifier: &str,
    metadata: &str,
    recheck: Option<Instant>,
) -> Result<bool> {
    let Some(cached) = revisions.get_mut(identifier).filter(|cached| cached.metadata == metadata)
        else { return Ok(true) };
    if app.needs_native_backfill(identifier)? { return Ok(true); }
    let Some(observed_at) = recheck else { return Ok(false) };
    let page = rpc::latest_turn_page(reader, identifier, "notLoaded")?;
    if cached.unchanged_turn(&page) {
        cached.checked_at = observed_at;
        return Ok(false);
    }
    app.preview_turns.remove(identifier)?;
    revisions.remove(identifier);
    Ok(true)
}

fn read_runtime_turn(reader: &mut dyn Rpc, identifier: &str, status: &Value,
    four_columns: bool) -> Result<Value> {
    if !four_columns {
        return rpc::latest_turn(reader, identifier);
    }
    let turn = rpc::latest_verified_turn(reader, identifier)?;
    if turn["confirmedEmpty"] == true
        && !matches!(status["type"].as_str(), Some("idle" | "notLoaded"))
    {
        return Ok(json!({}));
    }
    Ok(turn)
}

fn apply_observed_turn(
    app: &App,
    identifier: &str,
    status: &Value,
    turn: &Value,
    since: f64,
    revision: RuntimeRevision,
) -> Result<Seen> {
    let _operation = app.operation()?;
    ensure!(app.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
    app.store.observe(identifier, turn, None)?;
    if let Err(error) = app.backfill_native(identifier, status) {
        return Ok(Seen::Phase(error.to_string()));
    }
    if app.config.four_columns && crate::model::verified_runtime_state(status, turn).is_none() {
        return Ok(if turn["confirmedEmpty"] == true {
            Seen::Saved(identifier.to_owned(), revision)
        } else {
            Seen::Same
        });
    }
    let since = app.native_runtime_since(identifier,since)?;
    if app.baseline_native_backfill(identifier,status,turn,since)? {
        return Ok(Seen::Saved(identifier.to_owned(), revision));
    }
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

fn history_worker(app: Arc<App>) -> Result<()> {
    let mut reader: Option<StdioReader> = None;
    while app.enabled.load(Ordering::SeqCst) {
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
            app.update_health(
                json!({"error":error.to_string(),"scanComplete":false,"refreshing":false}),
            )?;
            reader = None;
        }
        pause(
            &app.enabled,
            Duration::from_secs(app.config.poll_seconds),
            Some(&app.refresh),
        );
    }
    Ok(())
}

fn runtime_worker(app: Arc<App>) -> Result<()> {
    let mut reader: Option<RuntimeConnection> = None;
    let mut revisions = HashMap::new();
    let since = now();
    while app.enabled.load(Ordering::SeqCst) {
        let result = runtime_step(&app, &mut reader, &mut revisions, since);
        if let Err(error) = result {
            reader = None;
            let should_pause = runtime_pause(&app, &error)?;
            if !frame_too_large(&error) && !app.config.remote_retry {
                break;
            }
            if should_pause {
                pause(
                    &app.enabled,
                    Duration::from_secs(app.config.runtime_poll_seconds),
                    None,
                );
            }
            continue;
        }
        pause(
            &app.enabled,
            Duration::from_secs(app.config.runtime_poll_seconds),
            None,
        );
    }
    Ok(())
}

fn runtime_step(
    app: &App,
    reader: &mut Option<RuntimeConnection>,
    revisions: &mut RuntimeRevisions,
    since: f64,
) -> Result<()> {
    if reader.is_none() {
        *reader = Some(RuntimeConnection::connect(app)?);
        app.invalidate_native_mapping()?;
        revisions.clear();
    }
    let scan = if app.config.proxy_command.is_empty() {
        scan_runtime
    } else {
        crate::remote::scan_runtime_page
    };
    scan(
        app,
        reader.as_mut().context("读取器未连接")?,
        revisions,
        since,
    )
}

fn title_worker(app: Arc<App>) -> Result<()> {
    while app.enabled.load(Ordering::SeqCst) {
        let error = title_batch(&app).err().map(|error| format!("{error:#}"));
        app.update_health(json!({"titleWorkerError":error}))?;
        pause(
            &app.enabled,
            Duration::from_secs(app.config.poll_seconds),
            None,
        );
    }
    Ok(())
}

fn title_batch(app: &App) -> Result<()> {
    let mut errors = Vec::new();
    for job in app.store.pending_titles(None)?.iter().take(20) {
        let Some(_operation) = app.worker_operation()? else {
            break;
        };
        if let Some(identifier) = job["thread_id"].as_str()
            && let Err(error) = app.sync_title(identifier)
        {
            errors.push(format!("{identifier}: {error:#}"));
        }
    }
    crate::workers::finish(errors)
}

fn project_worker(app: Arc<App>) -> Result<()> {
    while app.enabled.load(Ordering::SeqCst) {
        let error = crate::projects::refresh(&app)
            .err()
            .map(|error| format!("{error:#}"));
        app.update_health(json!({"projectWorkerError":error}))?;
        pause(
            &app.enabled,
            Duration::from_secs(app.config.runtime_poll_seconds),
            None,
        );
    }
    Ok(())
}

fn remote_worker(app: Arc<App>) -> Result<()> {
    while app.enabled.load(Ordering::SeqCst) {
        if let Err(error) = crate::remote::refresh(&app) {
            app.update_health(json!({"remoteVerified":false,"remoteError":error.to_string()}))?;
        }
        pause(
            &app.enabled,
            Duration::from_secs(app.config.poll_seconds),
            Some(&app.remote_refresh),
        );
    }
    Ok(())
}

pub fn start(app: &Arc<App>) -> Result<()> {
    #[cfg(target_os = "linux")]
    crate::singleton::runtime::start(app)?;
    crate::auto_archive::start(app)?;
    if !app.config.sidebar_socket.as_os_str().is_empty() && !app.config.sidebar_caller.is_empty() {
        let remote = Arc::clone(app);
        app.workers.spawn(move || remote_worker(remote))?;
    }
    let projects = Arc::clone(app);
    app.workers.spawn(move || project_worker(projects))?;
    let history = Arc::clone(app);
    app.workers.spawn(move || history_worker(history))?;
    start_host(app)
}

pub(crate) fn start_host(app: &Arc<App>) -> Result<()> {
    crate::catalog::start(app)?;
    if !app.config.desktop_endpoint().as_os_str().is_empty() || !app.config.proxy_command.is_empty()
    {
        let runtime = Arc::clone(app);
        app.workers.spawn(move || runtime_worker(runtime))?;
    }
    if app.config.sync_titles {
        let titles = Arc::clone(app);
        app.workers.spawn(move || title_worker(titles))?;
    }
    if app.config.native_sections && app.config.proxy_command.is_empty() {
        let native = Arc::clone(app);
        app.workers.spawn(move || {
            while native.enabled.load(Ordering::SeqCst) {
                if let Err(error) = native.refresh_native() {
                    native.update_health(json!({"nativeError":error.to_string()}))?;
                }
                pause(
                    &native.enabled,
                    Duration::from_secs(native.config.runtime_poll_seconds),
                    None,
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "observer/four_column_tests.rs"]
mod four_column_tests;

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
    fn idle_rechecks_limit_extra_reads_and_rotate_across_stable_rows() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {data_dir: directory.path().into(), ..Config::default()})?;
        let rows = [thread("first", 100), thread("second", 100), thread("third", 100)];
        app.store.discover(&rows)?;
        let mut revisions = RuntimeRevisions::new();
        let mut reader = Script {calls: Vec::new()};
        let started = Instant::now();
        let interval = Duration::from_secs(app.config.runtime_poll_seconds);
        observe_runtime_threads_at(&app, &mut reader, &mut revisions, 0.0, &rows, started)?;
        assert_eq!(reader.calls, ["first", "second", "third"]);
        reader.calls.clear();
        observe_runtime_threads_at(&app, &mut reader, &mut revisions, 0.0, &rows, started + interval)?;
        assert_eq!(reader.calls, ["first", "second"]);
        reader.calls.clear();
        observe_runtime_threads_at(&app, &mut reader, &mut revisions, 0.0, &rows, started + interval * 2)?;
        assert_eq!(reader.calls, ["first", "third"]);
        Ok(())
    }

    #[test]
    fn runtime_disconnect_retires_catalog_liveness_but_frame_limit_does_not() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(crate::config::Config {
            data_dir:directory.path().into(), sync_titles:false, ..crate::config::Config::default()
        })?;
        app.catalog.lock().unwrap().live = true;
        assert!(!runtime_pause(&app, &anyhow::anyhow!("Message too long"))?);
        assert!(app.catalog.lock().unwrap().live);
        assert!(runtime_pause(&app, &anyhow::anyhow!("connection closed"))?);
        assert!(!app.catalog.lock().unwrap().live);
        assert_eq!(app.health.lock().unwrap()["runtimeVerified"], false);
        Ok(())
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

    fn wait_until(mut ready: impl FnMut() -> Result<bool>) -> Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        while !ready()? {
            ensure!(std::time::Instant::now() < deadline, "worker 未继续重试");
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    #[test]
    fn title_worker_retries_database_failures_and_keeps_unavailable_client_jobs() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = Arc::new(App::new(Config {
            data_dir: directory.path().into(),
            desktop_socket: directory.path().join("unavailable.sock"),
            sync_titles: true,
            poll_seconds: 1,
            ..Config::default()
        })?);
        app.store.discover(&[json!({"id":"retry"})])?;
        app.store
            .set_state("retry", Some("review"), "keep", 0, Some(0))?;
        let database = rusqlite::Connection::open(directory.path().join("status.sqlite3"))?;
        database.execute_batch(
            "CREATE TRIGGER fail_title BEFORE UPDATE ON title_updates
            BEGIN SELECT RAISE(FAIL, 'temporary database failure'); END;",
        )?;
        let worker = app.clone();
        app.workers.spawn(move || title_worker(worker))?;
        wait_until(|| {
            Ok(app.health.lock().unwrap()["titleWorkerError"]
                .as_str()
                .is_some_and(|error| error.contains("temporary database failure")))
        })?;
        database.execute_batch("DROP TRIGGER fail_title;")?;
        wait_until(|| Ok(app.store.pending_titles(None)?[0]["attempted_at"].is_number()))?;
        app.shutdown()?;
        assert!(app.health.lock().unwrap()["titleWorkerError"].is_null());
        assert_eq!(app.store.note("retry")?, "keep");
        assert_eq!(app.store.pending_titles(None)?.len(), 1);
        Ok(())
    }

    #[test]
    fn project_worker_retries_after_the_client_registry_becomes_available() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("registry.json");
        let app = Arc::new(App::new(Config {
            data_dir: directory.path().join("data"),
            sidebar_state_file: source.clone(),
            runtime_poll_seconds: 1,
            ..Config::default()
        })?);
        let worker = app.clone();
        app.workers.spawn(move || project_worker(worker))?;
        wait_until(|| Ok(app.health.lock().unwrap()["projectWorkerError"].is_string()))?;
        std::fs::write(source, r#"{"local-projects":{}}"#)?;
        wait_until(|| Ok(app.projects.lock().unwrap().loaded))?;
        app.shutdown()?;
        assert!(app.health.lock().unwrap()["projectWorkerError"].is_null());
        assert!(app.projects.lock().unwrap().error.is_none());
        Ok(())
    }
}
