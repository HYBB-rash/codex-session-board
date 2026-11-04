use super::{StateScheduler, flags, phase, round, text};
use crate::clients::{Executor, Notion};
use crate::common::{digest, now, page_id, thread_id};
use crate::diagnostics::describe;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;

pub const FEEDBACK_PROPERTY: &str = "调度诊断";

pub fn receipt_blocker_kind(pid: &str, record: &Value, execution: &Value) -> &'static str {
    let classification = (|| -> Result<&'static str> {
        let raw = fs::read(text(&record["result_path"]))?;
        let receipt: Value = serde_json::from_slice(&raw)?;
        let marker = format!(
            "NOTION_RESULT notion-program/{pid}/round-{:03} {}",
            round(record),
            digest(&raw)
        );
        ensure!(
            receipt["page_id"] == pid
                && receipt["round"] == record["round"]
                && receipt["execution_token"] == record["execution_token"]
                && receipt["status"] == "blocked"
                && text(&execution["text"]).lines().any(|line| line == marker),
            "Blocked receipt identity mismatch"
        );
        Ok(match text(&receipt["blocker"]["category"]) {
            "permission" => "worker_permission",
            "user_input" => "worker_input",
            "tool_connection" => "worker_connection",
            "external_dependency" => "worker_dependency",
            "task_obstacle" => "worker_obstacle",
            _ => "worker_unknown",
        })
    })();
    classification.unwrap_or("receipt")
}

fn stage_name(stage: &str) -> &str {
    match stage {
        "pending" => "派发前",
        "prepared" => "准备执行",
        "creating_chat" => "创建执行聊天",
        "chat_created" => "执行聊天已创建",
        "linking_chat" => "绑定聊天路由",
        "claiming" => "领取任务",
        "claimed" => "任务已领取",
        "executing" => "发送执行指令",
        "start_prepared" => "已在本地认领，尚未发送",
        "start_sending" => "派发结果待核对",
        "start_confirming" => "等待实际启动确认",
        "start_status" => "已确认启动，等待更新进行中",
        "start_status_confirming" => "已确认启动，核对进行中写入结果",
        "waiting_execution" => "等待执行结果",
        "execution_blocked" => "执行受阻",
        "submitting_blocker" => "提交阻塞状态",
        "submitting_review" => "提交待审核状态",
        "restoring_execution" => "核对恢复进行中状态",
        "awaiting_review" => "等待人工审核",
        "needs_attention" => "调度待核对",
        "retry_prepared" => "核对续试请求",
        "retry_sending" => "发送同轮续试",
        "retry_confirming" => "核对续试执行回合",
        "retry_status" => "确认续试状态",
        _ => "调度阶段待核对",
    }
}

pub fn render_feedback(
    pid: &str,
    record: &Value,
    kind: &str,
    observed_flags: &[String],
    stage: Option<&str>,
) -> String {
    let (category, fact, mut action) = describe(kind, observed_flags);
    let number = round(record);
    let heading = if number > 0 {
        format!("第 {number} 轮状态说明")
    } else {
        "派发前状态说明".into()
    };
    let stage = stage_name(stage.unwrap_or_else(|| phase(record)));
    let basis = if kind.starts_with("worker_") {
        "依据：本任务、本轮、执行令牌和 SHA256 均匹配的阻塞收据；分类由执行会话声明，调度器未独立核验任务实质原因。"
    } else {
        "依据：调度检查点及最近一次执行状态观察；不读取任务正文。"
    };
    if ["failed", "interrupted"].contains(&kind) {
        action.push_str(" 确认只是普通中断后，可使用任务的「中断续试」入口；仍须通过续试检查。");
    }
    let mut retry =
        "已采取：保留原会话、轮次及检查点；本说明不会发送重试。续试不等于批准权限或解除任务障碍。"
            .to_owned();
    if kind.starts_with("automatic_retry_")
        && let Some(attempts) = record["automatic_recovery"]["attempts"].as_array()
    {
        let count = attempts
            .iter()
            .filter(|attempt| attempt["counts_toward_limit"] != false)
            .count();
        retry = format!(
            "自动恢复记录：同轮已保留 {count}/{} 次尝试。运行或消息发出不会清零；后续同轮中断共用余额。\n\
             任务：https://app.notion.com/p/{}\n原会话：codex://threads/{}\n",
            record["automatic_recovery"]["maximum"],
            pid.replace('-', ""),
            text(&record["thread_id"])
        );
        for attempt in attempts.iter().skip(attempts.len().saturating_sub(3)) {
            let result = match text(&attempt["result"]) {
                "prepared" => "prepared",
                "sending" => "sending",
                "send_unknown" => "send_unknown",
                "not_sent_state_changed" => "not_sent_state_changed",
                "in_progress" => "in_progress",
                "completed" => "completed",
                "failed" => "failed",
                "interrupted" => "interrupted",
                "needs_attention" => "needs_attention",
                _ => "unknown",
            };
            let code = match text(&attempt["error_code"]) {
                "httpConnectionFailed" => "httpConnectionFailed",
                "responseStreamConnectionFailed" => "responseStreamConnectionFailed",
                "responseStreamDisconnected" => "responseStreamDisconnected",
                "ownedAppServerTimeout" => "ownedAppServerTimeout",
                _ => "unknown",
            };
            let prepared = text(&attempt["prepared_at"]);
            let prepared = if prepared.len() <= 40
                && prepared
                    .chars()
                    .all(|character| character.is_ascii_digit() || "-T:+.Z".contains(character))
            {
                prepared
            } else {
                "时间待核对"
            };
            retry.push_str(&format!(
                "第 {} 次：{result}；{code}；{prepared}\n",
                attempt["number"].as_u64().unwrap_or(0)
            ));
        }
        retry.push_str(&format!(
            "完整历史共 {} 条，保留在本轮自动恢复记录中。",
            attempts.len()
        ));
    }
    format!(
        "{heading}\n分类：{category}\n环节：{stage}\n已知事实：{fact}\n{basis}\n下一步：{action}\n{retry}\n\
         证据入口：本任务的「AI 会话 / 线程」；本机 outputs 中的同轮诊断文件、历史和观察记录（仅为证据位置，不代表已读）。\n\
         限制：未知原因须由执行会话补充到本轮文章；本属性可读回不代表 Herman 已看到。"
    )
}

impl<N: Notion, E: Executor> StateScheduler<N, E> {
    fn feedback_enabled(&self) -> bool {
        self.config["diagnostic_feedback_property"] == FEEDBACK_PROPERTY
    }

    pub(super) fn queue_feedback(
        &mut self,
        pid: &str,
        record: &mut Value,
        kind: &str,
        observed_flags: &[String],
        stage: Option<&str>,
    ) -> Result<()> {
        if !self.feedback_enabled() {
            return Ok(());
        }
        let rendered = render_feedback(pid, record, kind, observed_flags, stage);
        let key = digest(rendered.as_bytes());
        if record["diagnostic_feedback_desired"]["key"] == key {
            return Ok(());
        }
        let text = format!("{rendered}\n记录时间：{}", now());
        ensure!(
            text.chars().count() <= 1900,
            "Diagnostic feedback must be bounded text"
        );
        record["diagnostic_feedback_desired"] = json!({"key":key,"text":text,"kind":kind});
        self.save(pid, record)
    }

    fn feedback_task(&mut self, pid: &str, record: &Value) -> Result<Value> {
        let task = self.notion.get_feedback_metadata(pid)?;
        let props = &task["properties"];
        ensure!(
            page_id(text(&task["id"]))? == pid
                && task["source_id"] == self.config["data_source_id"]
                && props["执行人 / 代理"] == self.config["executor"]
                && [
                    "未开始",
                    "待办事项",
                    "进行中",
                    "阻塞中",
                    "待审核",
                    "请求续试",
                    "续试需审批",
                    "续试需输入",
                    "续试需核对",
                    "续试不适用",
                ]
                .contains(&text(&props["状态"])),
            "Diagnostic target is no longer in scope"
        );
        if !text(&record["thread_id"]).is_empty() {
            ensure!(
                thread_id(text(&props["AI 会话 / 线程"])).as_deref()
                    == record["thread_id"].as_str(),
                "Diagnostic route changed"
            );
        }
        if !text(&record["model"]).is_empty() {
            ensure!(props["模型"] == record["model"], "Diagnostic model changed");
        }
        ensure!(
            props.get(FEEDBACK_PROPERTY).is_some()
                && (props[FEEDBACK_PROPERTY].is_null() || props[FEEDBACK_PROPERTY].is_string()),
            "Diagnostic property is missing or unreadable"
        );
        Ok(task)
    }

    pub(super) fn flush_feedback(&mut self, pid: &str, record: &mut Value) {
        if !self.feedback_enabled() || !record["diagnostic_feedback_desired"].is_object() {
            return;
        }
        if self.try_flush_feedback(pid, record).is_err() {
            let _ = self.event("diagnostic_feedback_unavailable", json!({"page_id":pid}));
        }
    }

    fn try_flush_feedback(&mut self, pid: &str, record: &mut Value) -> Result<()> {
        let desired = record["diagnostic_feedback_desired"].clone();
        let mut publication = record
            .get("diagnostic_feedback_publication")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let task = self.feedback_task(pid, record)?;
        let current = text(&task["properties"][FEEDBACK_PROPERTY]);
        if ["prepared", "sent", "outcome_unknown"].contains(&text(&publication["state"])) {
            if current != text(&publication["text"]) {
                self.event(
                    "diagnostic_feedback_needs_reconciliation",
                    json!({"page_id":pid}),
                )?;
                return Ok(());
            }
            publication["state"] = json!("verified");
            publication["verified_at"] = json!(now());
            record["diagnostic_feedback_publication"] = publication.clone();
            self.save(pid, record)?;
        }
        if current == text(&desired["text"]) {
            if publication["state"] != "verified" || publication["key"] != desired["key"] {
                let mut publication = desired;
                publication["state"] = json!("verified");
                publication["verified_at"] = json!(now());
                record["diagnostic_feedback_publication"] = publication;
                self.save(pid, record)?;
            }
            return Ok(());
        }
        let expected = if publication["state"] == "verified" {
            text(&publication["text"])
        } else {
            text(&publication["before"])
        };
        if current != expected {
            self.event("diagnostic_feedback_conflict", json!({"page_id":pid}))?;
            return Ok(());
        }
        let latest = self.feedback_task(pid, record)?;
        for field in ["状态", "AI 会话 / 线程", "模型", FEEDBACK_PROPERTY] {
            if latest["properties"][field] != task["properties"][field] {
                self.event("diagnostic_feedback_target_changed", json!({"page_id":pid}))?;
                return Ok(());
            }
        }
        publication = desired.clone();
        publication["before"] = json!(current);
        publication["state"] = json!("prepared");
        publication["attempted_at"] = json!(now());
        record["diagnostic_feedback_publication"] = publication.clone();
        self.save(pid, record)?;
        let desired_text = desired["text"].as_str().context("Missing feedback text")?;
        ensure!(
            !desired_text.is_empty() && desired_text.chars().count() <= 1900,
            "Diagnostic feedback must be bounded text"
        );
        match self
            .notion
            .update_properties(pid, &json!({(FEEDBACK_PROPERTY):desired_text}))
        {
            Ok(_) => {}
            Err(error) => {
                publication["state"] = json!(if super::external(&error)
                    .is_some_and(|error| !error.outcome_unknown)
                {
                    "failed"
                } else {
                    "outcome_unknown"
                });
                record["diagnostic_feedback_publication"] = publication.clone();
                self.save(pid, record)?;
                self.event(
                    "diagnostic_feedback_write_failed",
                    json!({"page_id":pid,"outcome":publication["state"]}),
                )?;
                return Ok(());
            }
        }
        publication["state"] = json!("sent");
        record["diagnostic_feedback_publication"] = publication.clone();
        self.save(pid, record)?;
        let confirmed = self.feedback_task(pid, record)?;
        if confirmed["properties"][FEEDBACK_PROPERTY] == desired["text"] {
            publication["state"] = json!("verified");
            publication["verified_at"] = json!(now());
            record["diagnostic_feedback_publication"] = publication;
            self.save(pid, record)?;
            self.event(
                "diagnostic_feedback_verified",
                json!({"page_id":pid,"round":round(record)}),
            )?;
        } else {
            self.event(
                "diagnostic_feedback_needs_reconciliation",
                json!({"page_id":pid}),
            )?;
        }
        Ok(())
    }

    pub(super) fn finish_retry_feedback(
        &mut self,
        pid: &str,
        record: &mut Value,
        outcome: &Value,
        expected: &Value,
    ) {
        if outcome["status"] != expected["outcome"] {
            return;
        }
        let kind = match text(&outcome["status"]) {
            "retry_started" | "already_running" => "resumed",
            "retry_send_unknown"
            | "retry_reconciliation_required"
            | "retry_waiting_confirmation" => "retry_unknown",
            "automatic_retry_reconciled" => "automatic_retry_waiting",
            kind => kind,
        };
        let observed_flags = flags(&json!({"active_flags":record["blocker_flags"]}));
        self.diagnostic(pid, record, kind, &observed_flags, None);
    }
}
