use crate::common::{append_event, command, digest, now};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Description = (&'static str, &'static str, &'static str);

fn worker_description(kind: &str) -> Option<Description> {
    Some(match kind {
        "worker_permission" => (
            "权限或审批",
            "执行会话已提交阻塞收据，声明分类为权限或审批。",
            "先在原执行聊天核对并处理授权或审批，再继续本轮。",
        ),
        "worker_input" => (
            "等待用户输入",
            "执行会话已提交阻塞收据，声明分类为等待用户输入。",
            "在原执行聊天补齐缺少的输入或决定，再继续本轮。",
        ),
        "worker_connection" => (
            "工具或连接",
            "执行会话已提交阻塞收据，声明分类为工具或连接。",
            "检查执行聊天指出的工具或连接，恢复后先核对未完成操作。",
        ),
        "worker_dependency" => (
            "外部依赖",
            "执行会话已提交阻塞收据，声明分类为外部依赖。",
            "先恢复执行聊天指出的外部服务、资料或其他前置条件，再继续本轮。",
        ),
        "worker_obstacle" => (
            "任务本身障碍",
            "执行会话已提交阻塞收据，声明分类为任务本身障碍。",
            "查看本轮文章或聊天中的实质障碍，调整要求或作出决定后再继续。",
        ),
        "worker_unknown" => (
            "执行会话报告阻塞，原因未知",
            "执行会话已提交阻塞收据，原因分类未知。",
            "在原执行聊天补充阻塞事实及下一步。",
        ),
        _ => return None,
    })
}

fn retry_description(kind: &str) -> Option<Description> {
    Some(match kind {
        "retry_unknown" => (
            "续试需要核对",
            "用户请求续试，但状态、归属或上次操作结果尚未确认。",
            "保留原聊天和轮次，核对派发记录、诊断和聊天，不要反复点击。",
        ),
        "retry_ineligible" => (
            "续试不适用",
            "当前状态不属于已确认中断且可继续的执行回合。",
            "先处理原有阻塞；返工使用待办事项，审批和输入在原聊天处理。",
        ),
        "automatic_retry_waiting" => (
            "等待自动续试",
            "已观察到明确连接错误或有持久证据的本机超时，等待同轮续试间隔。",
            "按持久计数继续原会话；审批、输入、停止和结果未知会阻止续试。",
        ),
        "automatic_retry_exhausted" => (
            "自动续试已耗尽",
            "本轮自动恢复次数已达到上限，执行仍未恢复。",
            "打开原会话核查错误和已有操作，处理原因后决定如何人工续接。",
        ),
        _ => return None,
    })
}

fn setup_description(kind: &str) -> Option<Description> {
    Some(match kind {
        "association" => (
            "执行回合归属不明",
            "无法安全关联当前聊天回合与派发记录。",
            "对照派发记录和原聊天确认归属，避免重复派发。",
        ),
        "read" => (
            "工具或连接",
            "读取任务或执行聊天失败，当前执行状态未知。",
            "检查 Notion 或桌面工具连接，再查看下一次只读核对结果。",
        ),
        "filesystem_permission" => (
            "文件权限或只读",
            "本机文件操作返回权限不足或文件系统只读。",
            "检查可写范围与挂载状态，核对未完成操作后续接。",
        ),
        "configuration" => (
            "调度配置",
            "任务选择的模型没有配置对应的执行模型。",
            "检查任务模型和模型映射，确认后再恢复处理。",
        ),
        "configuration_rechecked" => (
            "配置已重新读取",
            "上次模型检查失败，本次模型已通过配置检查。",
            "核对保留的检查点后决定如何恢复任务。",
        ),
        _ => return None,
    })
}

fn status_description(kind: &str) -> Option<Description> {
    Some(match kind {
        "write_unknown" => (
            "状态写入结果不明",
            "状态或聊天路由写入结果尚未确认。",
            "先读当前属性并核对检查点，避免盲目写入或重复派发。",
        ),
        "start_pending" => (
            "等待启动核对",
            "本轮已保留派发记录，尚未确认执行回合实际启动。",
            "核对原聊天；排队或发送回执不代表已开始，不重复派发。",
        ),
        "status_write_rejected" => (
            "状态更新未成功",
            "状态写入已明确失败，已保存执行证据和原轮次。",
            "下一次调度重新核对后仅补写状态，不重新派发。",
        ),
        "metadata_reconciled" => (
            "状态属性已核对",
            "当前状态或聊天路由已通过属性读回核对。",
            "继续按保留的执行检查点处理。",
        ),
        _ => return None,
    })
}

fn completion_description(kind: &str) -> Description {
    match kind {
        "receipt" => (
            "执行或交付",
            "完成收据缺失、不符合要求，或执行会话声明尚未完成。",
            "打开原聊天和本轮收据核对原因与已有交付。",
        ),
        "timeout" => (
            "执行状态未知",
            "执行回合长时间尚未完成。",
            "打开原聊天核对当前状态和已发生的操作。",
        ),
        "resumed" => (
            "执行已继续",
            "最近一次观察显示执行回合进行中，最终结果尚未确认。",
            "等待本轮完成，如长期无进展则检查执行聊天。",
        ),
        "resolved" => (
            "完成收据已校验",
            "本轮收据与派发标记已通过校验；执行会话声明正文已核验。",
            "查看任务记录及审核状态。",
        ),
        "failed" | "interrupted" => (
            "执行回合异常",
            "关联执行回合报告异常状态，不代表整个会话已经退出。",
            "查看原聊天，确认未完成操作结果后继续同轮。",
        ),
        _ => (
            "调度或执行状态未知",
            "调度出现异常，原因未确认。",
            "查看本机日志和关联聊天，核对检查点后恢复。",
        ),
    }
}

pub fn describe(kind: &str, flags: &[String]) -> (String, String, String) {
    let observed = |flag: &str| flags.iter().any(|value| value == flag);
    let values = if observed("waitingOnApproval") || kind == "retry_approval" {
        (
            "权限或审批",
            "执行会话明确报告正在等待审批。",
            "打开任务关联的执行聊天，查看并处理审批请求。",
        )
    } else if observed("waitingOnUserInput") || kind == "retry_input" {
        (
            "等待用户输入",
            "执行会话明确报告正在等待用户输入。",
            "打开任务关联的执行聊天，查看问题并回复。",
        )
    } else {
        worker_description(kind)
            .or_else(|| retry_description(kind))
            .or_else(|| setup_description(kind))
            .or_else(|| status_description(kind))
            .unwrap_or_else(|| completion_description(kind))
    };
    (values.0.into(), values.1.into(), values.2.into())
}

pub fn run_notification(arguments: &Value, title: &str, text: &str) -> Result<bool> {
    let arguments = command(arguments)?;
    let mut child = Command::new(&arguments[0])
        .args(&arguments[1..])
        .arg(title)
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn diagnostic_state(kind: &str, flags: &[String], stage: &str) -> Value {
    let mut observed: Vec<_> = flags
        .iter()
        .filter(|flag| ["waitingOnApproval", "waitingOnUserInput"].contains(&flag.as_str()))
        .cloned()
        .collect();
    observed.sort();
    json!({"kind": kind, "flags": observed, "stage": stage})
}

fn diagnostic_text(
    record: &Value,
    number: u64,
    stage: &str,
    details: &(String, String, String),
    timestamp: &str,
) -> String {
    let (category, fact, action) = details;
    let heading = if number > 0 {
        format!("第 {number} 轮")
    } else {
        "派发前".into()
    };
    let evidence = record["execution_observation_path"]
        .as_str()
        .map(|path| format!("本轮观察记录：`{path}`。"))
        .unwrap_or_else(|| "证据：调度检查点及本机服务日志。".into());
    format!(
        "# {heading}诊断\n\n- 记录时间：{timestamp}\n- 阶段：{stage}\n- 分类：{category}\n- 已确认：{fact}\n- 下一步：{action}\n- {evidence}\n\
         - 安全处理：保留检查点与原轮次；结果不明的发送或写入先核对。\n\
         - 限制：本机文件不代表通知已送达或已读；原始观察记录可能包含敏感信息。\n"
    )
}

fn notify_diagnostic(kind: &str, notify: Option<&Value>, target: &Path, category: &str) {
    if ![
        "resolved",
        "resumed",
        "automatic_retry_waiting",
        "automatic_retry_exhausted",
    ]
    .contains(&kind)
        && let Some(arguments) = notify
    {
        let _ = run_notification(
            arguments,
            "Notion 任务需要关注",
            &format!("{category}。详情：{}", target.display()),
        );
    }
}

pub fn write_diagnostic(
    record: &Value,
    kind: &str,
    flags: &[String],
    stage: Option<&str>,
    notify: Option<&Value>,
) -> Result<PathBuf> {
    let root = PathBuf::from(record["cwd"].as_str().context("Diagnostic needs cwd")?);
    let number = record["round"].as_u64().unwrap_or(0);
    let name = if number > 0 {
        format!("round-{number:03}")
    } else {
        "before-dispatch".into()
    };
    let target = root.join("outputs").join(format!("{name}-diagnostic.md"));
    fs::create_dir_all(target.parent().context("Invalid diagnostic path")?)?;
    let stage = stage.or(record["phase"].as_str()).unwrap_or("未知");
    let state = diagnostic_state(kind, flags, stage);
    let signature = digest(&serde_json::to_vec(&state)?);
    let signature_path = target.with_extension("signature");
    if target.exists() && fs::read_to_string(&signature_path).unwrap_or_default() == signature {
        return Ok(target);
    }
    let details = describe(kind, flags);
    let (category, fact, action) = &details;
    let timestamp = now();
    let text = diagnostic_text(record, number, stage, &details, &timestamp);
    append_event(
        &target.with_extension("jsonl"),
        &json!({
            "at": timestamp, "kind": kind, "flags": state["flags"], "stage": stage,
            "category": category, "fact": fact, "next": action
        }),
    )?;
    let temporary = target.with_extension("tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, &target)?;
    fs::write(signature_path, signature)?;
    notify_diagnostic(kind, notify, &target, category);
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deduplicates_diagnostics_without_echoing_errors() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let record = json!({"cwd": directory.path(), "round": 1, "phase": "waiting_execution", "error": "private raw response"});
        let path = write_diagnostic(&record, "read", &[], None, None)?;
        write_diagnostic(&record, "read", &[], None, None)?;
        assert_eq!(
            fs::read_to_string(path.with_extension("jsonl"))?
                .lines()
                .count(),
            1
        );
        assert!(!fs::read_to_string(&path)?.contains("private raw response"));
        Ok(())
    }
}
