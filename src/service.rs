use crate::config::{self, Config};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use reqwest::blocking::Client;
use rusqlite::{Connection, OpenFlags, backup::Backup};
use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessIdentity {
    start_time: u64,
    exe: PathBuf,
    device: u64,
    inode: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerRecord {
    pid: u32,
    url: String,
    #[serde(flatten)]
    identity: ProcessIdentity,
}

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

fn lock_data(data: &Path) -> Result<File> {
    let file = lock_file(data)?;
    file.lock_exclusive()?;
    Ok(file)
}

pub fn register(config: &Config, port: u16) -> Result<()> {
    ensure!(port > 0, "注册需要服务实际监听端口");
    let file = lock_file(&config.data_dir)?;
    match file.try_lock_exclusive() {
        Ok(()) => register_locked(config, port),
        Err(error) if error.kind() == ErrorKind::WouldBlock => {
            await_parent_registration(config, port)
        }
        Err(error) => Err(error.into()),
    }
}

fn register_locked(config: &Config, port: u16) -> Result<()> {
    let pid = std::process::id();
    if let Some(record) = read_record(&config.data_dir)? {
        ensure!(
            !verify_record(&record)? || record.pid == pid,
            "同一数据目录已有受管进程运行"
        );
    }
    let identity = process_identity(pid)?.context("无法核验当前服务进程")?;
    save_record(
        &config.data_dir,
        &ServerRecord {
            pid,
            url: format!("http://127.0.0.1:{port}"),
            identity,
        },
    )
}

fn await_parent_registration(config: &Config, port: u16) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(record) = read_record(&config.data_dir)?
            && record.pid == std::process::id()
            && verify_record(&record)?
        {
            ensure!(
                record.url == format!("http://127.0.0.1:{port}"),
                "启动器记录端口与实际监听端口不一致"
            );
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    bail!("启动锁被占用且未找到当前进程的父启动器记录")
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

fn stat_identity(stat: &str) -> Result<Option<u64>> {
    let (_, fields) = stat.rsplit_once(')').context("无效 /proc 进程记录")?;
    let values: Vec<_> = fields.split_whitespace().collect();
    let state = values.first().context("缺少进程状态")?;
    if matches!(*state, "Z" | "X") {
        return Ok(None);
    }
    Ok(Some(values.get(19).context("缺少进程启动时间")?.parse()?))
}

fn process_identity(pid: u32) -> Result<Option<ProcessIdentity>> {
    ensure!(pid > 0, "无效受管进程 PID");
    let directory = PathBuf::from(format!("/proc/{pid}"));
    let stat = match fs::read_to_string(directory.join("stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(start_time) = stat_identity(&stat)? else {
        return Ok(None);
    };
    let result = read_executable_identity(&directory, start_time);
    match result {
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        result => result.map(Some),
    }
}

fn read_executable_identity(directory: &Path, start_time: u64) -> Result<ProcessIdentity> {
    let executable = directory.join("exe");
    let metadata = fs::metadata(&executable)?;
    let path = fs::read_link(&executable)?;
    let path_text = path.to_string_lossy();
    let exe = PathBuf::from(path_text.strip_suffix(" (deleted)").unwrap_or(&path_text));
    let after = fs::read_to_string(directory.join("stat"))?;
    ensure!(
        stat_identity(&after)? == Some(start_time),
        "读取期间进程身份改变"
    );
    Ok(ProcessIdentity {
        start_time,
        exe,
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn read_record(data: &Path) -> Result<Option<ServerRecord>> {
    let bytes = match fs::read(data.join("server.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let record: ServerRecord =
        serde_json::from_slice(&bytes).context("无法核验启动记录；旧版记录须先用旧启动器停止")?;
    loopback_url(&record.url)?;
    Ok(Some(record))
}

fn verify_record(record: &ServerRecord) -> Result<bool> {
    match process_identity(record.pid)? {
        None => Ok(false),
        Some(identity) => {
            ensure!(
                identity == record.identity,
                "PID 已复用或进程身份改变，不会操作它"
            );
            Ok(true)
        }
    }
}

fn verify_info(value: &Value, config: &Config, pid: Option<u32>) -> Result<()> {
    ensure!(
        value["app"] == "codex-session-board" && value["dataDir"] == json!(config.data_dir),
        "端口或启动记录属于其他服务或数据目录"
    );
    if let Some(pid) = pid {
        ensure!(
            value["pid"].as_u64() == Some(u64::from(pid)),
            "服务 PID 与启动记录不一致"
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

fn save_record(data: &Path, record: &ServerRecord) -> Result<()> {
    config::write_json(
        &data.join("server.json"),
        &serde_json::to_value(record)?,
        true,
    )
}

fn matching_record(config: &Config, url: &str, build: &str) -> Result<bool> {
    let Some(record) = read_record(&config.data_dir)? else {
        return Ok(false);
    };
    if !verify_record(&record)? {
        return Ok(false);
    }
    let value = probe(&record.url)?.context("受管进程仍存活但暂不可访问，不会重复启动")?;
    verify_info(&value, config, Some(record.pid))?;
    ensure!(
        record.url.trim_end_matches('/') == url,
        "同一状态库仍有服务运行，请先 stop 再更换端口"
    );
    verify_version(&value, config, build)?;
    Ok(true)
}

fn adopt_running(config: &Config, url: &str, build: &str) -> Result<bool> {
    let Some(value) = probe(url)? else {
        return Ok(false);
    };
    verify_info(&value, config, None)?;
    verify_version(&value, config, build)?;
    let pid = u32::try_from(value["pid"].as_u64().context("服务未提供有效 PID")?)?;
    let identity = process_identity(pid)?.context("HTTP 服务所报进程未存活")?;
    let executable = fs::metadata(std::env::current_exe()?)?;
    ensure!(
        identity.device == executable.dev() && identity.inode == executable.ino(),
        "HTTP 服务所报进程不是当前可执行程序"
    );
    save_record(
        &config.data_dir,
        &ServerRecord {
            pid,
            url: url.into(),
            identity,
        },
    )?;
    Ok(true)
}

fn spawn_service(config_path: &Path, config: &Config, url: &str) -> Result<Child> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(config.data_dir.join("server.log"))?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("serve")
        .arg("--config")
        .arg(config::resolve_path(config_path)?)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()?;
    let saved = (|| -> Result<()> {
        let identity = process_identity(child.id())?.context("看板启动后立即退出")?;
        let record = ServerRecord {
            pid: child.id(),
            url: url.into(),
            identity,
        };
        save_record(&config.data_dir, &record)
    })();
    if let Err(error) = saved {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}

fn await_ready(child: &mut Child, config: &Config, url: &str, build: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(value) = probe(url)? {
            verify_info(&value, config, Some(child.id()))?;
            verify_version(&value, config, build)?;
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "看板启动失败 ({status})，请查看 {}",
                config.data_dir.join("server.log").display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("看板启动尚未确认，已保留受管进程记录；请检查 server.log")
}

pub fn launch(config_path: &Path) -> Result<Value> {
    let (_, default_data) = config::default_paths()?;
    config::initialize_config(config_path, &default_data)?;
    let config = config::load(Some(config_path))?;
    ensure!(config.port > 0, "启动器需要固定端口，port 不能为 0");
    let _lock = lock_data(&config.data_dir)?;
    let url = format!("http://127.0.0.1:{}", config.port);
    let build = config::build_id()?;
    let running = matching_record(&config, &url, &build)? || adopt_running(&config, &url, &build)?;
    if !running {
        let mut child = spawn_service(config_path, &config, &url)?;
        await_ready(&mut child, &config, &url, &build)?;
    }
    Ok(json!({
        "url": url, "running": true, "started": !running,
        "config": config::resolve_path(config_path)?
    }))
}

fn stop_record(record: &ServerRecord, config: &Config) -> Result<bool> {
    if !verify_record(record)? {
        return Ok(false);
    }
    let pid = Pid::from_raw(i32::try_from(record.pid)?).context("无效进程 PID")?;
    let descriptor =
        pidfd_open(pid, PidfdFlags::empty()).context("无法取得受管进程 pidfd，不会发送信号")?;
    let value = probe(&record.url)?.context("受管进程仍存活但无法核验服务身份")?;
    verify_info(&value, config, Some(record.pid))?;
    ensure!(verify_record(record)?, "受管进程在核验期间退出");
    pidfd_send_signal(&descriptor, Signal::INT)?;
    let timeout = config.rpc_timeout_seconds.saturating_add(10);
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(timeout))
        .context("停止等待时间过大")?;
    while Instant::now() < deadline {
        if !verify_record(record)? {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("服务进程退出尚未确认")
}

pub fn stop(config_path: &Path) -> Result<Value> {
    let config = config::load(Some(config_path))?;
    let _lock = lock_data(&config.data_dir)?;
    let record = read_record(&config.data_dir)?.context("没有找到本启动器创建的服务记录")?;
    let stopped = stop_record(&record, &config)?;
    Ok(json!({
        "running": false, "stopped": stopped,
        "config": config::resolve_path(config_path)?
    }))
}

fn copy_database(source: &Path, target: &Path) -> Result<()> {
    let source = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut target = Connection::open(target)?;
    let backup = Backup::new(&source, &mut target)?;
    backup.run_to_completion(128, Duration::from_millis(10), None)?;
    Ok(())
}

fn import_database(source: &Path, target: &Path) -> Result<bool> {
    ensure!(!target.try_exists()?, "目标状态库已经存在，不会覆盖");
    if !source.try_exists()? {
        return Ok(false);
    }
    let reserved = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)?;
    drop(reserved);
    if let Err(error) = copy_database(source, target) {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(true)
}

fn publish_migration(config_path: &Path, config: &Config, source: &Path) -> Result<bool> {
    let target = config.data_dir.join("status.sqlite3");
    let value = serde_json::to_value(config)?;
    let copied = import_database(source, &target)?;
    let result = config::write_json(config_path, &value, false);
    if result.is_err() && copied {
        fs::remove_file(&target).context("迁移失败且本次新建状态库未能清理")?;
    }
    result?;
    Ok(copied)
}

pub fn migrate(config_path: &Path, source_config: &Path) -> Result<Value> {
    ensure!(!config_path.try_exists()?, "目标配置已经存在，不会覆盖");
    let mut config = config::load(Some(source_config))?;
    let source = config.data_dir.join("status.sqlite3");
    let (_, destination) = config::default_paths()?;
    config.data_dir = destination;
    let _lock = lock_data(&config.data_dir)?;
    if let Some(record) = read_record(&config.data_dir)? {
        ensure!(
            !verify_record(&record)?,
            "目标数据目录仍有受管进程运行，不能迁移"
        );
    }
    let copied = publish_migration(config_path, &config, &source)?;
    Ok(json!({
        "imported": true, "databaseCopied": copied, "dataDir": config.data_dir,
        "config": config::resolve_path(config_path)?
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        value: Value,
        requests: usize,
    ) -> Result<(u16, thread::JoinHandle<Result<()>>)> {
        let server = tiny_http::Server::http("127.0.0.1:0")
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let port = server.server_addr().to_ip().context("缺少监听端口")?.port();
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
        Ok((port, handle))
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

    #[test]
    fn mismatched_process_identity_is_rejected_before_network_or_signals() -> Result<()> {
        let pid = std::process::id();
        let mut identity = process_identity(pid)?.context("测试进程应当存活")?;
        identity.start_time += 1;
        let record = ServerRecord {
            pid,
            url: "http://127.0.0.1:1".into(),
            identity,
        };
        assert!(verify_record(&record).is_err());
        assert!(stop_record(&record, &fixture_config(Path::new("/unused"))).is_err());
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
        let value = json!({"pid":123,"url":"http://127.0.0.1:8767"});
        config::write_json(&fixture.0.join("server.json"), &value, false)?;
        assert!(read_record(&fixture.0).is_err());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(fixture.0.join("server.json"))?)?,
            value
        );
        Ok(())
    }

    #[test]
    fn matching_service_reuses_verified_identity_and_rejects_port_change() -> Result<()> {
        let fixture = Fixture::new()?;
        let config = fixture_config(&fixture.0);
        let (port, server) = fixture_server(info(&config)?, 2)?;
        let url = format!("http://127.0.0.1:{port}");
        let identity = process_identity(std::process::id())?.context("测试进程应当存活")?;
        save_record(
            &fixture.0,
            &ServerRecord {
                pid: std::process::id(),
                url: url.clone(),
                identity,
            },
        )?;
        let build = config::build_id()?;
        assert!(matching_record(&config, &url, &build)?);
        assert!(matching_record(&config, "http://127.0.0.1:1", &build).is_err());
        join_server(server)?;
        Ok(())
    }

    #[test]
    fn live_inaccessible_process_cannot_be_launched_again() -> Result<()> {
        let fixture = Fixture::new()?;
        let config = fixture_config(&fixture.0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let url = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let identity = process_identity(std::process::id())?.context("测试进程应当存活")?;
        save_record(
            &fixture.0,
            &ServerRecord {
                pid: std::process::id(),
                url: url.clone(),
                identity,
            },
        )?;
        let error = matching_record(&config, &url, &config::build_id()?)
            .expect_err("HTTP 无响应必须阻止重复启动");
        assert!(error.to_string().contains("仍存活"));
        Ok(())
    }

    #[test]
    fn registration_supports_actual_ephemeral_port_and_parent_lock() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut config = fixture_config(&fixture.0);
        config.port = 0;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        register(&config, port)?;
        let record = read_record(&fixture.0)?.context("缺少注册记录")?;
        assert_eq!(record.url, format!("http://127.0.0.1:{port}"));
        assert!(verify_record(&record)?);
        let _parent = lock_data(&fixture.0)?;
        register(&config, port)?;
        assert!(register(&config, 0).is_err());
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
