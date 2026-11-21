use super::*;
#[path = "../support/read_only_backend.rs"]
mod read_only_backend;
use read_only_backend::Backend;

impl Backend {
    fn plugin(&self, profile: &Profile) -> Result<Plugin> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-session-board"));
        command.env("CODEX_APP_SERVER_WS_URL", format!("ws+unix://localhost{}:/rpc", self.socket.display()));
        Plugin::launch(command, Some(&profile.path), None)
    }
}

fn until(plugin: &mut Plugin, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let snapshot = plugin.snapshot()?;
        if predicate(&snapshot) { return Ok(snapshot); }
        ensure!(Instant::now() < deadline, "未观察到目标状态：{snapshot}");
        std::thread::sleep(Duration::from_millis(80));
    }
}

#[test]
fn inherited_shared_backend_drives_new_tasks_and_unloaded_projection_cannot_block_them() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut profile = Profile::new(directory.path())?;
    profile.config.runtime_poll_seconds = 1;
    profile.config.desktop_socket = directory.path().join("stale-daemon.sock");
    std::fs::write(&profile.path, serde_json::to_vec(&profile.config)?)?;
    let backend = Backend::start(directory.path(), THREAD)?;
    let mut plugin = backend.plugin(&profile)?;
    let active = until(&mut plugin, |snapshot| snapshot["sessions"][0]["state"] == "in_progress")?;
    assert_eq!(active["health"]["runtimeVerified"], true);
    assert_eq!(active["runtime"]["authority"]["pid"], std::process::id());
    assert_eq!(active["runtime"]["configId"], codex_session_board::config::config_id(
        &codex_session_board::config::load(Some(&profile.path))?)?);
    backend.set("notLoaded", "interrupted")?;
    let unloaded = until(&mut plugin, |snapshot| snapshot["sessions"][0]["turnStatus"] == "interrupted")?;
    assert_eq!(unloaded["sessions"][0]["state"], "in_progress");
    backend.set("notLoaded", "completed")?;
    until(&mut plugin, |snapshot| snapshot["sessions"][0]["state"] == "review")?;
    backend.set("idle", "interrupted")?;
    until(&mut plugin, |snapshot| snapshot["sessions"][0]["state"] == "blocked")?;
    plugin.close()
}

#[test]
fn a_shared_runtime_can_be_stopped_without_inheriting_its_ephemeral_socket() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = Profile::new(directory.path())?;
    let backend = Backend::start(directory.path(), THREAD)?;
    let mut plugin = backend.plugin(&profile)?;
    until(&mut plugin, |snapshot| snapshot["health"]["runtimeVerified"] == true)?;
    let output = Command::new(env!("CARGO_BIN_EXE_codex-session-board"))
        .arg("--config").arg(&profile.path).arg("stop")
        .env_remove("CODEX_APP_SERVER_WS_URL").output()?;
    ensure!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stopped: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(stopped["stopped"], true);
    plugin.input.take();
    plugin.child.wait()?;
    assert!(std::net::TcpStream::connect(("127.0.0.1", profile.config.port)).is_err());
    Ok(())
}

#[test]
fn a_different_client_parent_cannot_reuse_an_existing_board_owner() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = Profile::new(directory.path())?;
    let mut owner = Plugin::start(Some(&profile.path), None)?;
    owner.snapshot()?;
    let mut command = Command::new("python3");
    command.args(["-B", "-c", "import subprocess,sys\nchild=subprocess.Popen(sys.argv[1:])\nsys.exit(child.wait())"])
        .arg(env!("CARGO_BIN_EXE_codex-session-board"))
        .env_remove("CODEX_APP_SERVER_WS_URL");
    let mut other = Plugin::launch(command, Some(&profile.path), None)?;
    let response = other.call("board_snapshot", json!({}))?;
    assert_eq!(response["isError"], true);
    assert!(response["content"][0]["text"].as_str().unwrap_or("").contains("其他客户端后台"), "{response}");
    other.close()?;
    owner.close()
}
