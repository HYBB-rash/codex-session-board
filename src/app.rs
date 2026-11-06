use crate::{
    config::Config,
    model::{board_state, board_states, now},
    rpc::{DesktopConnection, Role, SidebarConnection},
    store::Store,
    titles,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

pub struct App {
    pub config: Config,
    pub store: Arc<Store>,
    pub refresh: AtomicBool,
    pub health: Mutex<Value>,
    pub(crate) projects: Mutex<crate::projects::Catalog>,
    pub(crate) title_lock: Mutex<()>,
    pub(crate) section_lock: Mutex<()>,
    pub(crate) active_sessions: Mutex<Option<HashSet<String>>>,
    pub(crate) section_cursor: AtomicUsize,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let store = Arc::new(Store::open(&config.data_dir, config.sync_titles)?);
        let sections_enabled = config.sync_sections;
        Ok(Self {
            config,
            store,
            refresh: AtomicBool::new(false),
            projects: Mutex::new(crate::projects::Catalog::default()),
            title_lock: Mutex::new(()),
            section_lock: Mutex::new(()),
            active_sessions: Mutex::new(None),
            section_cursor: AtomicUsize::new(0),
            health: Mutex::new(json!({
                "lastScanAt":null,"lastSuccessAt":null,"error":null,"scanComplete":false,
                "refreshing":false,"discovered":0,"runtimeVerified":false,
                "runtimeError":"尚未连接桌面运行观察","runtimeLastAt":null,
                "sectionsEnabled":sections_enabled,"sectionsError":null,"sectionsVerified":false,
            })),
        })
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
        let states: serde_json::Map<String, Value> = board_states()
            .map(|(state, label)| (state.to_string(), json!(label)))
            .collect();
        let mut sessions = self.store.sessions()?;
        for session in &mut sessions {
            if session["state"] == "completed" {
                session["state"] = json!(board_state("completed"));
            }
        }
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| anyhow::anyhow!("项目列表锁已损坏"))?;
        projects.annotate(&mut sessions);
        Ok(
            json!({"states":states,"sessions":sessions,"projectCatalog":*projects,
            "health":self.health.lock().map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?.clone()}),
        )
    }

    pub fn request(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        match (path, body) {
            ("/api/sessions", None) => self.snapshot(),
            ("/api/state", Some(body)) => self.save(body),
            ("/api/new", Some(_)) => {
                let url = open_client_url("codex://threads/new".to_string(), &self.config)?;
                Ok(json!({"ok":true,"url":url}))
            }
            ("/api/open", Some(body)) => {
                let identifier = body["id"].as_str().context("缺少会话 ID")?;
                ensure!(self.store.contains(identifier)?, "会话不在本地看板中");
                let url = open_conversation(identifier, &self.config)?;
                Ok(json!({"ok":true,"url":url}))
            }
            ("/api/refresh", Some(_)) => {
                self.refresh.store(true, Ordering::Relaxed);
                Ok(json!({"ok":true}))
            }
            _ => bail!("接口不存在"),
        }
    }

    fn save(&self, body: &Value) -> Result<Value> {
        ensure!(body.is_object(), "请求格式无效");
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
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
        self.store
            .set_state(identifier, state.as_str(), &note, version, Some(input))?;
        self.sync_title(identifier)?;
        let sections = self.sync_sections(Some(identifier))?;
        let pending = self.store.pending_titles(Some(identifier))?;
        Ok(
            json!({"ok":true,"titleEnabled":self.store.sync_titles,"titlePending":!pending.is_empty(),
            "titleError":pending.first().map(|job| &job["error"]),
            "sectionsEnabled":self.config.sync_sections,"sectionsPending":sections["pending"],
            "sectionsError":sections["error"]}),
        )
    }

    pub fn sync_sections(&self, only: Option<&str>) -> Result<Value> {
        if !self.config.sync_sections {
            return Ok(json!({"pending":0,"error":null}));
        }
        let Ok(_guard) = self.section_lock.try_lock() else {
            return Ok(json!({"pending":1,"error":null}));
        };
        let timeout = Duration::from_secs(self.config.rpc_timeout_seconds.min(if only.is_some() {
            3
        } else {
            20
        }));
        let result =
            SidebarConnection::connect(&self.config, timeout).and_then(|mut writer| match only {
                Some(identifier) => crate::sections::move_one(self, &mut writer, identifier),
                None => crate::sections::reconcile(self, &mut writer, None),
            });
        let report = match result {
            Ok(report) => report,
            Err(error) => json!({"pending":1,"error":error.to_string()}),
        };
        if only.is_none() {
            self.update_health(json!({"sectionsVerified":report["pending"] == 0,
                "sectionsPending":report["pending"],"sectionsError":report["error"]}))?;
        }
        Ok(report)
    }

    pub fn sync_title(&self, identifier: &str) -> Result<()> {
        let Ok(_guard) = self.title_lock.try_lock() else {
            return Ok(());
        };
        let jobs = self.store.pending_titles(Some(identifier))?;
        let Some(job) = jobs.first() else {
            return Ok(());
        };
        let version = job["version"].as_i64().context("标题任务版本无效")?;
        let timeout = Duration::from_secs(self.config.rpc_timeout_seconds.min(3));
        match DesktopConnection::connect(&self.config.desktop_socket, timeout, Role::TitleWriter) {
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
        let before = self.store.state_revision(identifier)?;
        if self
            .store
            .observe_runtime(identifier, status, turn, Some(since))?
        {
            self.sync_title(identifier)?;
        }
        if self.store.state_revision(identifier)? != before {
            self.sync_sections(Some(identifier))?;
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
