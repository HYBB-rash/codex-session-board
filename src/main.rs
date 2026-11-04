use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use notion_program::clients::{ExternalError, Notion, RoutingExecutor, RpcClient, connect_notion};
use notion_program::common::{
    append_event, load_config, now, page_id, read_json, required_str, write_json,
};
use notion_program::{archive, clients, scheduler::StateScheduler};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(version, about = "Notion 任务调度与知识归档（Rust）")]
struct Cli {
    #[arg(long, global = true, default_value = "config.json")]
    config: PathBuf,
    #[arg(long, global = true, default_value = "manual_test")]
    trigger: String,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    #[command(about = "检查队列并推进一次调度")]
    Run,
    #[command(about = "只检查本地配置，不连接外部服务")]
    CheckConfig,
    #[command(about = "读取指定任务属性或完整页面，不派单")]
    Inspect {
        #[arg(long)]
        page: String,
        #[arg(long)]
        body: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    #[command(about = "只读检查 Notion 连接")]
    Probe,
    #[command(about = "归档和项目上下文")]
    Archive {
        #[command(subcommand)]
        command: ArchiveAction,
    },
    #[command(about = "将标准输入/输出 JSON RPC 转发到本地 WebSocket")]
    Bridge {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long, default_value_t = 8 * 1024 * 1024)]
        max_frame_bytes: usize,
    },
    #[command(about = "从 Python 项目迁移配置到新目录，不复制运行状态或启用服务")]
    ImportConfig {
        #[arg(long)]
        source: PathBuf,
        #[arg(long, default_value = ".")]
        destination: PathBuf,
    },
    #[command(about = "查看本地检查点和阶段统计，不连接外部服务")]
    Status,
    #[command(about = "发送一次显式指定的 App Server RPC 请求")]
    Rpc {
        #[arg(long)]
        method: String,
        #[arg(long, default_value = "{}")]
        params: String,
    },
}

#[derive(Subcommand)]
enum ArchiveAction {
    Run {
        #[arg(long)]
        probe: bool,
    },
    Context {
        #[arg(long)]
        task: String,
        #[arg(long)]
        output: PathBuf,
    },
}

fn check_config(config: &Value, is_archive: bool) -> Result<Value> {
    for key in [
        "workspace",
        "data_source_id",
        "executor",
        "mcp_server",
        "data_dir",
        "task_workspace_root",
    ] {
        required_str(config, key)?;
    }
    notion_program::common::canonical_id(required_str(config, "data_source_id")?)?;
    for role in ["fetch", "query", "update", "create"] {
        required_str(&config["tools"], role)?;
    }
    ensure!(
        config["daemon_socket"].is_string() || config["codex_command"].is_array(),
        "Configure daemon_socket or codex_command"
    );
    if let Some(command) = config.get("codex_command") {
        notion_program::common::command(command)?;
    }
    if is_archive {
        for key in [
            "archive_data_dir",
            "worker_cwd",
            "project_data_source_id",
            "analysis_model",
        ] {
            required_str(config, key)?;
        }
        notion_program::common::canonical_id(required_str(config, "project_data_source_id")?)?;
    } else {
        ensure!(
            config["model_map"]
                .as_object()
                .is_some_and(|models| !models.is_empty()),
            "model_map must be a nonempty object"
        );
        ensure!(
            config["model_map"]
                .as_object()
                .unwrap()
                .values()
                .all(|value| value.as_str().is_some_and(|text| !text.is_empty())),
            "Model mappings must be nonempty strings"
        );
    }
    if let Some(timeout) = config.get("request_timeout_seconds") {
        ensure!(
            timeout
                .as_f64()
                .is_some_and(|value| value.is_finite() && value > 0.0),
            "request_timeout_seconds must be positive"
        );
    }
    let attempts = match config.get("connection_attempts") {
        Some(value) => value
            .as_u64()
            .context("connection_attempts must be an integer")?,
        None => 2,
    };
    ensure!(
        (1..=3).contains(&attempts),
        "connection_attempts must be 1..3"
    );
    let delay = match config.get("connection_retry_delay_seconds") {
        Some(value) => value
            .as_f64()
            .context("connection_retry_delay_seconds must be a number")?,
        None => 2.0,
    };
    ensure!(
        (0.0..=30.0).contains(&delay),
        "connection_retry_delay_seconds must be 0..30"
    );
    Ok(json!({"status": "valid", "config": config["_config_path"], "archive": is_archive}))
}

fn run_scheduler(config: Value, trigger: String) -> Result<Value> {
    check_config(&config, false)?;
    let attempts = config["connection_attempts"].as_u64().unwrap_or(2);
    let delay = config["connection_retry_delay_seconds"]
        .as_f64()
        .unwrap_or(2.0);
    for attempt in 1..=attempts {
        let outcome = (|| -> Result<Vec<Value>> {
            let notion = connect_notion(&config)?;
            let executor = RoutingExecutor::new(config.clone())?;
            let mut scheduler =
                StateScheduler::new(config.clone(), notion, executor, trigger.clone())?;
            scheduler.run_once()
        })();
        match outcome {
            Ok(outcomes) => {
                if attempt == attempts
                    || !outcomes.iter().any(|item| item["reconnect_notion"] == true)
                {
                    return Ok(json!(outcomes));
                }
                append_event(
                    &PathBuf::from(required_str(&config, "data_dir")?).join("events.jsonl"),
                    &json!({"at": now(), "trigger": trigger, "action": "connection_reconnect", "attempt": attempt, "retrying": true}),
                )?;
            }
            Err(error) => {
                let retryable = error
                    .downcast_ref::<ExternalError>()
                    .is_some_and(|failure| failure.reconnectable && !failure.outcome_unknown);
                append_event(
                    &PathBuf::from(required_str(&config, "data_dir")?).join("events.jsonl"),
                    &json!({"at": now(), "trigger": trigger, "action": "queue_connection_error", "attempt": attempt, "retrying": retryable && attempt < attempts}),
                )?;
                if !retryable || attempt == attempts {
                    return Err(error);
                }
            }
        }
        thread::sleep(Duration::from_secs_f64(delay));
    }
    bail!("Connection retry budget exhausted")
}

fn import_config(source: &Path, destination: &Path) -> Result<Value> {
    let source = fs::canonicalize(source).context("Original project does not exist")?;
    let destination =
        fs::canonicalize(destination).context("Destination directory does not exist")?;
    ensure!(
        source != destination,
        "Source and destination must be separate directories"
    );
    let config_target = destination.join("config.json");
    let archive_target = destination.join("archive-config.json");
    ensure!(
        !config_target.exists() && !archive_target.exists(),
        "Destination configuration exists; edit or back it up before importing"
    );
    let executable = destination.join("target/release/notion-program");
    let mut config = read_json(&source.join("config.json"), Value::Null)?;
    ensure!(config.is_object(), "Source configuration is missing");
    let mut archiver = read_json(&source.join("archive-config.json"), Value::Null)?;
    let migrate = |config: &mut Value| -> Result<()> {
        config["workspace"] = json!(destination);
        config["data_dir"] = json!(destination.join("data"));
        config["task_workspace_root"] = json!(destination.join("tasks"));
        if let Some(command) = config["codex_command"].as_array()
            && let Some(position) = command.iter().position(|value| value == "--socket")
        {
            let socket = command
                .get(position + 1)
                .and_then(Value::as_str)
                .context("Missing source daemon socket")?
                .to_string();
            config["daemon_socket"] = json!(socket);
            config.as_object_mut().unwrap().remove("codex_command");
        } else if config["daemon_socket"].is_null() {
            ensure!(
                config["codex_command"].is_array(),
                "Original configuration has no transport"
            );
            let contains_python = config["codex_command"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .any(|argument| {
                    argument.ends_with(".py")
                        || Path::new(argument)
                            .file_name()
                            .is_some_and(|name| name.to_string_lossy().starts_with("python"))
                });
            ensure!(
                !contains_python,
                "Cannot migrate an unknown Python transport automatically"
            );
        }
        config.as_object_mut().unwrap().remove("_config_path");
        Ok(())
    };
    migrate(&mut config)?;
    if config["project_context"]["enabled"] == true {
        config["project_context"]["helper_command"] =
            json!([executable, "--config", archive_target, "archive"]);
    }
    if archiver.is_object() {
        migrate(&mut archiver)?;
        archiver["archive_data_dir"] = json!(destination.join("data/archive-runtime"));
        archiver["worker_cwd"] = json!(destination);
        write_json(&archive_target, &archiver)?;
    }
    write_json(&config_target, &config)?;
    Ok(json!({"status": "imported", "config": config_target,
        "archive_config": if archiver.is_object() { json!(archive_target) } else { Value::Null },
        "runtime_state_copied": false, "services_changed": false}))
}

fn execute(cli: Cli) -> Result<Value> {
    match &cli.command {
        Some(Action::Bridge {
            socket,
            max_frame_bytes,
        }) => {
            ensure!(*max_frame_bytes > 0, "max-frame-bytes must be positive");
            clients::bridge(socket, *max_frame_bytes)?;
            return Ok(Value::Null);
        }
        Some(Action::ImportConfig {
            source,
            destination,
        }) => return import_config(source, destination),
        _ => {}
    }
    let mut config = load_config(&cli.config)?;
    match cli.command.unwrap_or(Action::Run) {
        Action::Run => run_scheduler(config, cli.trigger),
        Action::CheckConfig => check_config(&config, config.get("archive_data_dir").is_some()),
        Action::Inspect { page, body, output } => {
            let pid = page_id(&page)?;
            let mut notion = connect_notion(&config)?;
            let result = if body {
                notion.fetch(&pid)?
            } else {
                notion.get_task_metadata(&pid)?
            };
            if let Some(output) = output {
                write_json(&output, &result)?;
                Ok(json!({"page_id": pid, "saved": output}))
            } else {
                Ok(result)
            }
        }
        Action::Probe => {
            let mut notion = connect_notion(&config)?;
            let result = notion.fetch(&format!(
                "collection://{}",
                required_str(&config, "data_source_id")?
            ))?;
            let path =
                PathBuf::from(required_str(&config, "data_dir")?).join("live-fetch-probe.json");
            write_json(&path, &result)?;
            Ok(json!({"status": "connected", "saved": path}))
        }
        Action::Archive { command } => match command {
            ArchiveAction::Run { probe } => {
                config["_notification_probe"] = json!(probe);
                config["_probe"] = json!(probe);
                archive::run(&config)
            }
            ArchiveAction::Context { task, output } => {
                let pid = page_id(&task)?;
                match archive::context_for_task(&config, &pid) {
                    Ok(value) => {
                        write_json(&output, &value)?;
                        Ok(value)
                    }
                    Err(error) => {
                        write_json(
                            &output,
                            &json!({"contract": "project_context_v1", "status": "blocked", "task_page_id": pid, "at": now(), "reason": error.to_string()}),
                        )?;
                        Err(error)
                    }
                }
            }
        },
        Action::Status => {
            let state = read_json(
                &PathBuf::from(required_str(&config, "data_dir")?).join("state.json"),
                json!({}),
            )?;
            let mut counts = serde_json::Map::new();
            for record in state
                .as_object()
                .context("State must be an object")?
                .values()
            {
                let phase = record["phase"].as_str().unwrap_or("pending");
                let count = counts.get(phase).and_then(Value::as_u64).unwrap_or(0);
                counts.insert(phase.into(), json!(count + 1));
            }
            Ok(json!({"tasks": state.as_object().unwrap().len(), "phases": counts, "state": state}))
        }
        Action::Rpc { method, params } => {
            RpcClient::from_config(&config)?.request(&method, &serde_json::from_str(&params)?)
        }
        Action::Bridge { .. } | Action::ImportConfig { .. } => unreachable!(),
    }
}

fn main() {
    match execute(Cli::parse()) {
        Ok(value) => {
            if !value.is_null() {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
        }
        Err(error) => {
            eprintln!("notion-program: {error:#}");
            std::process::exit(2);
        }
    }
}
