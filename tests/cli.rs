use anyhow::Result;
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_notion-program"))
}

#[test]
fn help_runs_without_configuration() -> Result<()> {
    let output = binary().arg("--help").output()?;
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout)?;
    for command in [
        "run",
        "archive",
        "inspect",
        "bridge",
        "import-config",
        "check-config",
    ] {
        assert!(help.contains(command));
    }
    Ok(())
}

#[test]
fn imports_config_without_copying_runtime_state_or_python_dependency() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("original");
    let destination = directory.path().join("rust");
    fs::create_dir_all(source.join("data"))?;
    fs::create_dir_all(&destination)?;
    let original = json!({
        "version": 1, "workspace": "/old", "data_dir": "/old/data", "task_workspace_root": "/old/tasks",
        "data_source_id": "11111111-1111-4111-8111-111111111111",
        "executor": "Fedora Codex", "model_map": {"model": "group/model"},
        "codex_command": ["/old/.venv/bin/python", "/old/daemon_bridge.py", "--socket", "/tmp/test-notion-no-service.sock"],
        "mcp_server": "codex_apps", "tools": {"fetch": "notion.fetch", "query": "notion.query", "update": "notion.update", "create": "notion.create"},
        "project_context": {"enabled": true, "helper_command": ["python", "archive_runtime.py"]}
    });
    let original_bytes = serde_json::to_vec(&original)?;
    fs::write(source.join("config.json"), &original_bytes)?;
    fs::write(
        source.join("data/state.json"),
        br#"{"original":"preserve"}"#,
    )?;
    let result = binary()
        .arg("import-config")
        .arg("--source")
        .arg(&source)
        .arg("--destination")
        .arg(&destination)
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(source.join("config.json"))?, original_bytes);
    assert!(!destination.join("data").exists());
    let config: Value = serde_json::from_slice(&fs::read(destination.join("config.json"))?)?;
    assert!(config.get("codex_command").is_none());
    assert_eq!(config["workspace"], json!(destination));
    assert_eq!(config["daemon_socket"], "/tmp/test-notion-no-service.sock");
    assert!(
        config["project_context"]["helper_command"][0]
            .as_str()
            .unwrap()
            .ends_with("target/release/notion-program")
    );
    let check = binary()
        .arg("--config")
        .arg(destination.join("config.json"))
        .arg("check-config")
        .output()?;
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let second = binary()
        .arg("import-config")
        .arg("--source")
        .arg(&source)
        .arg("--destination")
        .arg(&destination)
        .output()?;
    assert!(!second.status.success());
    Ok(())
}

#[test]
fn local_status_does_not_connect_to_daemon() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("data"))?;
    fs::write(
        directory.path().join("config.json"),
        br#"{"version":1,"data_dir":"data","daemon_socket":"/does-not-exist"}"#,
    )?;
    fs::write(
        directory.path().join("data/state.json"),
        br#"{"task":{"phase":"waiting_execution","custom":{"preserve":true}}}"#,
    )?;
    let output = binary()
        .arg("--config")
        .arg(directory.path().join("config.json"))
        .arg("status")
        .output()?;
    assert!(output.status.success());
    let status: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(status["tasks"], 1);
    assert_eq!(status["phases"]["waiting_execution"], 1);
    assert_eq!(status["state"]["task"]["custom"]["preserve"], true);
    Ok(())
}

#[test]
fn paused_archiver_and_invalid_bridge_do_not_connect() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("archive-config.json");
    fs::write(
        &config,
        br#"{"version":1,"enabled":false,"daemon_socket":"/does-not-exist"}"#,
    )?;
    let output = binary()
        .arg("--config")
        .arg(&config)
        .args(["archive", "run"])
        .output()?;
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["status"],
        "paused"
    );
    let invalid = binary()
        .args([
            "bridge",
            "--socket",
            "/does-not-exist",
            "--max-frame-bytes",
            "0",
        ])
        .output()?;
    assert!(!invalid.status.success());
    assert!(String::from_utf8(invalid.stderr)?.contains("positive"));
    Ok(())
}
