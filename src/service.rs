use crate::app::App;
use crate::config::{self, Config};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use reqwest::blocking::Client;
#[cfg(target_os = "linux")]
use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::Instant;
use tiny_http::Server;

#[cfg(target_os = "macos")]
mod macos;
mod migration;
#[cfg(target_os = "macos")]
use macos::process_start;

#[cfg(test)]
use migration::{import_database, publish as publish_migration};

pub fn info(config: &Config) -> Result<Value> {
    Ok(json!({
        "app": "codex-session-board",
        "version": env!("CARGO_PKG_VERSION"),
        "build": config::build_id()?,
        "dataDir": config.data_dir,
        "configId": config::config_id(config)?,
        "pid": std::process::id()
    }))
}

fn lock_file(data: &Path) -> Result<File> {
    config::create_private_dir(data)?;
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(data.join("launch.lock"))?)
}

#[cfg(test)]
fn lock_data(data: &Path) -> Result<File> {
    let file = lock_file(data)?;
    file.lock_exclusive()?;
    Ok(file)
}

fn loopback_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).context("无效服务地址")?;
    ensure!(
        parsed.scheme() == "http"
            && parsed.host_str() == Some("127.0.0.1")
            && parsed.port().is_some_and(|port| port > 0)
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.path() == "/"
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "服务记录必须指向回环 HTTP 地址"
    );
    Ok(())
}

fn probe(url: &str) -> Result<Option<Value>> {
    loopback_url(url)?;
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(1))
        .build()?;
    let response = match client
        .get(format!("{}/api/info", url.trim_end_matches('/')))
        .send()
    {
        Ok(response) => response,
        Err(error) if error.is_timeout() || error.is_connect() => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let response = response
        .error_for_status()
        .context("端口已有无法核验的 HTTP 服务")?;
    let value: Value = response.json().context("端口已有非看板服务")?;
    ensure!(value.is_object(), "服务身份响应必须为对象");
    Ok(Some(value))
}

#[cfg(any(test, target_os = "linux"))]
fn stat_identity(stat: &str) -> Result<Option<u64>> {
    let (_, fields) = stat.rsplit_once(')').context("无效 /proc 进程记录")?;
    let values: Vec<_> = fields.split_whitespace().collect();
    let state = values.first().context("缺少进程状态")?;
    if matches!(*state, "Z" | "X") {
        return Ok(None);
    }
    Ok(Some(values.get(19).context("缺少进程启动时间")?.parse()?))
}

#[cfg(target_os = "linux")]
fn process_start(pid: u32) -> Result<Option<u64>> {
    ensure!(pid > 0, "无效进程 PID");
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    stat_identity(&stat)
}

fn verify_info(value: &Value, config: &Config, pid: Option<u32>) -> Result<()> {
    ensure!(
        value["app"] == "codex-session-board" && value["dataDir"] == json!(config.data_dir),
        "端口上的服务属于其他看板或数据目录"
    );
    if let Some(pid) = pid {
        ensure!(
            value["pid"].as_u64() == Some(u64::from(pid)),
            "服务 PID 已改变"
        );
    }
    Ok(())
}

fn verify_version(value: &Value, config: &Config, build: &str) -> Result<()> {
    ensure!(value["build"] == build, "已有其他版本看板运行，请先 stop");
    ensure!(
        value["configId"] == config::config_id(config)?,
        "运行服务的配置已变化，请先 stop"
    );
    Ok(())
}

fn running(config: &Config, url: &str) -> Result<bool> {
    let Some(value) = probe(url)? else {
        return Ok(false);
    };
    verify_info(&value, config, None)?;
    verify_version(&value, config, &config::build_id()?)?;
    Ok(true)
}

fn classify(config: &Config, lock: &File) -> Result<bool> {
    ensure!(config.port > 0, "看板需要固定端口");
    let occupied = match lock.try_lock_exclusive() {
        Ok(()) => false,
        Err(error) if error.kind() == ErrorKind::WouldBlock => true,
        Err(error) => return Err(error.into()),
    };
    if running(config, &format!("http://127.0.0.1:{}", config.port))? {
        return Ok(true);
    }
    if occupied {
        bail!("同一数据目录已有看板仍存活，但端口暂不可访问，不会重复启动");
    }
    Ok(false)
}

fn confirm_process(pid: u32, start: u64) -> Result<()> {
    ensure!(process_start(pid)? == Some(start), "PID 已复用，不会操作它");
    Ok(())
}

fn stop_verified(pid: u32, start: u64, url: &str, config: &Config) -> Result<()> {
    confirm_process(pid, start)?;
    #[cfg(target_os = "macos")]
    return macos::stop(pid, start, url, config);
    #[cfg(target_os = "linux")]
    stop_linux(pid, start, url, config)
}

#[cfg(target_os = "linux")]
fn stop_linux(pid: u32, start: u64, url: &str, config: &Config) -> Result<()> {
    let descriptor = pidfd_open(
        Pid::from_raw(i32::try_from(pid)?).context("无效进程 PID")?,
        PidfdFlags::empty(),
    )
    .context("无法取得进程 pidfd，不会发送信号")?;
    let value = probe(url)?.context("进程仍存活但无法核验服务身份")?;
    verify_info(&value, config, Some(pid))?;
    confirm_process(pid, start)?;
    pidfd_send_signal(&descriptor, Signal::INT)?;
    let timeout = config.rpc_timeout_seconds.saturating_add(10);
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(timeout))
        .context("停止等待时间过大")?;
    while Instant::now() < deadline {
        if process_start(pid)? != Some(start) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("服务进程退出尚未确认")
}

pub struct OwnedBoard {
    pub url: String,
    pub config: PathBuf,
    pub server: Server,
    pub port: u16,
    pub app: Arc<App>,
    lock: File,
}

impl OwnedBoard {
    pub fn run(self) -> Result<()> {
        let _lock = self.lock;
        crate::http::run(self.server, self.port, self.app)
    }
}

pub enum OpenedBoard {
    Reused { url: String, config: PathBuf },
    Owned(OwnedBoard),
}

pub fn open(config_path: &Path) -> Result<OpenedBoard> {
    let (_, default_data) = config::default_paths()?;
    config::initialize_config(config_path, &default_data)?;
    let loaded = config::load(Some(config_path))?;
    ensure!(loaded.port > 0, "看板需要固定端口");
    let url = format!("http://127.0.0.1:{}", loaded.port);
    let resolved = config::resolve_path(config_path)?;
    let lock = lock_file(&loaded.data_dir)?;
    if classify(&loaded, &lock)? {
        return Ok(OpenedBoard::Reused {
            url,
            config: resolved,
        });
    }
    config::build_id()?;
    let (server, port) = crate::http::bind(loaded.port)
        .map_err(|error| anyhow::anyhow!("端口暂不可访问，不会重复启动：{error}"))?;
    let app = Arc::new(App::new(loaded)?);
    #[cfg(target_os = "macos")]
    macos::listen(&app.config)?;
    crate::observer::start(&app)?;
    Ok(OpenedBoard::Owned(OwnedBoard {
        url: format!("http://127.0.0.1:{port}"),
        config: resolved,
        server,
        port,
        app,
        lock,
    }))
}

pub fn stop(config_path: &Path) -> Result<Value> {
    let config = config::load(Some(config_path))?;
    ensure!(config.port > 0, "看板需要固定端口");
    let url = format!("http://127.0.0.1:{}", config.port);
    let lock = lock_file(&config.data_dir)?;
    let occupied = match lock.try_lock_exclusive() {
        Ok(()) => false,
        Err(error) if error.kind() == ErrorKind::WouldBlock => true,
        Err(error) => return Err(error.into()),
    };
    let stopped = match probe(&url)? {
        Some(value) => {
            verify_info(&value, &config, None)?;
            let pid = u32::try_from(value["pid"].as_u64().context("服务未提供有效 PID")?)
                .context("服务未提供有效 PID")?;
            let start = process_start(pid)?.context("HTTP 所报进程未存活")?;
            stop_verified(pid, start, &url, &config)?;
            true
        }
        None if occupied => bail!("数据目录上的看板仍存活，但配置端口没有响应"),
        None => false,
    };
    Ok(json!({
        "running": false, "stopped": stopped,
        "config": config::resolve_path(config_path)?
    }))
}

pub fn migrate(config_path: &Path, source_config: &Path) -> Result<Value> {
    let config_path = migration::configuration(config_path)?;
    let mut config = migration::load_source(source_config)?;
    let source = config.data_dir.join("status.sqlite3");
    config.data_dir = config::default_paths()?.1;
    let _lock = migration::lock(&config.data_dir)?;
    let copied = migration::publish(&config_path, &config, &source)?;
    Ok(json!({
        "imported": true, "databaseCopied": copied, "dataDir": config.data_dir,
        "config": config_path,
        "snapshotConsistency": "per_database"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Result<Self> {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let directory =
                std::env::temp_dir().join(format!("board-service-{}-{stamp}", std::process::id()));
            config::create_private_dir(&directory)?;
            Ok(Self(directory))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture_config(directory: &Path) -> Config {
        Config {
            data_dir: directory.into(),
            ..Config::default()
        }
    }

    fn fixture_server(
        port: u16,
        value: Value,
        requests: usize,
    ) -> Result<thread::JoinHandle<Result<()>>> {
        let server = tiny_http::Server::http(("127.0.0.1", port))
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let handle = thread::spawn(move || {
            for _ in 0..requests {
                let request = server
                    .recv_timeout(Duration::from_secs(3))?
                    .context("测试服务未收到身份探测")?;
                ensure!(request.url() == "/api/info", "只允许身份探测");
                request.respond(tiny_http::Response::from_string(value.to_string()))?;
            }
            Ok(())
        });
        Ok(handle)
    }

    fn join_server(handle: thread::JoinHandle<Result<()>>) -> Result<()> {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("测试 HTTP 线程异常退出"))?
    }

    #[test]
    fn loopback_records_cannot_probe_external_or_injected_addresses() {
        for url in [
            "https://127.0.0.1:8767",
            "http://localhost:8767",
            "http://example.com:8767",
            "http://127.0.0.1:8767/path",
            "http://user@127.0.0.1:8767",
            "http://127.0.0.1:8767?x=1",
            "http://127.0.0.1:8767#fragment",
            "http://127.0.0.1:0",
        ] {
            assert!(loopback_url(url).is_err(), "{url}");
        }
        assert!(loopback_url("http://127.0.0.1:8767").is_ok());
    }

    #[test]
    fn proc_parser_handles_parentheses_and_zombies() -> Result<()> {
        let mut fields = vec!["S"; 20];
        fields[19] = "123456";
        let text = format!("123 (name with ) parentheses) {}", fields.join(" "));
        assert_eq!(stat_identity(&text)?, Some(123456));
        fields[0] = "Z";
        assert_eq!(
            stat_identity(&format!("123 (zombie) {}", fields.join(" ")))?,
            None
        );
        assert!(stat_identity("malformed").is_err());
        Ok(())
    }

    fn classify_only(config: &Config) -> Result<bool> {
        classify(config, &lock_file(&config.data_dir)?)
    }

    #[test]
    fn mismatched_process_identity_is_rejected_before_network_or_signals() -> Result<()> {
        let pid = std::process::id();
        let start = process_start(pid)?.context("测试进程应当存活")?;
        let error = stop_verified(
            pid,
            start + 1,
            "http://127.0.0.1:1",
            &fixture_config(Path::new("/unused")),
        )
        .expect_err("身份不符不能发信号");
        assert!(error.to_string().contains("PID 已复用"));
        Ok(())
    }

    #[test]
    fn identity_checks_include_configuration_build_and_database() -> Result<()> {
        let fixture = Fixture::new()?;
        let config = fixture_config(&fixture.0);
        let mut value = info(&config)?;
        verify_info(&value, &config, Some(std::process::id()))?;
        verify_version(&value, &config, &config::build_id()?)?;
        value["configId"] = json!("old");
        assert!(verify_version(&value, &config, &config::build_id()?).is_err());
        value["dataDir"] = json!("/other");
        assert!(verify_info(&value, &config, None).is_err());
        Ok(())
    }

    #[test]
    fn legacy_or_forged_records_are_not_silently_replaced() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut config = fixture_config(&fixture.0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        config.port = listener.local_addr()?.port();
        drop(listener);
        let path = fixture.0.join("server.json");
        let value = json!({"pid":123,"url":"http://127.0.0.1:8767"});
        config::write_json(&path, &value, false)?;
        assert!(!classify_only(&config)?);
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path)?)?, value);
        Ok(())
    }

    #[test]
    fn matching_service_reuses_verified_identity_and_rejects_port_change() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut config = fixture_config(&fixture.0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        config.port = port;
        let server = fixture_server(port, info(&config)?, 1)?;
        assert!(classify_only(&config)?);
        let _held = lock_data(&fixture.0)?;
        config.port = 1;
        let error = classify_only(&config).expect_err("更换端口不能另起一份");
        assert!(error.to_string().contains("仍存活"));
        join_server(server)?;
        Ok(())
    }

    #[test]
    fn live_inaccessible_process_cannot_be_launched_again() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut config = fixture_config(&fixture.0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        config.port = listener.local_addr()?.port();
        let _held = lock_data(&fixture.0)?;
        let error = classify_only(&config).expect_err("HTTP 无响应必须阻止重复启动");
        assert!(error.to_string().contains("仍存活"));
        assert!(!fixture.0.join("server.json").exists());
        Ok(())
    }

    #[test]
    fn registration_supports_actual_ephemeral_port_and_parent_lock() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut config = fixture_config(&fixture.0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        config.port = listener.local_addr()?.port();
        let parent = lock_data(&fixture.0)?;
        let error = classify_only(&config).expect_err("父进程占着锁时不能再监听");
        assert!(error.to_string().contains("仍存活"));
        drop(parent);
        config.port = 0;
        assert!(classify_only(&config).is_err());
        Ok(())
    }

    #[test]
    fn data_lock_serializes_independent_open_descriptors() -> Result<()> {
        let fixture = Fixture::new()?;
        let first = lock_data(&fixture.0)?;
        let second = lock_file(&fixture.0)?;
        assert!(second.try_lock_exclusive().is_err());
        FileExt::unlock(&first)?;
        second.try_lock_exclusive()?;
        Ok(())
    }

    #[test]
    fn migration_uses_backup_and_preserves_source_history_and_pending_titles() -> Result<()> {
        let fixture = Fixture::new()?;
        let source = fixture.0.join("source.sqlite3");
        let target = fixture.0.join("target.sqlite3");
        let connection = Connection::open(&source)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE changes(note TEXT);
             INSERT INTO changes VALUES ('人工备注');
             CREATE TABLE title_updates(version INTEGER);
             INSERT INTO title_updates VALUES (7);",
        )?;
        assert!(import_database(&source, &target)?);
        let copied = Connection::open(&target)?;
        assert_eq!(
            copied.query_row("SELECT note FROM changes", [], |row| row
                .get::<_, String>(0))?,
            "人工备注"
        );
        assert_eq!(
            copied.query_row("SELECT version FROM title_updates", [], |row| row
                .get::<_, i64>(0))?,
            7
        );
        assert!(source.exists());
        assert!(import_database(&source, &target).is_err());
        Ok(())
    }

    #[test]
    fn failed_backup_or_config_publication_cleans_only_the_new_database() -> Result<()> {
        let fixture = Fixture::new()?;
        let bad = fixture.0.join("bad.sqlite3");
        let target = fixture.0.join("status.sqlite3");
        fs::write(&bad, b"not a database")?;
        assert!(import_database(&bad, &target).is_err());
        assert!(!target.exists());
        let source = fixture.0.join("source.sqlite3");
        Connection::open(&source)?.execute_batch("CREATE TABLE retained(value TEXT);")?;
        let config = fixture_config(&fixture.0);
        let path = fixture.0.join("config.json");
        fs::write(&path, b"existing config")?;
        assert!(publish_migration(&path, &config, &source).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read(&path)?, b"existing config");
        assert!(source.exists());
        assert!(publish_migration(
            &fixture.0.join("retry.json"),
            &config,
            &source
        )?);
        Ok(())
    }

    #[test]
    fn existing_database_is_preserved_even_when_source_is_absent() -> Result<()> {
        let fixture = Fixture::new()?;
        let target = fixture.0.join("status.sqlite3");
        fs::write(&target, b"existing database")?;
        assert!(import_database(&fixture.0.join("missing.sqlite3"), &target).is_err());
        assert_eq!(fs::read(&target)?, b"existing database");
        Ok(())
    }
}
