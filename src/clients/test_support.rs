use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub(super) const SOURCE: &str = "11111111-1111-4111-8111-111111111111";
pub(super) const THREAD: &str = "33333333-3333-4333-8333-333333333333";
pub(super) const MARKER: &str = "notion-program/11111111-1111-4111-8111-111111111111/round-002";

pub(super) fn row(number: usize) -> Value {
    json!({
        "url": format!("https://www.notion.so/{number:032x}"),
        "任务名称": format!("任务 {number}"), "状态": "待办事项",
        "执行人 / 代理": "Fedora Codex", "AI 会话 / 线程": "", "模型": "model"
    })
}

pub(super) fn page(content: &str) -> Value {
    json!({
        "id": SOURCE,
        "text": format!(
            "<page url=\"https://notion.so/{SOURCE}\">\n<parent-data-source url=\"collection://source-id\" />\n<properties>{{\"任务名称\":\"测试\",\"执行人 / 代理\":\"Fedora Codex\"}}</properties>\n<content>{content}</content>\n</page>"
        )
    })
}

pub(super) fn snapshot(identifier: &str, status: &str, prompt: &str, owner: &str) -> Value {
    json!({
        "thread": {"id": THREAD, "status": {"type": owner, "activeFlags": []}},
        "turns": [{
            "id": identifier, "status": status, "items": [
                {"type": "userMessage", "content": [{"type": "text", "text": prompt}]},
                {"id": "final", "type": "agentMessage", "phase": "final_answer", "text": "完整结果"}
            ]
        }]
    })
}

pub(super) fn config(options: Value) -> Value {
    json!({
        "codex_command": [protocol_server(), options.to_string()],
        "request_timeout_seconds": 2,
        "workspace": "/tmp", "data_source_id": "source-id", "executor": "Fedora Codex",
        "mcp_server": "explicit-server",
        "tools": {
            "fetch": "fetch-tool", "query": "query-tool", "update": "update-tool",
            "create": "create-tool", "access": "access-tool"
        }
    })
}

fn protocol_server() -> &'static Path {
    static SERVER: OnceLock<PathBuf> = OnceLock::new();
    SERVER.get_or_init(|| {
        let executable = std::env::current_exe().expect("Locate the running unit test");
        let profile = executable
            .parent()
            .and_then(Path::parent)
            .expect("Unit test executable must be under the profile's deps directory");
        let server = profile.join(format!(
            "notion-program-test-server{}",
            std::env::consts::EXE_SUFFIX
        ));
        if !server.is_file() {
            build_protocol_server(profile, &server);
        }
        server
    })
}

fn build_protocol_server(profile: &Path, server: &Path) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let target = profile
        .parent()
        .expect("Cargo profile must have a target directory");
    let mut build = Command::new(cargo);
    build
        .args([
            "build",
            "--offline",
            "--locked",
            "--bin",
            "notion-program-test-server",
        ])
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target);
    match profile.file_name().and_then(|name| name.to_str()) {
        Some("debug") => {}
        Some("release") => {
            build.arg("--release");
        }
        Some(name) => {
            build.args(["--profile", name]);
        }
        None => panic!("Cargo profile directory has no name"),
    }
    let output = build.output().expect("Build the Rust protocol test server");
    assert!(
        output.status.success(),
        "Cannot build the Rust protocol test server:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        server.is_file(),
        "Cargo did not create {}",
        server.display()
    );
}
