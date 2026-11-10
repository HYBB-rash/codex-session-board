use anyhow::{Result, bail, ensure};
use codex_session_board::{config, service};
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn launch(config_path: &Path) -> Result<Value> {
    let (_, default_data) = config::default_paths()?;
    config::initialize_config(config_path, &default_data)?;
    let config = config::load(Some(config_path))?;
    ensure!(config.port > 0, "启动器需要固定端口，port 不能为 0");
    let _lock = service::lock_data(&config.data_dir)?;
    let url = format!("http://127.0.0.1:{}", config.port);
    let build = config::build_id()?;
    let running = service::matching_record(&config, &url, &build)?
        || service::adopt_running(&config, &url, &build)?;
    if !running {
        let mut child = spawn_service(config_path, &config, &url)?;
        await_ready(&mut child, &config, &url, &build)?;
    }
    Ok(json!({
        "url": url, "running": true, "started": !running,
        "config": config::resolve_path(config_path)?
    }))
}

fn spawn_service(config_path: &Path, config: &config::Config, url: &str) -> Result<Child> {
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
    if let Err(error) = service::save_started_process(config, child.id(), url) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}

fn await_ready(child: &mut Child, config: &config::Config, url: &str, build: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(value) = service::probe(url)? {
            service::verify_info(&value, config, Some(child.id()))?;
            service::verify_version(&value, config, build)?;
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
