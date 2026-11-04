use super::{round, text};
use crate::common::canonical_id;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

pub const PROJECT_CONTEXT_CONTRACT: &str = "project_context_v1";

fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|value| !value.trim().is_empty())
}

fn helper(value: &Value) -> Result<Vec<String>> {
    let arguments = value
        .as_array()
        .context("Project context helper_command must be a nonempty list of strings")?;
    ensure!(
        !arguments.is_empty(),
        "Project context helper_command must be a nonempty list of strings"
    );
    arguments
        .iter()
        .map(|argument| {
            ensure!(
                nonempty(argument) && !text(argument).contains('\0'),
                "Project context helper_command must be a nonempty list of strings"
            );
            Ok(text(argument).to_owned())
        })
        .collect()
}

pub fn prepare_project_context(config: &Value) -> Result<Value> {
    let settings = &config["project_context"];
    if settings.is_null() {
        return Ok(json!({}));
    }
    ensure!(
        settings.is_object(),
        "Project context configuration requires a boolean enabled flag"
    );
    ensure!(
        settings.get("enabled").is_none() || settings["enabled"].is_boolean(),
        "Project context configuration requires a boolean enabled flag"
    );
    if settings["enabled"] != true {
        return Ok(json!({}));
    }
    Ok(json!({
        "project_context_contract": PROJECT_CONTEXT_CONTRACT,
        "project_context_helper_command": helper(&settings["helper_command"])?
    }))
}

fn shell_quote(argument: &str) -> String {
    if !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_@%+=:,./-".contains(character))
    {
        argument.to_owned()
    } else {
        format!("'{}'", argument.replace('\'', "'\"'\"'"))
    }
}

pub fn project_context_prompt(pid: &str, record: &Value) -> Result<String> {
    if record["project_context_contract"].is_null() {
        return Ok(String::new());
    }
    ensure!(
        record["project_context_contract"] == PROJECT_CONTEXT_CONTRACT,
        "Unknown project context contract"
    );
    let owner = canonical_id(pid)?;
    let output = Path::new(record["cwd"].as_str().context("Missing task workspace")?)
        .join("data")
        .join(format!("round-{:03}-project-context.json", round(record)));
    let mut arguments = helper(&record["project_context_helper_command"])?;
    arguments.extend([
        "context".into(),
        "--task".into(),
        owner.clone(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
    ]);
    let command = arguments
        .iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let example = json!({"contract":PROJECT_CONTEXT_CONTRACT,"status":"read","task_page_id":owner,
        "context_page_id":"实际项目上下文页UUID","version":"helper返回的实际版本","read_at":"实际读取时间",
        "topics":[{"page_id":"实际专题UUID","title":"实际专题标题","applied":"本轮如何实际应用该专题"}],
        "application":"最新摘要和所读专题如何影响本轮实施与验证"});
    Ok(format!(
        "项目上下文契约：{PROJECT_CONTEXT_CONTRACT}。每个新任务/新轮都要重新检查项目上下文。\
         实际开工前由执行者运行以下命令，不得复用上一轮结果冒充本轮读取；调度器不运行 helper，也不读取项目或专题正文：\n\
         ```bash\n{command}\n```\n\
         helper 按任务的实际所属项目解析上下文；确认上下文缺失时由 helper 自动基础创建。\
         已有上下文不可读时只做有限重试，仍失败则返回 blocked，不得重建或覆盖已有上下文。\
         命令失败、返回 blocked、输出缺失或不可读、归属无法确认时均停止开工并返回 blocked。\
         仅在 helper 明确确认无所属项目时标记 unassigned，不按标题、目录或聊天猜测项目，不自动改变项目归属。\n\
         执行者必须阅读 helper 返回的最新摘要和目录，选择相关专题，通过 Notion fetch 真正阅读专题正文；标题、链接或摘要不算阅读。\
         相关内容截断或不可读时有限重试，仍失败则 blocked。没有相关专题时允许 topics=[]，说明判断及摘要如何实际应用。\
         在报告中记录 context_page_id、version、updated_at、read_at、已读专题 page_id/title 以及如何实际应用到实施与验证；不编造版本或更新时间。\
         上下文不修改任务需求；变化时记录影响，不擅自改变本轮目标。\n\
         completed 执行收据顶层必须增加 project_context 对象（与 notion_delivery 同级）：{}。所有字段必须存在。\
         status=read 时 ID、version、application 非空，version 为非空字符串或正整数，topics 列出真正读取的专题及具体 applied。\
         无所属项目时提交 status=unassigned、context_page_id=null、version=null、topics=[]、application 字符串（可空），\
         并增加非空 reason 记录 helper 确认无所属项目的实际依据。不能将 helper 失败写成 unassigned，未实际读取不得声明 read。\
         同轮续接保留本契约及原目标，不能改配置绕过检查。调度器只校验声明，不证明正文已被阅读。\n",
        serde_json::to_string(&example)?
    ))
}

pub fn validate_project_context_receipt(pid: &str, record: &Value, result: &Value) -> Result<()> {
    if record["project_context_contract"].is_null() || result["status"] != "completed" {
        return Ok(());
    }
    ensure!(
        record["project_context_contract"] == PROJECT_CONTEXT_CONTRACT,
        "Unknown project context contract"
    );
    let context = result["project_context"]
        .as_object()
        .context("Missing project context receipt")?;
    for field in [
        "contract",
        "status",
        "task_page_id",
        "context_page_id",
        "version",
        "read_at",
        "topics",
        "application",
    ] {
        ensure!(
            context.contains_key(field),
            "Missing project context field: {field}"
        );
    }
    let context = &result["project_context"];
    ensure!(
        context["contract"] == record["project_context_contract"],
        "Project context receipt contract does not match this round"
    );
    ensure!(
        canonical_id(text(&context["task_page_id"]))? == canonical_id(pid)?,
        "Project context receipt belongs to another task"
    );
    ensure!(
        nonempty(&context["read_at"]),
        "Project context read_at must be nonempty text"
    );
    ensure!(
        context["application"].is_string(),
        "Project context application must be text"
    );
    let topics = context["topics"]
        .as_array()
        .context("Project context topics must be a list")?;
    if context["status"] == "unassigned" {
        ensure!(
            context["context_page_id"].is_null()
                && context["version"].is_null()
                && topics.is_empty(),
            "Unassigned project context requires null IDs/version and no topics"
        );
        ensure!(
            nonempty(&context["reason"]),
            "Unassigned project context requires a reason"
        );
        return Ok(());
    }
    ensure!(
        context["status"] == "read",
        "Unknown project context receipt status"
    );
    canonical_id(text(&context["context_page_id"]))?;
    ensure!(
        nonempty(&context["version"]) || context["version"].as_u64().is_some_and(|value| value > 0),
        "Project context version must be nonempty text or a positive integer"
    );
    ensure!(
        nonempty(&context["application"]),
        "Project context application must describe actual use"
    );
    let mut seen = HashSet::new();
    for topic in topics {
        ensure!(topic.is_object(), "Project context topic must be an object");
        let target = canonical_id(text(&topic["page_id"]))?;
        ensure!(seen.insert(target), "Duplicate project context topic");
        ensure!(
            nonempty(&topic["title"]) && nonempty(&topic["applied"]),
            "Project context topic requires title and actual applied use"
        );
    }
    Ok(())
}
