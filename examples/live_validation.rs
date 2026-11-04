use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use notion_program::clients::{Notion, connect_notion};
use notion_program::common::{canonical_id, load_config, lock, now, read_json, write_json};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    config: PathBuf,
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    CreateProject {
        #[arg(long)]
        source: String,
        #[arg(long)]
        record: PathBuf,
    },
    CreateTask {
        #[arg(long)]
        project: String,
        #[arg(long, default_value = "GPT-6 Astra")]
        model: String,
        #[arg(long)]
        archive: bool,
        #[arg(long)]
        record: PathBuf,
    },
    Transition {
        #[arg(long)]
        record: PathBuf,
        #[arg(long)]
        to: String,
    },
    Fetch {
        #[arg(long)]
        id: String,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config(&cli.config)?;
    let mut notion = connect_notion(&config)?;
    let output = match cli.action {
        Action::Fetch { id, output } => {
            let raw = notion.fetch(&id)?;
            write_json(&output, &raw)?;
            json!({"saved": output})
        }
        Action::Transition { record, to } => {
            let Some(_guard) = lock(&record.with_extension("lock"))? else {
                bail!("Validation record is locked");
            };
            let mut state = read_json(&record, Value::Null)?;
            ensure!(
                matches!(state["kind"].as_str(), Some("dispatch" | "archive")),
                "Not a validation task"
            );
            let pid = canonical_id(
                state["page_id"]
                    .as_str()
                    .context("Missing validation page")?,
            )?;
            let metadata = notion.get_task_metadata(&pid)?;
            ensure!(
                metadata["source_id"] == config["data_source_id"],
                "Task belongs to another tracker"
            );
            ensure!(
                metadata["properties"]["执行人 / 代理"] == config["executor"],
                "Executor changed"
            );
            ensure!(
                (state["kind"] == "dispatch" && to == "待办事项")
                    || (state["kind"] == "archive" && to == "已完成待归档"),
                "Only the fixture's initial test transition is allowed"
            );
            if metadata["properties"]["状态"] == to {
                state["phase"] = json!("activated");
            } else {
                ensure!(
                    state["phase"] == "created",
                    "Previous status write needs reconciliation"
                );
                ensure!(
                    metadata["properties"]["状态"] == "计划中",
                    "Fixture is no longer planned"
                );
                let body = notion.get_task(&pid)?;
                ensure!(
                    body["content"].as_str().is_some_and(
                        |content| content.contains(state["marker"].as_str().unwrap_or("\0"))
                    ),
                    "Validation ownership marker is missing"
                );
                state["phase"] = json!("setting_status");
                state["target_status"] = json!(to);
                write_json(&record, &state)?;
                let response = notion.update_properties(&pid, &json!({"状态": to}))?;
                state["status_response"] = response;
                write_json(&record, &state)?;
                let verified = notion.get_task_metadata(&pid)?;
                ensure!(
                    verified["properties"]["状态"] == to,
                    "Status readback not yet confirmed"
                );
                state["phase"] = json!("activated");
            }
            state["activated_at"] = json!(now());
            write_json(&record, &state)?;
            state
        }
        action => {
            let (record, kind, parent, title, properties, content) = match action {
                Action::CreateProject { source, record } => {
                    let source = canonical_id(&source)?;
                    let schema = notion.fetch(&format!("collection://{source}"))?;
                    ensure!(
                        schema["text"]
                            .as_str()
                            .is_some_and(|text| text.contains("\"项目名称\"")),
                        "Project title property is missing"
                    );
                    (
                        record, "project", json!({"data_source_id": source}),
                        "Rust v0.1.0 上线验证（测试项目）".to_string(),
                        json!({"项目名称": "Rust v0.1.0 上线验证（测试项目）", "状态": "进行中", "优先级": "低"}),
                        "此项目仅用于 Herman 授权的 Rust v0.1.0 真实线上验证。测试任务、上下文、专题和归档回执均保留于本项目，不混入实际业务项目。\n\n测试结果以实际执行、文件、收据及读回证据为准；自动化测试不代表 Herman 已人工验收。\n".to_string(),
                    )
                }
                Action::CreateTask {
                    project,
                    model,
                    archive,
                    record,
                } => {
                    let project = canonical_id(&project)?;
                    ensure!(
                        config["model_map"][&model].is_string(),
                        "Selected model has no mapping"
                    );
                    let title = if archive {
                        "Rust v0.1.0 上线验证｜归档样本"
                    } else {
                        "Rust v0.1.0 上线验证｜真实派发与交付"
                    };
                    let content = if archive {
                        "## 归档测试资料\n\n这是一份专门用于验证归档器的测试样本，不是业务任务，也不代表人工审核通过。\
                         测试控制器将它从计划中放入已完成待归档，作为归档器的输入。\n\n\
                         ## 已验证的技术结论\n\nRust v0.1.0 调度器通过持久化检查点区分准备、发送、确认启动和提交状态。\
                         发送结果不明时先核对原派发，不直接重发。只有待办事项可以启动新轮；审批、用户输入和执行结果不明不应靠自动续试绕过。\n\n\
                         ## 验证范围\n\n本样本用于检查真实的来源读取、归档分析、项目上下文与专题写入、收据读回和最终已归档状态。\
                         它不证明重启、长期运行或人工审核已通过。\n".to_string()
                    } else {
                        "## 任务要求\n\n这是 Herman 授权的 Rust v0.1.0 真实线上测试。仅处理本任务及所属测试项目，\
                         不修改调度器程序、配置、服务或其他真实任务。\n\n\
                         在本任务工作目录编写 Rust 文件，实际计算数组 [2,4,6,8] 的数量、总和及平均值；\
                         用 rustc 编译并运行，断言结果依次为 4、20、5。保存 Rust 源文件和实际结果 JSON，写简短中文报告，\
                         列出编译运行命令、实际结果及校验。按派发契约创建或复用本任务历史索引和本轮独立文章，\
                         读回核验后提交本地收据。遵循项目上下文 helper 的读取及声明要求。\n\n\
                         ## 完成标准\n\n真实 Rust 程序运行且断言通过；代码和结果文件存在；报告、历史与文章可从任务页打开；\
                         当前轮收据身份和 SHA256 与最终消息一致。由调度器提交待审核，不能自行声称 Herman 已验收。\n\n\
                         ## 审阅区\n\n待 Herman 填写。\n".to_string()
                    };
                    (
                        record,
                        if archive { "archive" } else { "dispatch" },
                        json!({"data_source_id": config["data_source_id"]}),
                        title.to_string(),
                        json!({"任务名称": title, "状态": "计划中", "执行人 / 代理": config["executor"],
                            "模型": model, "优先级": "低", "工作量等级": "小",
                            "所属项目": serde_json::to_string(&vec![format!("https://app.notion.com/p/{project}")])?}),
                        content,
                    )
                }
                _ => unreachable!(),
            };
            let Some(_guard) = lock(&record.with_extension("lock"))? else {
                bail!("Validation record is locked");
            };
            ensure!(
                !record.exists(),
                "Creation is already reserved; reconcile the existing record"
            );
            let marker = format!("notion-program/live-validation/{}", Uuid::new_v4());
            let content = format!("{content}\n测试归属标记：`{marker}`\n");
            notion.fetch("notion://docs/notion-flavored-markdown-spec")?;
            let mut state = json!({"phase": "creating", "kind": kind, "marker": marker,
                "parent": parent, "title": title, "properties": properties, "content": content, "at": now()});
            write_json(&record, &state)?;
            let response = notion.call(
                "create",
                &json!({"parent": parent, "allow_async": false,
                "pages": [{"properties": properties, "content": content}]}),
            )?;
            state["response"] = response;
            write_json(&record, &state)?;
            let pages = state["response"]["pages"]
                .as_array()
                .context("Creation did not return pages")?;
            ensure!(pages.len() == 1, "Creation must return exactly one page");
            let pid = canonical_id(
                pages[0]["id"]
                    .as_str()
                    .context("Created page has no UUID")?,
            )?;
            state["page_id"] = json!(pid);
            state["phase"] = json!("created");
            state["created_at"] = json!(now());
            write_json(&record, &state)?;
            state
        }
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
