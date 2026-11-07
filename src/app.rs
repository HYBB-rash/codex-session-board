use crate::{
    config::Config,
    model::{board_state, now},
    rpc::{DesktopConnection, Role, SidebarConnection},
    store::Store,
    titles,
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

mod native;
mod remote;
mod remote_snapshot;
mod drafts;

pub struct App {
    pub config: Config,
    pub store: Arc<Store>,
    pub refresh: AtomicBool,
    pub remote_refresh: AtomicBool,
    pub health: Mutex<Value>,
    pub(crate) projects: Mutex<crate::projects::Catalog>,
    pub(crate) title_lock: Mutex<()>,
    pub(crate) active_sessions: Mutex<Option<HashSet<String>>>,
    thread_operations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    native_generation: AtomicUsize,
    pub(crate) enabled: AtomicBool,
    pub(crate) remotes: Mutex<HashMap<String, Arc<App>>>,
    pub(crate) remote_requested: Mutex<HashSet<String>>,
    lifecycle: RwLock<()>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        config.validate_modes()?;
        ensure!(
            !config.sync_sections,
            "sync_sections 已停用，请配置 native_sections"
        );
        let mut store = Store::open(&config.data_dir, config.sync_titles)?;
        store.native_sections = config.native_sections;
        store.legacy_mode = config.legacy_mode;
        let store = Arc::new(store);
        let native_sections = config.native_sections;
        Ok(Self {
            config,
            store,
            refresh: AtomicBool::new(false),
            remote_refresh: AtomicBool::new(false),
            projects: Mutex::new(crate::projects::Catalog::default()),
            title_lock: Mutex::new(()),
            active_sessions: Mutex::new(None),
            thread_operations: Mutex::new(HashMap::new()),
            native_generation: AtomicUsize::new(0),
            enabled: AtomicBool::new(true),
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
        ensure!(self.enabled.load(Ordering::SeqCst), "主机连接已停止");
        let guard = self.lifecycle.read().map_err(|_| anyhow::anyhow!("主机操作锁已损坏"))?;
        ensure!(self.enabled.load(Ordering::SeqCst), "主机连接已停止");
        Ok(guard)
    }

    pub(crate) fn deactivate(&self) -> Result<()> {
        self.enabled.store(false, Ordering::SeqCst);
        let _guard = self.lifecycle.write().map_err(|_| anyhow::anyhow!("主机操作锁已损坏"))?;
        self.update_health(json!({"runtimeVerified":false,"nativeVerified":false,
            "scanComplete":false,"runtimeError":"主机已断开或登记改变，保留缓存"}))
    }

    pub(crate) fn connection(&self, timeout: Duration, role: Role) -> Result<DesktopConnection> {
        ensure!(self.enabled.load(Ordering::SeqCst), "主机登记已改变，请刷新核对");
        ensure!(!self.config.legacy_mode || !matches!(role, Role::DraftWriter | Role::ArchiveWriter),
            "六状态兼容模式不允许创建或归档远程会话");
        if let Some(binding) = &self.config.remote_binding {
            DesktopConnection::remote(binding, timeout, role)
        } else if self.config.proxy_command.is_empty() {
            DesktopConnection::connect(&self.config.desktop_socket, timeout, role)
        } else {
            DesktopConnection::proxy(&self.config.proxy_command, timeout, role)
        }
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
        self.store.discover(threads)?;
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
        let states: serde_json::Map<String, Value> = self.config.board_states()
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
        self.append_remote(&mut sessions, &mut catalog)?;
        self.annotate_connections(&mut sessions);
        Ok(
            json!({"states":states,"sessions":sessions,"projectCatalog":catalog,"columnSorts":self.store.column_sorts()?,
            "legacyMode":self.config.legacy_mode,
            "pendingDrafts":self.store.pending_drafts()?,
            "health":self.health.lock().map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?.clone()}),
        )
    }

    pub fn request(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        if !self.config.legacy_mode
            && matches!(path, "/api/state" | "/api/archive")
            && let Some(body) = body
            && body["id"].as_str().is_some_and(|id| id.starts_with("remote:"))
        {
            return self.route_remote(path, body);
        }
        match (path, body) {
            ("/api/sessions", None) => self.snapshot(),
            ("/api/state", Some(body)) => self.save(body),
            ("/api/column-sort", Some(body)) => self.store.save_column_sort(body),
            ("/api/archive", Some(body)) => crate::archive::request(self, body),
            ("/api/new", Some(body)) => self.new_session(body),
            ("/api/new/reset", Some(body)) => self.reset_remote_draft(body),
            ("/api/connect", Some(body)) => self.connect_remote(body),
            ("/api/open", Some(body)) => {
                let identifier = body["id"].as_str().context("缺少会话 ID")?;
                if identifier.starts_with("remote:") {
                    return self.open_remote(identifier);
                }
                ensure!(self.store.contains(identifier)?, "会话不在本地看板中");
                let url = open_conversation(identifier, &self.config)?;
                Ok(json!({"ok":true,"url":url}))
            }
            ("/api/refresh", Some(_)) => {
                self.refresh.store(true, Ordering::Relaxed);
                self.remote_refresh.store(true, Ordering::Relaxed);
                Ok(json!({"ok":true}))
            }
            _ => bail!("接口不存在"),
        }
    }

    fn new_session(&self, body: &Value) -> Result<Value> {
        ensure!(body.is_object(), "请求格式无效");
        if body["projectId"].as_str().is_some_and(|project|project.starts_with("remote:")) {
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
        if self.config.native_sections {
            let expected = body["sectionVersion"]
                .as_str()
                .context("缺少原生位置，请刷新后重试")?;
            self.save_native(
                identifier,
                state.as_str(),
                &note,
                (version, input),
                expected,
            )?;
        } else {
            self.store
                .set_state(identifier, state.as_str(), &note, version, Some(input))?;
        }
        self.sync_title(identifier)?;
        let pending = self.store.pending_titles(Some(identifier))?;
        Ok(
            json!({"ok":true,"titleEnabled":self.store.sync_titles,"titlePending":!pending.is_empty(),
            "titleError":pending.first().map(|job| &job["error"]),
            "sectionsEnabled":false,"sectionsPending":0,"sectionsError":null}),
        )
    }

    pub fn sync_title(&self, identifier: &str) -> Result<()> {
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
    let mut child = Command::new(config.open_command.first().context("缺少客户端命令")?)
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
            ensure!(status.success(), "Codex 客户端未接受跳转请求");
            return Ok(url);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::spawn(move || {
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
        let _guard = app.title_lock.lock().unwrap();
        let started = std::time::Instant::now();
        let response = app.request(
            "/api/state",
            Some(&json!({
                "id":"sample","state":"review","note":"保留备注","version":0,"inputVersion":0
            })),
        )?;
        assert!(started.elapsed() < Duration::from_millis(150));
        assert_eq!(response["ok"], true);
        assert_eq!(response["titlePending"], true);
        assert_eq!(app.store.sessions()?[0]["note"], "保留备注");
        Ok(())
    }

    use std::path::PathBuf;
}
