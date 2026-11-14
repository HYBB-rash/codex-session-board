#[path = "support/fixture.rs"]
#[allow(dead_code)]
mod fixture;

use anyhow::{Context, Result, ensure};
use codex_session_board::{app::App, config::Config};
use fixture::PortLease;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const THREAD: &str = "11111111-1111-4111-8111-111111111111";

struct Plugin {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Plugin {
    fn start(path: Option<&Path>, home: Option<&Path>) -> Result<Self> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-session-board"));
        command.env_remove("CODEX_APP_SERVER_WS_URL");
        Self::launch(command, path, home)
    }

    fn launch(mut command: Command, path: Option<&Path>, home: Option<&Path>) -> Result<Self> {
        command.arg("mcp").env_remove("CODEX_SESSION_BOARD_CONFIG")
            .stdin(Stdio::piped()).stdout(Stdio::piped());
        if let Some(path) = path {
            command.arg("--config").arg(path);
        }
        if let Some(home) = home {
            command.env("HOME", home).env("XDG_CONFIG_HOME", home.join("config"))
                .env("XDG_DATA_HOME", home.join("share"));
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().context("缺少插件输入")?;
        let output = BufReader::new(child.stdout.take().context("缺少插件输出")?);
        Ok(Self { child, input: Some(input), output })
    }

    fn call(&mut self, name: &str, arguments: Value) -> Result<Value> {
        writeln!(self.input.as_mut().context("插件输入已关闭")?, "{}",
            json!({"id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}}))?;
        let mut line = String::new();
        ensure!(self.output.read_line(&mut line)? > 0, "插件意外退出");
        let response: Value = serde_json::from_str(&line)?;
        ensure!(response.get("error").is_none(), "{response}");
        Ok(response["result"].clone())
    }

    fn snapshot(&mut self) -> Result<Value> {
        let response = self.call("board_snapshot", json!({}))?;
        ensure!(response["isError"] != true, "{response}");
        Ok(response["structuredContent"].clone())
    }

    fn close(&mut self) -> Result<()> {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(status.success(), "插件关闭失败：{status}");
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "插件未随输入关闭退出");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[path = "plugin_runtime/authority.rs"]
mod authority;

impl Drop for Plugin {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct Profile {
    _port: Arc<PortLease>,
    config: Config,
    path: PathBuf,
}

impl Profile {
    fn new(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let port = PortLease::acquire()?;
        let config = Config {
            data_dir: root.join("data"), port: port.port, sync_titles: false,
            desktop_socket: PathBuf::new(), sidebar_socket: PathBuf::new(),
            sidebar_caller: String::new(), codex_command: vec!["/usr/bin/false".into()],
            rpc_timeout_seconds: 1, poll_seconds: 1, ..Config::default()
        };
        let path = root.join("config.json");
        std::fs::write(&path, serde_json::to_vec(&config)?)?;
        Ok(Self { _port: port, config, path })
    }

    fn seed(&self, note: &str) -> Result<()> {
        let app = App::new(self.config.clone())?;
        app.store.discover(&[json!({"id":THREAD,"name":"fixture","updatedAt":1})])?;
        app.store.set_state(THREAD, Some("review"), note, 0, Some(0))?;
        Ok(())
    }
}

#[test]
fn mcp_processes_share_one_owner_and_take_over_after_owner_exits() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = Profile::new(directory.path())?;
    profile.seed("preserved note")?;
    let mut owner = Plugin::start(Some(&profile.path), None)?;
    let first = owner.snapshot()?;
    assert_eq!(first["runtime"]["pid"], owner.child.id());
    let mut follower = Plugin::start(Some(&profile.path), None)?;
    assert_eq!(follower.snapshot()?["runtime"], first["runtime"]);
    let row = &first["sessions"][0];
    let response = follower.call("board_update", json!({"id":THREAD,"state":"review",
        "note":"updated through follower","version":row["version"],"inputVersion":row["inputVersion"]}))?;
    assert_ne!(response["isError"], true, "{response}");
    assert_eq!(owner.snapshot()?["sessions"][0]["note"], "updated through follower");
    owner.close()?;
    let next = follower.snapshot()?;
    assert_eq!(next["runtime"]["pid"], follower.child.id());
    assert_eq!(next["sessions"][0]["note"], "updated through follower");
    follower.close()?;
    assert!(std::net::TcpStream::connect(("127.0.0.1", profile.config.port)).is_err());
    Ok(())
}

#[test]
fn changed_data_directory_cannot_create_another_owner_for_the_same_entry() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = Profile::new(&directory.path().join("profile"))?;
    let other = Profile::new(&directory.path().join("other"))?;
    let mut owner = Plugin::start(Some(&profile.path), None)?;
    owner.snapshot()?;
    std::fs::write(&profile.path, serde_json::to_vec(&other.config)?)?;
    let mut follower = Plugin::start(Some(&profile.path), None)?;
    let error = follower.call("board_snapshot", json!({}))?;
    assert_eq!(error["isError"], true, "{error}");
    assert!(error["content"][0]["text"].as_str().unwrap_or("").contains("正式实例仍存活"));
    assert!(!other.config.data_dir.exists());
    assert_eq!(owner.snapshot()?["runtime"]["dataDir"], profile.config.data_dir.to_string_lossy().as_ref());
    owner.close()?;
    assert_eq!(follower.snapshot()?["runtime"]["dataDir"], other.config.data_dir.to_string_lossy().as_ref());
    follower.close()
}

fn legacy_installation(home: &Path, profile: &Profile) -> Result<()> {
    let state = home.join("share/codex-session-board/installation");
    std::fs::create_dir_all(&state)?;
    std::fs::write(state.join("current.json"), serde_json::to_vec(&json!({
        "phase":"desktop_loaded","legacy_installation":{"config":profile.path}
    }))?)?;
    Ok(())
}

#[test]
fn simultaneous_plugin_entries_elect_one_business_owner() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = Profile::new(directory.path())?;
    profile.seed("one shared runtime")?;
    let mut entries = Vec::new();
    for _ in 0..4 {
        let path = profile.path.clone();
        entries.push(std::thread::spawn(move || -> Result<(Plugin, Value)> {
            let mut plugin = Plugin::start(Some(&path), None)?;
            let snapshot = plugin.snapshot()?;
            Ok((plugin, snapshot))
        }));
    }
    let mut plugins = Vec::new();
    for entry in entries {
        plugins.push(entry.join().map_err(|_| anyhow::anyhow!("插件接入线程异常"))??);
    }
    let owner = plugins[0].1["runtime"]["pid"].as_u64().context("缺少业务所有者")?;
    assert!(plugins.iter().any(|(plugin, _)| u64::from(plugin.child.id()) == owner));
    for (_, snapshot) in &plugins {
        assert_eq!(snapshot["runtime"]["pid"], owner);
        assert_eq!(snapshot["sessions"][0]["note"], "one shared runtime");
    }
    plugins.sort_by_key(|(plugin, _)| u64::from(plugin.child.id()) == owner);
    for (mut plugin, _) in plugins {
        plugin.close()?;
    }
    Ok(())
}

#[test]
fn first_upgrade_imports_recorded_configuration_instead_of_old_default_cache() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let home = directory.path();
    let formal = Profile::new(&home.join("formal"))?;
    let old = Profile::new(&home.join("config/codex-session-board"))?;
    formal.seed("correct production note")?;
    old.seed("wrong historical cache")?;
    legacy_installation(home, &formal)?;
    let mut plugin = Plugin::start(None, Some(home))?;
    let snapshot = plugin.snapshot()?;
    assert_eq!(snapshot["sessions"][0]["note"], "correct production note");
    assert_eq!(snapshot["runtime"]["dataDir"], formal.config.data_dir.to_string_lossy().as_ref());
    assert!(std::net::TcpStream::connect(("127.0.0.1", old.config.port)).is_err());
    plugin.close()?;
    let mut updated = formal.config.clone();
    updated.poll_seconds = 37;
    std::fs::write(&old.path, serde_json::to_vec(&updated)?)?;
    let mut plugin = Plugin::start(None, Some(home))?;
    plugin.snapshot()?;
    plugin.close()?;
    let kept: Config = serde_json::from_slice(&std::fs::read(&old.path)?)?;
    assert_eq!(kept.poll_seconds, 37);
    assert_eq!(App::new(old.config.clone())?.store.sessions()?[0]["note"], "wrong historical cache");
    Ok(())
}

#[test]
fn missing_recorded_production_config_does_not_initialize_a_default_database() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let formal = Profile::new(&directory.path().join("formal"))?;
    legacy_installation(directory.path(), &formal)?;
    std::fs::remove_file(&formal.path)?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .arg("mcp").env_remove("CODEX_SESSION_BOARD_CONFIG")
        .env("HOME", directory.path()).env("XDG_CONFIG_HOME", directory.path().join("config"))
        .env("XDG_DATA_HOME", directory.path().join("share"))
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let status = child.wait()?;
    assert!(!status.success());
    assert!(!directory.path().join("share/codex-session-board/status.sqlite3").exists());
    Ok(())
}
