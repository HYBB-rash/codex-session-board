mod plugin_update;

use crate::{
    config::Config,
    model::now,
    operations::{Action, Method, Operation},
    rpc::{DesktopConnection, Role, SidebarConnection},
    store::Store,
    titles,
    workers::WorkerGroup,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex, RwLock, RwLockReadGuard,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

mod archive_batches;
mod drafts;
mod project_assignment;
mod native;
mod native_backfill;
mod native_mapping;
mod native_probes;
mod preview;
mod remote;
mod remote_snapshot;
mod target_drafts;
mod ssh_setup;
mod connection;
mod four_columns;
#[cfg(test)]
mod placement_tests;
pub use connection::preflight;
pub(crate) use connection::ConnectionRequest;

pub struct App {
    pub config: Config,
    pub store: Arc<Store>,
    pub(crate) mapping_store: Arc<Store>,
    pub(crate) native_mapping_scope: Mutex<Option<Value>>,
    pub refresh: AtomicBool,
    pub remote_refresh: AtomicBool,
    pub(crate) catalog_refresh: AtomicBool,
    pub(crate) catalog: Mutex<crate::catalog::State>,
    pub health: Mutex<Value>,
    pub(crate) projects: Mutex<crate::projects::Catalog>,
    pub(crate) title_lock: Mutex<()>,
    pub(crate) active_sessions: Mutex<Option<HashSet<String>>>,
    pub(crate) evidence_gaps: Mutex<crate::observer::EvidenceGaps>,
    pub(crate) preview_turns: preview::PreviewTurns,
    thread_operations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    ssh_previews: Mutex<HashMap<String, ssh_setup::Decision>>,
    native_generation: AtomicUsize,
    pub(crate) enabled: AtomicBool,
    pub(crate) workers: WorkerGroup,
    pub(crate) remotes: Mutex<HashMap<String, Arc<App>>>,
    pub(crate) remote_requested: Mutex<HashSet<String>>,
    lifecycle: RwLock<()>,
}

impl App {
    pub fn new(mut config: Config) -> Result<Self> {
        if config.local_authority.is_none() && config.remote_binding.is_none() {
            config.local_authority = Some(crate::rpc::authority::LocalAuthority::parent()?);
        }
        config.validate_modes()?;
        let store = Arc::new(Store::open_configured(&config)?);
        let native_sections = config.native_sections;
        let catalog = crate::catalog::State::load(&store, &config.host_id);
        Ok(Self {
            config,
            mapping_store: Arc::clone(&store),
            native_mapping_scope: Mutex::new(None),
            store,
            refresh: AtomicBool::new(false),
            remote_refresh: AtomicBool::new(false),
            catalog_refresh: AtomicBool::new(true),
            catalog: Mutex::new(catalog),
            projects: Mutex::new(crate::projects::Catalog::default()),
            title_lock: Mutex::new(()),
            active_sessions: Mutex::new(None),
            evidence_gaps: Mutex::default(),
            preview_turns: preview::PreviewTurns::default(),
            thread_operations: Mutex::new(HashMap::new()),
            ssh_previews: Mutex::new(HashMap::new()),
            native_generation: AtomicUsize::new(0),
            enabled: AtomicBool::new(true),
            workers: WorkerGroup::default(),
            remotes: Mutex::new(HashMap::new()),
            remote_requested: Mutex::new(HashSet::new()),
            lifecycle: RwLock::new(()),
            health: Mutex::new(json!({
                "lastScanAt":null,"lastSuccessAt":null,"error":null,"scanComplete":false,
                "refreshing":false,"discovered":0,"runtimeVerified":false,
                "runtimeError":"尚未连接桌面运行观察","runtimeLastAt":null,
                "sectionsEnabled":false,"sectionsError":null,"sectionsVerified":false,
                "nativeSections":native_sections,"nativeVerified":false,"nativeError":null,
            })),
        })
    }

    pub(crate) fn operation(&self) -> Result<RwLockReadGuard<'_, ()>> {
        self.worker_operation()?.context("主机连接已停止")
    }

    pub(crate) fn worker_operation(&self) -> Result<Option<RwLockReadGuard<'_, ()>>> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let guard = self
            .lifecycle
            .read()
            .map_err(|_| anyhow::anyhow!("主机操作锁已损坏"))?;
        Ok(self.enabled.load(Ordering::SeqCst).then_some(guard))
    }

    pub(crate) fn deactivate(&self) -> Result<()> {
        self.enabled.store(false, Ordering::SeqCst);
        let _guard = self
            .lifecycle
            .write()
            .map_err(|_| anyhow::anyhow!("主机操作锁已损坏"))?;
        if let Some(binding) = &self.config.remote_binding {
            binding.disconnect()?;
        }
        self.catalog
            .lock()
            .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?
            .live = false;
        self.update_health(json!({"runtimeVerified":false,"nativeVerified":false,
            "scanComplete":false,"runtimeError":"主机已断开或登记改变，保留缓存"}))
    }

    pub fn shutdown(&self) -> Result<()> {
        self.workers.close();
        let mut errors = Vec::new();
        if let Err(error) = self.deactivate() {
            errors.push(format!("{error:#}"));
        }
        let remotes = match self.remotes.lock() {
            Ok(remotes) => remotes.values().cloned().collect::<Vec<_>>(),
            Err(poison) => {
                errors.push("主机锁已损坏".into());
                poison.into_inner().values().cloned().collect()
            }
        };
        for remote in &remotes {
            remote.workers.close();
            remote.enabled.store(false, Ordering::SeqCst);
        }
        for remote in remotes {
            if let Err(error) = remote.shutdown() {
                errors.push(format!("{error:#}"));
            }
        }
        if let Err(error) = self.workers.join() {
            errors.push(format!("{error:#}"));
        }
        crate::workers::finish(errors)
    }

    pub fn update_health(&self, updates: Value) -> Result<()> {
        let mut health = self
            .health
            .lock()
            .map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?;
        for (key, value) in updates.as_object().context("健康状态必须为对象")? {
            health[key] = value.clone();
        }
        Ok(())
    }

    pub fn discover(&self, threads: &[Value], complete: bool) -> Result<()> {
        let mut threads = threads.to_vec();
        self.retain_business_threads(&mut threads)?;
        self.store.discover(&threads)?;
        if complete {
            let mut active = self
                .active_sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?;
            *active = Some(
                threads
                    .iter()
                    .filter_map(|thread| thread["id"].as_str())
                    .map(str::to_owned)
                    .collect(),
            );
        }
        Ok(())
    }

    pub fn is_active(&self, identifier: &str) -> Result<bool> {
        Ok(self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?
            .as_ref()
            .is_some_and(|active| active.contains(identifier)))
    }

    pub fn snapshot(&self) -> Result<Value> {
        self.load_remote_cache()?;
        let states: serde_json::Map<String, Value> = self
            .config
            .board_states()
            .map(|(state, label)| (state.to_string(), json!(label)))
            .collect();
        let mut sessions = self.store.sessions()?;
        if self.config.native_sections {
            let active = self
                .active_sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?;
            if let Some(active) = active.as_ref() {
                sessions.retain(|session| {
                    session["id"]
                        .as_str()
                        .is_some_and(|identifier| active.contains(identifier))
                });
            }
        }
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| anyhow::anyhow!("项目列表锁已损坏"))?;
        projects.annotate(&mut sessions);
        let mut catalog = serde_json::to_value(&*projects)?;
        drop(projects);
        self.annotate_previews(&mut sessions)?;
        self.append_remote(&mut sessions, &mut catalog)?;
        self.retain_business_rows(&mut sessions)?;
        self.annotate_connections(&mut sessions);
        crate::store::placement::project(&mut sessions, self.config.legacy_mode, self.config.four_columns);
        Ok(
            json!({"states":states,"sessions":sessions,"projectCatalog":catalog,"columnSorts":self.store.column_sorts()?,
            "legacyMode":self.config.legacy_mode,"fourColumns":self.config.four_columns,
            "runtime":crate::service::info(&self.config)?,
            "pendingDrafts":self.store.pending_drafts()?,
            "health":self.snapshot_health()?}),
        )
    }

    fn snapshot_health(&self) -> Result<Value> {
        let mut health = self.health.lock().map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?.clone();
        health["sshAliasSetup"] = json!(self.ssh_setup_enabled());
        Ok(health)
    }

    pub fn request(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        if path == "/api/native-calibration/reset" && let Some(body) = body {
            return self.reset_native_calibration(body);
        }
        let method = if body.is_some() { Method::Post } else { Method::Get };
        let op = Operation::for_request(path, method).context("接口不存在")?;
        self.execute(op, body.unwrap_or(&Value::Null))
    }

    pub(crate) fn execute(&self, op: &Operation, body: &Value) -> Result<Value> {
        if self.config.four_columns && matches!(op.action, Action::Update | Action::Sort)
            && let Some(state) = body["state"].as_str()
        {
            ensure!(self.config.board_states().any(|item| item.0 == state),
                "该状态已停止使用，请刷新看板并重新选择");
        }
        if self.routes_remote_write(op.action, body) {
            return self.route_remote(op.path, body);
        }
        match op.action {
            Action::Snapshot => self.snapshot(),
            Action::PluginUpdateStatus => self.plugin_update(None),
            Action::PluginUpdateStart => self.plugin_update(Some(body)),
            Action::PreviewReply => self.preview_reply(body),
            Action::Update => self.save(body),
            Action::Sort => self.store.save_column_sort(body),
            Action::Archive | Action::ArchiveBatches | Action::ArchiveBatchStart |
                Action::ArchiveBatchFinish | Action::ArchiveBatchCheck => self.archive_route(op.action, body),
            Action::New => self.new_session(body),
            Action::NewReset => self.reset_remote_draft(body),
            Action::ProjectAssign => self.assign_project(body),
            Action::Connect => self.connect_remote(body),
            Action::SshSetup => self.ssh_setup(body),
            Action::Open => self.open_session(body),
            Action::Refresh => self.refresh_hosts(body),
        }
    }

    fn open_session(&self, body: &Value) -> Result<Value> {
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        if identifier.starts_with("remote:") {
            return self.open_remote(identifier);
        }
        ensure!(self.store.contains(identifier)?, "会话不在本地看板中");
        let url = open_conversation(identifier, &self.config)?;
        Ok(json!({"ok":true,"url":url}))
    }

    fn routes_remote_write(&self, action: Action, body: &Value) -> bool {
        !self.config.legacy_mode
            && matches!(action, Action::Update | Action::Archive)
            && (action != Action::Archive || body.get("batchId").is_none())
            && body["id"].as_str().is_some_and(|id| id.starts_with("remote:"))
    }

    fn new_session(&self, body: &Value) -> Result<Value> {
        ensure!(body.is_object(), "请求格式无效");
        if body["projectId"]
            .as_str()
            .is_some_and(|project| project.starts_with("target:"))
        {
            return self.prepare_target_native_draft(body);
        }
        if body["projectId"]
            .as_str()
            .is_some_and(|project| project.starts_with("remote:"))
        {
            return self.new_remote_session(body);
        }
        let url = match body.get("projectId") {
            None => "codex://threads/new".to_string(),
            Some(project) => {
                let identifier = project.as_str().context("项目 ID 必须是文本")?;
                ensure!(
                    !identifier.starts_with("remote:"),
                    "远程项目暂不支持新建会话"
                );
                crate::projects::validate_project(&self.config, identifier)?;
                let mut url = url::Url::parse("codex://new")?;
                url.query_pairs_mut().append_pair("projectId", identifier);
                url.to_string()
            }
        };
        let url = open_client_url(url, &self.config)?;
        Ok(json!({"ok":true,"url":url}))
    }

    fn save(&self, body: &Value) -> Result<Value> {
        let _operation = self.operation()?;
        ensure!(body.is_object(), "请求格式无效");
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        if identifier.starts_with("remote:") {
            return self.store.save_remote(body);
        }
        let state = body.get("state").context("缺少状态")?;
        ensure!(state.is_string() || state.is_null(), "状态格式无效");
        let note = match body.get("note") {
            Some(note) => note.as_str().context("备注必须是文本")?.to_string(),
            None => self.store.note(identifier)?,
        };
        let version = body["version"].as_i64().context("缺少状态版本")?;
        let input = body["inputVersion"]
            .as_i64()
            .context("缺少输入版本，请刷新并核对后重试")?;
        let section_version = if self.config.native_sections {
            let expected = body["sectionVersion"]
                .as_str()
                .context("缺少原生位置，请刷新后重试")?;
            Some(self.save_native(
                identifier,
                state.as_str(),
                &note,
                (version, input),
                expected,
            )?)
        } else {
            self.store
                .set_state(identifier, state.as_str(), &note, version, Some(input))?;
            None
        };
        self.sync_title(identifier)?;
        let pending = self.store.pending_titles(Some(identifier))?;
        Ok(
            json!({"ok":true,"titleEnabled":self.store.sync_titles,"titlePending":!pending.is_empty(),
            "titleError":pending.first().map(|job| &job["error"]),
            "sectionsEnabled":false,"sectionsPending":0,"sectionsError":null,"sectionVersion":section_version}),
        )
    }

    pub fn sync_title(&self, identifier: &str) -> Result<()> {
        if self.is_native_probe(identifier)? { return Ok(()); }
        if self.needs_native_backfill(identifier)? { return Ok(()); }
        let Ok(_guard) = self.title_lock.try_lock() else {
            return Ok(());
        };
        let operation = self.thread_lock(identifier)?;
        let _thread = self
            .config
            .native_sections
            .then(|| operation.lock())
            .transpose()
            .map_err(|_| anyhow::anyhow!("会话操作锁已损坏"))?;
        let jobs = self.store.pending_titles(Some(identifier))?;
        let Some(job) = jobs.first() else {
            return Ok(());
        };
        let version = job["version"].as_i64().context("标题任务版本无效")?;
        if self.config.native_sections
            && let Err(error) = self.check_native_title(identifier)
        {
            self.store
                .title_failed(identifier, version, &error.to_string())?;
            return Ok(());
        }
        let timeout = Duration::from_secs(self.config.rpc_timeout_seconds.min(3));
        match self.connection(timeout, Role::TitleWriter) {
            Ok(mut writer) => titles::sync(&self.store, &mut writer, identifier),
            Err(error) => self
                .store
                .title_failed(identifier, version, &error.to_string()),
        }
    }

    pub fn observe_runtime(
        &self,
        identifier: &str,
        status: &Value,
        turn: &Value,
        since: f64,
    ) -> Result<()> {
        if self.config.native_sections {
            return self.observe_native_runtime(identifier, status, turn, since);
        }
        if self
            .store
            .observe_runtime(identifier, status, turn, Some(since))?
        {
            self.sync_title(identifier)?;
        }
        Ok(())
    }
}

pub fn conversation_url(identifier: &str, host: &str) -> Result<String> {
    let uuid = uuid::Uuid::parse_str(identifier).context("无效会话标识")?;
    ensure!(uuid.to_string() == identifier, "无效会话标识");
    ensure!(!host.is_empty(), "缺少 Codex 主机标识");
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("hostId", host)
        .finish();
    Ok(format!("codex://threads/{identifier}?{query}"))
}

pub fn open_conversation(identifier: &str, config: &Config) -> Result<String> {
    let url = conversation_url(identifier, &config.host_id)?;
    open_client_url(url, config)
}

fn open_client_url(url: String, config: &Config) -> Result<String> {
    #[cfg(target_os = "linux")]
    let singleton = crate::singleton::runtime::navigation(config)?;
    let directory = config
        .data_dir
        .canonicalize()
        .context("客户端工作目录不可用")?;
    let mut child = Command::new(config.open_command.first().context("缺少客户端命令")?)
        .current_dir(&directory)
        .args(&config.open_command[1..])
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("无法启动配置的 Codex 客户端，请检查 open_command")?;
    let deadline = now() + 1.0;
    while now() < deadline {
        if let Some(status) = child.try_wait()? {
            ensure!(
                status.success(),
                "Codex 客户端未接受跳转请求，退出状态：{status}，工作目录：{}",
                directory.display()
            );
            return Ok(url);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::spawn(move || {
        #[cfg(target_os = "linux")]
        let _singleton = singleton;
        let _ = child.wait();
    });
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_title_sync_does_not_wait_for_background_batch() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().to_path_buf(),
            desktop_socket: PathBuf::new(),
            ..Config::default()
        })?;
        app.store
            .discover(&[json!({"id":"sample","name":"示例"})])?;
        let response = std::thread::scope(|scope| {
            let guard = app.title_lock.lock().unwrap();
            let (sender, receiver) = std::sync::mpsc::channel();
            let app = &app;
            scope.spawn(move || {
                let result = app.request("/api/state", Some(&json!({
                    "id":"sample","state":"review","note":"保留备注","version":0,"inputVersion":0
                })));
                let _ = sender.send(result);
            });
            let response = receiver.recv_timeout(Duration::from_secs(10));
            drop(guard);
            response
        }).context("后台仍占用标题锁时前台保存应已返回")??;
        assert_eq!(response["ok"], true);
        assert_eq!(response["titlePending"], true);
        assert_eq!(app.store.note("sample")?, "保留备注");
        let jobs = app.store.pending_titles(Some("sample"))?;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["state"], "review");
        assert!(jobs[0]["attempted_at"].is_null());
        Ok(())
    }

    use std::path::PathBuf;
}
