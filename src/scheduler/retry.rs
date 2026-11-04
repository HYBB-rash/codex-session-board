use super::{
    CONTRACT, Executor, Notion, RETRY_REQUEST, StateScheduler, association_error, flags, phase,
    presend_error, round, text,
};
use crate::common::{command, digest, now, read_json, write_json};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub fn retry_signal(current: &Value, turn: &Value, receipt: Option<&Value>) -> Value {
    let thread = &current["thread"]["status"];
    let error = &turn["error"];
    let mut code = error["codexErrorInfo"].clone();
    let mut http_status = Value::Null;
    if let Some(object) = code.as_object().filter(|object| object.len() == 1) {
        let (name, details) = object.iter().next().expect("Nonempty error code");
        http_status = details["httpStatusCode"].clone();
        code = json!(name);
    }
    let mut result = json!({"eligible":false,"reason":"unclassified_terminal","error_code":code});
    let fail = |mut result: Value, reason: &str| {
        result["reason"] = json!(reason);
        result
    };
    if !["failed", "interrupted"].contains(&text(&turn["status"]))
        || !["idle", "notLoaded"].contains(&text(&thread["type"]))
        || thread["activeFlags"]
            .as_array()
            .is_some_and(|values| !values.is_empty())
        || current["turns"]
            .as_array()
            .is_some_and(|turns| turns.iter().any(|turn| turn["status"] == "inProgress"))
    {
        return fail(result, "not_idle_or_waiting");
    }
    if turn["status"] == "interrupted" {
        let proof = receipt
            .map(|receipt| &receipt["local_timeout"])
            .unwrap_or(&Value::Null);
        let timing = (
            turn["startedAt"].as_f64(),
            turn["completedAt"].as_f64(),
            proof["observed_at"].as_f64(),
            proof["seconds"].as_f64(),
        );
        let (Some(started), Some(completed), Some(observed), Some(seconds)) = timing else {
            return result;
        };
        if !error.is_null() && error != &json!({})
            || proof["source"] != "owned_app_server_wait"
            || proof["turn_id"] != turn["id"]
            || seconds <= 0.0
            || completed - started < seconds - 2.0
            || completed < observed - 2.0
            || completed > observed + 10.0
        {
            return result;
        }
        result["error_code"] = json!("ownedAppServerTimeout");
        result["reason"] = json!("owned_app_server_timeout");
    } else {
        if ![
            "httpConnectionFailed",
            "responseStreamConnectionFailed",
            "responseStreamDisconnected",
        ]
        .contains(&text(&code))
        {
            return result;
        }
        if !http_status.is_null()
            && ![408, 500, 502, 503, 504]
                .iter()
                .any(|status| http_status == *status)
        {
            return fail(result, "nontransient_http_status");
        }
    }
    let Some(items) = turn["items"].as_array() else {
        return fail(result, "operation_history_missing");
    };
    for item in items {
        let kind = text(&item["type"]);
        if [
            "userMessage",
            "agentMessage",
            "reasoning",
            "error",
            "contextCompaction",
        ]
        .contains(&kind)
            || kind == "fileChange" && item["status"] == "completed"
            || kind == "commandExecution"
                && item["status"] == "completed"
                && (item["exitCode"].is_i64() || item["exitCode"].is_u64())
                && (item["processId"].is_null() || item["processId"] == "")
        {
            continue;
        }
        return fail(result, "operation_outcome_unknown");
    }
    result["eligible"] = json!(true);
    if turn["status"] != "interrupted" {
        result["reason"] = json!("structured_transport_failure");
    }
    result
}

impl<N: Notion, E: Executor> StateScheduler<N, E> {
    pub(super) fn retry_policy(&self) -> Result<Option<(usize, Vec<f64>)>> {
        let policy = &self.config["automatic_retry"];
        if policy["enabled"] != true {
            return Ok(None);
        }
        let maximum = policy
            .get("max_attempts")
            .unwrap_or(&json!(3))
            .as_u64()
            .context("Invalid automatic retry policy")?;
        ensure!((1..=3).contains(&maximum), "Invalid automatic retry policy");
        let default_delays = json!([60, 180, 300]);
        let delays = policy
            .get("delays_seconds")
            .unwrap_or(&default_delays)
            .as_array()
            .context("Invalid automatic retry policy")?;
        ensure!(
            delays.len() >= maximum as usize,
            "Invalid automatic retry policy"
        );
        let delays = delays
            .iter()
            .map(|delay| {
                let delay = delay.as_f64().context("Invalid automatic retry policy")?;
                ensure!(
                    delay.is_finite() && delay >= 1.0,
                    "Invalid automatic retry policy"
                );
                Ok(delay)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some((maximum as usize, delays)))
    }

    fn automatic_attempt_count(record: &Value) -> usize {
        record["automatic_recovery"]["attempts"]
            .as_array()
            .map(|attempts| {
                attempts
                    .iter()
                    .filter(|attempt| attempt["counts_toward_limit"] != false)
                    .count()
            })
            .unwrap_or(0)
    }

    pub(super) fn automatic_retry(
        &mut self,
        pid: &str,
        record: &mut Value,
        observed: &Value,
    ) -> Result<Option<Value>> {
        let Some((maximum, delays)) = self.retry_policy()? else {
            return Ok(None);
        };
        let turn = &observed["turn"];
        let signal = &observed["automatic_retry_signal"];
        let eligible = signal["eligible"] == true
            && ["failed", "interrupted"].contains(&text(&observed["status"]))
            && turn["status"] == observed["status"]
            && ["idle", "notLoaded"].contains(&text(&observed["thread_status"]))
            && flags(observed).is_empty()
            && !text(&turn["id"]).is_empty()
            && turn["id"] == observed["dispatched_turn_id"]
            && observed["continuation"] != true
            && record["workflow"] == CONTRACT
            && !Path::new(text(&record["result_path"])).exists();
        if !eligible {
            if record["automatic_recovery"].is_object()
                && !["completed", "exhausted"]
                    .contains(&text(&record["automatic_recovery"]["state"]))
            {
                record["automatic_recovery"]["state"] = json!("suspended");
                record["automatic_recovery"]["reason"] =
                    json!(signal["reason"].as_str().unwrap_or("unsafe_observation"));
                self.save(pid, record)?;
            }
            return Ok(None);
        }
        let task = self.metadata(pid, Some(record))?;
        let status = text(&task["properties"]["状态"]);
        let early = record["dispatch_protocol"] == super::START_PROTOCOL
            && !record["start_resolved_at"].is_null()
            && record["running_status_verified_at"].is_null();
        if !["进行中", "阻塞中", RETRY_REQUEST].contains(&status)
            && !(early && status == "待办事项")
        {
            return Ok(None);
        }
        if !record["automatic_recovery"].is_object() {
            record["automatic_recovery"] = json!({
                "id":Uuid::new_v4().simple().to_string(),"round":record["round"],"thread_id":record["thread_id"],
                "initial_turn_id":turn["id"],"created_at":now(),"attempts":[],"maximum":maximum,"state":"waiting"
            });
        }
        ensure!(
            record["automatic_recovery"]["round"] == record["round"]
                && record["automatic_recovery"]["thread_id"] == record["thread_id"],
            "Automatic retry recovery belongs to a different execution"
        );
        let used = Self::automatic_attempt_count(record);
        let observation_path = record["execution_observation_path"].clone();
        if let Some(attempt) = record["automatic_recovery"]["attempts"]
            .as_array_mut()
            .and_then(|attempts| attempts.last_mut())
            && attempt["confirmed_turn_id"] == turn["id"]
        {
            attempt["result"] = observed["status"].clone();
            attempt["terminal_at"] = json!(now());
            attempt["error_code"] = signal["error_code"].clone();
            attempt["observation_path"] = observation_path;
        }
        let stored_max = record["automatic_recovery"]["maximum"]
            .as_u64()
            .context("Invalid recovery budget")? as usize;
        if used >= stored_max.min(maximum) {
            record["automatic_recovery"]["state"] = json!("exhausted");
            if record["automatic_recovery"]["exhausted_at"].is_null() {
                record["automatic_recovery"]["exhausted_at"] = json!(now());
            }
            self.save(pid, record)?;
            let outcome = self.block(
                pid,
                record,
                &format!("同轮自动续试 {used} 次后仍未恢复。"),
                "automatic_retry_exhausted",
                &[],
            )?;
            self.recovery_notice(pid, record)?;
            return Ok(Some(outcome));
        }
        if record["automatic_recovery"]["parent_turn_id"] != turn["id"] {
            record["automatic_recovery"]["parent_turn_id"] = turn["id"].clone();
            record["automatic_recovery"]["state"] = json!("waiting");
            record["automatic_recovery"]["error_code"] = signal["error_code"].clone();
            record["automatic_recovery"]["next_attempt_at"] = json!(epoch() + delays[used]);
            self.save(pid, record)?;
        }
        let deadline = record["automatic_recovery"]["next_attempt_at"]
            .as_f64()
            .context("Missing automatic retry deadline")?;
        if epoch() < deadline {
            return self
                .block(
                    pid,
                    record,
                    &format!("已确认可恢复异常；等待第 {} 次同轮自动续试。", used + 1),
                    "automatic_retry_waiting",
                    &[],
                )
                .map(Some);
        }
        self.request_retry(pid, record, true, Some(observed))
            .map(Some)
    }

    fn reserve_automatic_attempt(record: &mut Value, intent: &Value) -> Result<()> {
        let recovery = &mut record["automatic_recovery"];
        let code = recovery["error_code"].clone();
        let attempts = recovery["attempts"]
            .as_array_mut()
            .context("Missing automatic recovery attempts")?;
        attempts.push(json!({
            "id":intent["id"],"number":attempts.len()+1,"parent_turn_id":intent["parent_turn_id"],
            "prepared_at":now(),"intent_path":intent["intent_path"],"execution_path":intent["execution_path"],
            "error_code":code,"result":"prepared","counts_toward_limit":true
        }));
        recovery["state"] = json!("prepared");
        Ok(())
    }

    fn observe_automatic_attempt(record: &mut Value, observed: Option<&Value>, result: &str) {
        if record["retry"]["source"] != "automatic" {
            return;
        }
        let recovery = &mut record["automatic_recovery"];
        let Some(attempt) = recovery["attempts"]
            .as_array_mut()
            .and_then(|attempts| attempts.last_mut())
        else {
            return;
        };
        if let Some(observed) = observed {
            attempt["confirmed_turn_id"] = observed["turn"]["id"].clone();
            attempt["result"] = observed["status"].clone();
        } else {
            attempt["result"] = json!(result);
            if result == "not_sent_state_changed" {
                attempt["counts_toward_limit"] = json!(false);
            }
        }
        attempt["observed_at"] = json!(now());
        let state = attempt["result"].clone();
        recovery["state"] = state;
    }

    pub(super) fn request_retry(
        &mut self,
        pid: &str,
        record: &mut Value,
        automatic: bool,
        observation: Option<&Value>,
    ) -> Result<Value> {
        if !["waiting_execution", "execution_blocked"].contains(&phase(record))
            || record["workflow"] != CONTRACT
            || text(&record["execution_path"]).is_empty()
            || text(&record["thread_id"]).is_empty()
        {
            return self.reject_retry(pid, record, "unknown", None);
        }
        self.metadata(pid, Some(record))?;
        let receipt = self.execution_receipt(record)?;
        let marker = format!("notion-program/{pid}/round-{:03}", round(record));
        if receipt["thread_id"] != record["thread_id"] || receipt["dispatch_marker"] != marker {
            return self.reject_retry(pid, record, "unknown", None);
        }
        let observed = if automatic {
            observation
                .context("Missing automatic retry observation")?
                .clone()
        } else {
            match self.executor.poll_execution(&receipt) {
                Ok(observed) => observed,
                Err(error) if association_error(&error) => {
                    return self.reject_retry(pid, record, "unknown", None);
                }
                Err(error) => return Err(error),
            }
        };
        let check = self.round_path(
            record,
            "data",
            &format!("retry-check-{}.json", Uuid::new_v4().simple()),
        )?;
        write_json(&check, &observed)?;
        record["execution_observation_path"] = json!(check);
        let observed_flags = flags(&observed);
        if observed_flags
            .iter()
            .any(|flag| flag == "waitingOnApproval")
        {
            return self.reject_retry(pid, record, "approval", None);
        }
        if observed_flags
            .iter()
            .any(|flag| flag == "waitingOnUserInput")
        {
            return self.reject_retry(pid, record, "input", None);
        }
        if !observed_flags.is_empty() {
            return self.reject_retry(pid, record, "unknown", None);
        }
        let turn = &observed["turn"];
        if observed["status"] == "in_progress"
            && turn["status"] == "inProgress"
            && observed["thread_status"] == "active"
            && !text(&turn["id"]).is_empty()
        {
            return self.set_retry_status(
                pid,
                record,
                "进行中",
                "waiting_execution",
                "already_running",
            );
        }
        if !automatic
            && (record["automatic_recovery"].is_object()
                || self.retry_policy()?.is_some()
                    && observed["automatic_retry_signal"]["eligible"] == true)
        {
            if let Some(outcome) = self.automatic_retry(pid, record, &observed)? {
                return Ok(outcome);
            }
            if let Some(maximum) = record["automatic_recovery"]["maximum"].as_u64()
                && Self::automatic_attempt_count(record) >= maximum as usize
            {
                return self.reject_retry(pid, record, "ineligible", None);
            }
        }
        if observed["status"] != "interrupted"
            && !(automatic
                && observed["automatic_retry_signal"]["eligible"] == true
                && observed["status"] == "failed")
        {
            return self.reject_retry(pid, record, "ineligible", None);
        }
        if !["idle", "notLoaded"].contains(&text(&observed["thread_status"]))
            || text(&turn["id"]).is_empty()
            || turn["id"] != observed["dispatched_turn_id"]
        {
            return self.reject_retry(pid, record, "unknown", None);
        }
        let result_path = Path::new(text(&record["result_path"]));
        if result_path.exists() && read_json(result_path, Value::Null)?["status"] == "blocked" {
            return self.reject_retry(pid, record, "ineligible", None);
        }
        if record["last_retry_parent_turn"] == turn["id"] {
            return self.reject_retry(pid, record, "unknown", None);
        }
        let latest = self.metadata(pid, Some(record))?;
        let allowed = if automatic {
            vec![RETRY_REQUEST, "进行中", "阻塞中"]
        } else {
            vec![RETRY_REQUEST]
        };
        if !allowed.contains(&text(&latest["properties"]["状态"])) {
            return Ok(json!({"page_id":pid,"status":"retry_cancelled"}));
        }
        let id = Uuid::new_v4().simple().to_string();
        let source = if automatic { "automatic" } else { "manual" };
        let authorization = if automatic {
            "按 Herman 已授权的自动恢复策略续接；这不是新的人类指令。"
        } else {
            "Herman 在任务页面主动请求续试。"
        };
        let prompt = format!(
            "续试请求：{id}。{authorization}\n这是当前轮次的续接，不是新任务或新轮次。\
             先核对已经完成和结果不明的操作，复用已有文章与成果；不重复执行或创建、不覆盖旧阶段。\
             本请求不批准任何权限，不回答审批或用户问题，也不解除实质性阻塞。\
             若仍有这些阻塞，保留证据并按原规则返回 blocked。\n\n{}",
            self.worker_prompt(pid, record)?
        );
        let mut draft = json!({
            "status":"in_progress","transport":receipt["transport"].as_str().unwrap_or("desktop_tools"),
            "thread_id":record["thread_id"],"dispatch_marker":marker,"previous_turn_id":turn["id"],
            "source_thread_id":self.config["desktop_caller_thread_id"],
            "dispatch_prompt_sha256":digest(prompt.trim().as_bytes()),"retry_id":id,"retry_source":source,
            "local_timeout":if automatic { observed["local_timeout"].clone() } else { Value::Null }
        });
        for key in ["model", "sandbox", "approvals_reviewer"] {
            if let Some(value) = receipt.get(key) {
                draft[key] = value.clone();
            }
        }
        let intent = json!({
            "id":id,"requested_at":now(),"parent_turn_id":turn["id"],
            "original_execution_path":record["execution_path"],"original_dispatched_turn_id":record["dispatched_turn_id"],
            "previous_retry_parent_turn":record["last_retry_parent_turn"],
            "execution_path":self.round_path(record,"data",&format!("retry-{id}-execution.json"))?,
            "intent_path":self.round_path(record,"data",&format!("retry-{id}-intent.json"))?,
            "prompt":prompt,"draft":draft,"source":source
        });
        write_json(Path::new(text(&intent["intent_path"])), &intent)?;
        if automatic {
            Self::reserve_automatic_attempt(record, &intent)?;
        }
        self.checkpoint(pid, record, "retry_prepared", json!({"retry":intent}))?;
        self.continue_retry(pid, record)
    }

    fn reject_retry(
        &mut self,
        pid: &str,
        record: &mut Value,
        reason: &str,
        return_phase: Option<&str>,
    ) -> Result<Value> {
        let (target, message) = match reason {
            "approval" => (
                "续试需审批",
                "请打开原执行聊天处理审批；续试按钮不能代替批准。",
            ),
            "input" => ("续试需输入", "请打开原执行聊天回答问题，再等待调度核对。"),
            "ineligible" => (
                "续试不适用",
                "仅恢复已确认中断的执行回合；其他阻塞请先处理原诊断。",
            ),
            _ => (
                "续试需核对",
                "执行状态或上次操作结果未确认；保留原轮次，不重复发送。",
            ),
        };
        let return_phase = return_phase.unwrap_or_else(|| phase(record)).to_owned();
        record["retry_feedback"] = json!({"reason":reason,"message":message,"at":now()});
        let kind = format!("retry_{reason}");
        self.diagnostic(pid, record, &kind, &[], None);
        self.set_retry_status(pid, record, target, &return_phase, &kind)
    }

    fn set_retry_status(
        &mut self,
        pid: &str,
        record: &mut Value,
        target: &str,
        return_phase: &str,
        outcome: &str,
    ) -> Result<Value> {
        if record["pending_status_write"].is_object()
            && record["pending_status_write"]["state"] != "rejected"
        {
            self.diagnostic(pid, record, "write_unknown", &[], None);
            return Ok(json!({"page_id":pid,"status":"waiting_status_confirmation"}));
        }
        self.checkpoint(
            pid,
            record,
            "retry_status",
            json!({"retry_status":{
                "target":target,"return_phase":return_phase,"outcome":outcome,
                "automatic":record["retry"]["source"] == "automatic"
            }}),
        )?;
        self.finish_retry_status(pid, record)
    }

    fn finish_retry_status(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let pending = record["retry_status"].clone();
        let task = self.metadata(pid, Some(record))?;
        let status = text(&task["properties"]["状态"]);
        let target = pending["target"]
            .as_str()
            .context("Missing retry status target")?;
        let allowed = [RETRY_REQUEST, "续试需核对"].contains(&status)
            || pending["automatic"] == true && ["进行中", "阻塞中"].contains(&status);
        if record["pending_status_write"].is_object()
            && record["pending_status_write"]["state"] != "rejected"
        {
            ensure!(
                record["pending_status_write"]["phase"] == "retry_status"
                    && record["pending_status_write"]["target"] == target,
                "An earlier status write requires reconciliation"
            );
            if status != target {
                self.diagnostic(pid, record, "write_unknown", &[], None);
                return Ok(json!({"page_id":pid,"status":"waiting_status_confirmation"}));
            }
        }
        if status != target && allowed {
            self.write_status_intent(pid, record, target)?;
            match self.update(pid, json!({"状态":target})) {
                Ok(receipt) => {
                    record["pending_status_write"]["state"] = json!("acknowledged");
                    record["pending_status_write"]["receipt"] = receipt;
                    self.save(pid, record)?;
                }
                Err(error) => {
                    self.status_error(pid, record, &error)?;
                    return Err(error);
                }
            }
            let confirmed = self.metadata(pid, Some(record))?;
            if confirmed["properties"]["状态"] != target {
                return Ok(json!({"page_id":pid,"status":"waiting_status_confirmation"}));
            }
        } else if status != target {
            self.checkpoint(pid, record, text(&pending["return_phase"]), json!({}))?;
            return Ok(json!({"page_id":pid,"status":"retry_state_changed"}));
        }
        self.confirm_status_intent(record);
        self.checkpoint(pid, record, text(&pending["return_phase"]), json!({}))?;
        let outcome =
            json!({"page_id":pid,"status":pending["outcome"],"round":record.get("round")});
        self.finish_retry_feedback(pid, record, &outcome, &pending);
        Ok(outcome)
    }

    pub(super) fn continue_retry(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        if phase(record) == "retry_status" {
            return self.finish_retry_status(pid, record);
        }
        let intent = record["retry"].clone();
        let path = PathBuf::from(
            intent["execution_path"]
                .as_str()
                .context("Missing retry execution path")?,
        );
        if phase(record) == "retry_prepared" {
            let task = self.metadata(pid, Some(record))?;
            let allowed = [RETRY_REQUEST].contains(&text(&task["properties"]["状态"]))
                || intent["source"] == "automatic"
                    && ["进行中", "阻塞中"].contains(&text(&task["properties"]["状态"]));
            if !allowed
                || intent["source"] == "automatic"
                    && !path.exists()
                    && Path::new(text(&record["result_path"])).exists()
            {
                Self::observe_automatic_attempt(record, None, "not_sent_state_changed");
                record["last_retry_parent_turn"] = intent["previous_retry_parent_turn"].clone();
                self.checkpoint(pid, record, "execution_blocked", json!({}))?;
                return if !allowed {
                    Ok(json!({"page_id":pid,"status":"retry_cancelled"}))
                } else {
                    self.reject_retry(pid, record, "ineligible", None)
                };
            }
            if path.exists() {
                self.checkpoint(pid, record, "retry_confirming", json!({}))?;
            } else {
                self.checkpoint(
                    pid,
                    record,
                    "retry_sending",
                    json!({"last_retry_parent_turn":intent["parent_turn_id"]}),
                )?;
                Self::observe_automatic_attempt(record, None, "sending");
                self.save(pid, record)?;
                let model = self.model(record)?;
                match self.executor.send_continuation(
                    &intent["draft"],
                    text(&intent["prompt"]),
                    &model,
                ) {
                    Ok(sent) => write_json(&path, &sent)?,
                    Err(error)
                        if super::error_kind(
                            &error,
                            &["ContinuationChanged", "continuation_changed"],
                        ) =>
                    {
                        Self::observe_automatic_attempt(record, None, "not_sent_state_changed");
                        record["last_retry_parent_turn"] =
                            intent["previous_retry_parent_turn"].clone();
                        self.checkpoint(pid, record, "execution_blocked", json!({}))?;
                        let observed = self.executor.poll_execution(&intent["draft"]).ok();
                        let observed_flags = observed.as_ref().map(flags).unwrap_or_default();
                        let reason = if observed_flags
                            .iter()
                            .any(|flag| flag == "waitingOnApproval")
                        {
                            "approval"
                        } else if observed_flags
                            .iter()
                            .any(|flag| flag == "waitingOnUserInput")
                        {
                            "input"
                        } else {
                            "unknown"
                        };
                        return self.reject_retry(pid, record, reason, None);
                    }
                    Err(error) if presend_error(&error) => {
                        record["last_retry_parent_turn"] =
                            intent["previous_retry_parent_turn"].clone();
                        self.checkpoint(pid, record, "retry_prepared", json!({}))?;
                        return Err(error);
                    }
                    Err(_) => {
                        Self::observe_automatic_attempt(record, None, "send_unknown");
                        self.diagnostic(pid, record, "retry_unknown", &[], None);
                        let mut outcome = self.set_retry_status(
                            pid,
                            record,
                            "续试需核对",
                            "retry_sending",
                            "retry_send_unknown",
                        )?;
                        outcome["reconnect_before_next_task"] = json!(true);
                        return Ok(outcome);
                    }
                }
                self.checkpoint(pid, record, "retry_confirming", json!({}))?;
            }
        }
        let receipt = if path.exists() {
            read_json(&path, Value::Null)?
        } else {
            intent["draft"].clone()
        };
        let observed = match self.executor.poll_execution(&receipt) {
            Ok(observed) => observed,
            Err(error) if association_error(&error) => {
                let current = phase(record).to_owned();
                self.diagnostic(pid, record, "retry_unknown", &[], None);
                return self.set_retry_status(
                    pid,
                    record,
                    "续试需核对",
                    &current,
                    "retry_reconciliation_required",
                );
            }
            Err(error) => return Err(error),
        };
        let observation_path = PathBuf::from(format!("{}.observation.json", path.display()));
        write_json(&observation_path, &observed)?;
        let turn = &observed["turn"];
        if text(&observed["dispatched_turn_id"]).is_empty()
            || turn["id"] == intent["parent_turn_id"]
        {
            let current = phase(record).to_owned();
            self.diagnostic(pid, record, "retry_unknown", &[], None);
            return self.set_retry_status(
                pid,
                record,
                "续试需核对",
                &current,
                "retry_waiting_confirmation",
            );
        }
        if !path.exists() {
            let mut reconciled = receipt;
            reconciled["reconciled_from_history"] = json!(true);
            write_json(&path, &reconciled)?;
        }
        record["execution_path"] = json!(path);
        record["dispatched_turn_id"] = observed["dispatched_turn_id"].clone();
        record["execution_observation_path"] = json!(observation_path);
        record["retry"]["confirmed_turn_id"] = turn["id"].clone();
        Self::observe_automatic_attempt(record, Some(&observed), "");
        self.event("retry_started",json!({"page_id":pid,"round":round(record),"retry_id":intent["id"],"thread_id":record["thread_id"],"turn_id":turn["id"]}))?;
        self.checkpoint(pid, record, "retry_confirming", json!({}))?;
        let task = self.metadata(pid, Some(record))?;
        let status = text(&task["properties"]["状态"]);
        if ![RETRY_REQUEST, "续试需核对"].contains(&status)
            && !(intent["source"] == "automatic" && ["进行中", "阻塞中"].contains(&status))
        {
            self.checkpoint(pid, record, "waiting_execution", json!({}))?;
            return Ok(json!({"page_id":pid,"status":"retry_state_changed"}));
        }
        if observed["status"] == "in_progress"
            && turn["status"] == "inProgress"
            && observed["thread_status"] == "active"
            && !text(&turn["id"]).is_empty()
        {
            return self.set_retry_status(
                pid,
                record,
                "进行中",
                "waiting_execution",
                "retry_started",
            );
        }
        if observed["status"] == "completed" {
            return self.accept_execution(pid, record, &observed);
        }
        let observed_flags = flags(&observed);
        if observed_flags
            .iter()
            .any(|flag| flag == "waitingOnApproval")
        {
            return self.reject_retry(pid, record, "approval", Some("waiting_execution"));
        }
        if observed_flags
            .iter()
            .any(|flag| flag == "waitingOnUserInput")
        {
            return self.reject_retry(pid, record, "input", Some("waiting_execution"));
        }
        if intent["source"] == "automatic" {
            self.checkpoint(pid, record, "waiting_execution", json!({}))?;
            if status == "续试需核对" && observed["automatic_retry_signal"]["eligible"] == true
            {
                let outcome = self.set_retry_status(
                    pid,
                    record,
                    "阻塞中",
                    "waiting_execution",
                    "automatic_retry_reconciled",
                )?;
                if outcome["status"] != "automatic_retry_reconciled" {
                    return Ok(outcome);
                }
            }
            if let Some(outcome) = self.automatic_retry(pid, record, &observed)? {
                return Ok(outcome);
            }
        }
        self.block(
            pid,
            record,
            &format!("续试后的执行状态：{}", text(&observed["status"])),
            text(&observed["status"]),
            &observed_flags,
        )
    }

    fn recovery_notice(&mut self, pid: &str, record: &mut Value) -> Result<()> {
        if record["automatic_recovery"].get("notice").is_some() {
            return Ok(());
        }
        record["automatic_recovery"]["notice"] = json!({"state":"prepared","at":now()});
        self.save(pid, record)?;
        let path = self.round_path(record, "outputs", "automatic-retry.json")?;
        write_json(&path, &record["automatic_recovery"])?;
        let notification = self.config.get("diagnostic_notify_command").cloned();
        let result = if let Some(notification) = notification.filter(|value| !value.is_null()) {
            let body = format!(
                "任务：https://app.notion.com/p/{}\n原会话：codex://threads/{}\n第 {} 轮，额外续试 {} 次仍未恢复。\n\
                 已知为明确连接错误或有持久证据的本机执行超时；根因与先前业务效果仍需核查。\
                 请打开原会话核查错误与检查点，处理后再手动续接。详细记录：{}",
                pid.replace('-', ""),
                text(&record["thread_id"]),
                round(record),
                Self::automatic_attempt_count(record),
                path.display()
            );
            notify(&notification, &body)
                .unwrap_or_else(|_| json!({"state":"unknown","channel":"desktop"}))
        } else {
            json!({"state":"unconfigured","channel":"desktop"})
        };
        super::merge(&mut record["automatic_recovery"]["notice"], result);
        record["automatic_recovery"]["notice"]["finished_at"] = json!(now());
        self.save(pid, record)?;
        write_json(&path, &record["automatic_recovery"])
    }
}

fn notify(value: &Value, body: &str) -> Result<Value> {
    let arguments = command(value)?;
    let mut child = Command::new(&arguments[0])
        .args(&arguments[1..])
        .arg("Notion 自动续试已耗尽")
        .arg(body)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(
                json!({"state":if status.success() {"accepted"} else {"failed"},
                "returncode":status.code(),"channel":"desktop"}),
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(json!({"state":"unknown","channel":"desktop","error":"timeout"}));
        }
        thread::sleep(Duration::from_millis(20));
    }
}
