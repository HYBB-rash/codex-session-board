use super::App;
use crate::config;
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Stdio};

const STATUS: &str = "board-update.json";
const RUNNING: &[&str] = &["fetching", "verifying", "installing", "reloading"];

impl App {
    pub(super) fn plugin_update(&self, body: Option<&Value>) -> Result<Value> {
        let Some(updater) = std::env::var_os("CODEX_SESSION_BOARD_UPDATER") else {
            ensure!(body.is_none(), "更新只在已安装的插件中可用");
            return Ok(json!({"available":false}));
        };
        let (_, data) = config::default_paths()?;
        let state = config::resolve_path(&data.join("installation"))?;
        let build = config::build_id()?;
        if let Some(body) = body {
            ensure!(body.as_object().is_some_and(|fields| fields.is_empty()), "更新不接受参数");
            self.start_plugin_update(Path::new(&updater), &state, &build)?;
        }
        let mut status = status(&state, &build)?;
        if status["phase"] == "active" || status["phase"] == "up_to_date" {
            self.snapshot().context("新版运行时尚未成功读回看板")?;
        }
        status["available"] = json!(true);
        status["runningVersion"] = json!(env!("CARGO_PKG_VERSION"));
        Ok(status)
    }

    fn start_plugin_update(&self, updater: &Path, state: &Path, build: &str) -> Result<()> {
        let mut command = Command::new("python3");
        command.args(["-EBS"]).arg(updater).arg(state)
            .arg(&self.config.codex_command[0]).arg(build).stdin(Stdio::null());
        command.env_remove("CODEX_SESSION_BOARD_UPDATE_AUTHORITY");
        if let Some(authority) = &self.config.local_authority {
            command.env("CODEX_SESSION_BOARD_UPDATE_AUTHORITY", serde_json::to_string(authority)?);
        }
        if !self.config.desktop_endpoint().as_os_str().is_empty() {
            command.env("CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET", self.config.desktop_endpoint());
        }
        let result = command.output().context("无法启动更新程序，请确认本机已安装 Python 3")?;
        ensure!(result.status.success(), "无法启动更新程序：{}", String::from_utf8_lossy(&result.stderr).lines().last().unwrap_or("未知错误"));
        Ok(())
    }
}

fn updating(state: &Path) -> Result<bool> {
    let lock = match File::open(state.join("update.lock")) {
        Ok(lock) => lock,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    match FileExt::try_lock_shared(&lock) {
        Ok(()) => Ok(false),
        Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(error.into()),
    }
}

fn status(state: &Path, build: &str) -> Result<Value> {
    let busy = updating(state)?;
    let mut result: Value = match fs::read(state.join(STATUS)) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == ErrorKind::NotFound => json!({"phase":"idle"}),
        Err(error) => return Err(error.into()),
    };
    let phase = result["phase"].as_str().context("更新回执缺少阶段")?;
    if !busy && RUNNING.contains(&phase) {
        result["failedStage"] = json!(phase);
        result["phase"] = json!("failed");
        result["error"] = json!("更新进程已退出，未留下最终回执");
    } else if result["installed"] == true {
        let phase = match (phase, result["build"] == build) {
            ("up_to_date", true) => "up_to_date",
            (_, true) => "active",
            ("desktop_loaded" | "awaiting_board_readback", false) => "reopen_board",
            _ => "awaiting_desktop_reload",
        };
        result["phase"] = json!(phase);
    }
    result["busy"] = json!(busy);
    Ok(result)
}
