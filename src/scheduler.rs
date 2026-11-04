use crate::clients::{Executor, ExternalError, Notion};
use crate::common::{
    append_event, canonical_id, config_path, digest, lock, now, page_id, read_json, thread_id,
    write_json,
};
use crate::diagnostics::write_diagnostic;
use crate::history::{history_prompt, validate_history_delivery};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

mod context;
mod feedback;
mod retry;
#[cfg(test)]
mod tests;

pub use context::{
    prepare_project_context, project_context_prompt, validate_project_context_receipt,
};
pub use retry::retry_signal;

pub const CONTRACT: &str = "execution_chat_owns_page_v1";
pub const START_PROTOCOL: &str = "confirmed_start_v1";
pub const ARTICLE_CONTRACT: &str = "per_round_article_v2";
pub const RETRY_REQUEST: &str = "请求续试";
pub const RETRY_BLOCKED: &[&str] = &["续试需审批", "续试需输入", "续试需核对", "续试不适用"];
pub const TERMINAL: &[&str] = &["已完成待归档", "已归档", "已废弃"];
pub const START_PHASES: &[&str] = &[
    "start_prepared",
    "start_sending",
    "start_confirming",
    "start_status",
    "start_status_confirming",
];
pub const RETRY_PHASES: &[&str] = &[
    "retry_prepared",
    "retry_sending",
    "retry_confirming",
    "retry_status",
];
const WAITING: &[&str] = &["waiting_execution", "execution_blocked"];
const RETURNED_TODO_ERROR: &str =
    "ValueError: Active task returned to a different state; reconcile this round";
const MODEL_ERROR: &str = "ValueError: Selected model has no configured mapping";

pub struct StateScheduler<N: Notion, E: Executor> {
    pub config: Value,
    pub state: Value,
    pub notion: N,
    pub executor: E,
    trigger: String,
    data: PathBuf,
    state_path: PathBuf,
}

struct SchedulerLock(fs::File);

struct RoundFiles {
    number: u64,
    cwd: PathBuf,
    snapshot: PathBuf,
    result: PathBuf,
}

struct ExecutionSettings {
    model: String,
    sandbox: String,
    reviewer: String,
}

struct StatusSubmission {
    completed: bool,
    restoring: bool,
    target: String,
}

impl StatusSubmission {
    fn new(record: &Value) -> Self {
        let completed = phase(record) == "submitting_review";
        let restoring = phase(record) == "restoring_execution";
        let target = if restoring {
            "进行中"
        } else if completed {
            "待审核"
        } else {
            "阻塞中"
        }
        .to_owned();
        Self {
            completed,
            restoring,
            target,
        }
    }
}

impl Drop for SchedulerLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn phase(record: &Value) -> &str {
    record["phase"].as_str().unwrap_or("pending")
}

fn remove(record: &mut Value, field: &str) -> Option<Value> {
    record
        .as_object_mut()
        .and_then(|object| object.remove(field))
}

fn merge(record: &mut Value, values: Value) {
    if let (Some(record), Some(values)) = (record.as_object_mut(), values.as_object()) {
        record.extend(values.clone());
    }
}

fn round(record: &Value) -> u64 {
    record["round"].as_u64().unwrap_or(0)
}

fn flags(execution: &Value) -> Vec<String> {
    let Some(value) = execution
        .get("active_flags")
        .or_else(|| execution.get("waiting_flags"))
    else {
        return Vec::new();
    };
    match value.as_array() {
        Some(values) => values
            .iter()
            .map(|value| value.as_str().unwrap_or("invalidActiveFlag").to_owned())
            .collect(),
        None => vec!["invalidActiveFlags".into()],
    }
}

fn external(error: &anyhow::Error) -> Option<&ExternalError> {
    error.downcast_ref::<ExternalError>()
}

fn error_kind(error: &anyhow::Error, names: &[&str]) -> bool {
    external(error).is_some_and(|error| {
        names
            .iter()
            .any(|name| error.kind.eq_ignore_ascii_case(name))
    })
}

fn presend_error(error: &anyhow::Error) -> bool {
    external(error)
        .is_some_and(|error| !error.outcome_unknown && error.kind.starts_with("pre_send"))
        || error_kind(
            error,
            &[
                "DesktopPreSendReadError",
                "pre_send_read",
                "presend_read",
                "presend",
                "pre_send",
            ],
        )
}

fn association_error(error: &anyhow::Error) -> bool {
    external(error).is_some_and(|error| {
        [
            "execution_association_conflict",
            "execution_pagination_incomplete",
            "execution_history_bound",
            "execution_dispatch_not_found",
        ]
        .contains(&error.kind.as_str())
    }) || error_kind(error, &["ExecutionAssociationError", "association"])
}

fn read_error(error: &anyhow::Error) -> bool {
    presend_error(error)
        || error_kind(error, &["DesktopReadError", "read", "desktop_read"])
        || external(error).is_some_and(|error| {
            !error.outcome_unknown
                && [
                    "fetch",
                    "schema",
                    "query",
                    "access",
                    "read",
                    "execution_read",
                    "thread/read",
                    "thread/turns/list",
                    "read_thread",
                ]
                .contains(&error.operation.as_str())
                && !["CodexNeedsAttention", "needs_attention"].contains(&error.kind.as_str())
        })
}

fn signature(error: &anyhow::Error) -> String {
    let message = error.to_string();
    if external(error).is_some() || message.starts_with("ValueError:") {
        message
    } else {
        format!("ValueError: {message}")
    }
}

fn reconnect_notion(error: &anyhow::Error) -> bool {
    external(error).is_some_and(|error| {
        error.reconnectable
            && !error.outcome_unknown
            && ["fetch", "schema", "query", "access"].contains(&error.operation.as_str())
    })
}

fn start_error_kind(record: &Value, error: &anyhow::Error) -> &'static str {
    if external(error).is_some_and(|error| error.operation == "update" && !error.outcome_unknown) {
        "status_write_rejected"
    } else if phase(record).starts_with("start_status") {
        "write_unknown"
    } else if association_error(error) {
        "association"
    } else if read_error(error) {
        "read"
    } else {
        "start_pending"
    }
}

fn deferred_error_kind(record: &Value, error: &anyhow::Error) -> Option<&'static str> {
    let status_phase = [
        "submitting_review",
        "submitting_blocker",
        "linking_chat",
        "claiming",
        "retry_status",
        "restoring_execution",
    ]
    .contains(&phase(record));
    if status_phase && external(error).is_some_and(|error| error.outcome_unknown) {
        return Some("write_unknown");
    }
    let rejected = external(error)
        .is_some_and(|error| error.operation == "update" && !error.outcome_unknown)
        && record["dispatch_protocol"] == START_PROTOCOL
        && [
            "submitting_review",
            "submitting_blocker",
            "restoring_execution",
            "retry_status",
        ]
        .contains(&phase(record));
    if rejected {
        Some("status_write_rejected")
    } else {
        read_error(error).then_some("read")
    }
}

fn attention_error_kind(error: &anyhow::Error) -> &'static str {
    if signature(error) == MODEL_ERROR {
        "configuration"
    } else if error.chain().any(|error| {
        error.downcast_ref::<std::io::Error>().is_some_and(|error| {
            error.kind() == std::io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(30)
        })
    }) {
        "filesystem_permission"
    } else {
        "unknown"
    }
}

fn returned_to_todo(record: &Value, status: &str) -> bool {
    status == "待办事项"
        && (WAITING.contains(&phase(record))
            || phase(record) == "needs_attention"
                && WAITING.contains(&text(&record["interrupted_phase"]))
                && record["error"] == RETURNED_TODO_ERROR)
}

impl<N: Notion, E: Executor> StateScheduler<N, E> {
    pub fn new(config: Value, notion: N, executor: E, trigger: String) -> Result<Self> {
        ensure!(config.is_object(), "Configuration must be an object");
        let data = config_path(&config, "data_dir")?;
        fs::create_dir_all(&data)?;
        let state_path = data.join("state.json");
        let state = read_json(&state_path, json!({}))?;
        ensure!(state.is_object(), "Scheduler state must be an object");
        ensure!(
            state
                .as_object()
                .is_some_and(|state| state.values().all(Value::is_object)),
            "Scheduler records must be objects"
        );
        Ok(Self {
            config,
            state,
            notion,
            executor,
            trigger,
            data,
            state_path,
        })
    }

    fn save(&mut self, pid: &str, record: &Value) -> Result<()> {
        self.state[pid] = record.clone();
        write_json(&self.state_path, &self.state)
    }

    fn event(&self, action: &str, details: Value) -> Result<Value> {
        let mut event = json!({"at": now(), "trigger": self.trigger, "action": action});
        merge(&mut event, details);
        append_event(&self.data.join("events.jsonl"), &event)?;
        Ok(event)
    }

    fn checkpoint(
        &mut self,
        pid: &str,
        record: &mut Value,
        next_phase: &str,
        values: Value,
    ) -> Result<()> {
        merge(record, values);
        record["phase"] = json!(next_phase);
        record["updated_at"] = json!(now());
        self.save(pid, record)?;
        self.event(
            next_phase,
            json!({"page_id": pid, "round": record.get("round")}),
        )?;
        Ok(())
    }

    fn verify(&self, task: &Value, statuses: &[&str]) -> Result<()> {
        ensure!(
            task["source_id"] == self.config["data_source_id"],
            "Task moved outside the authorized tracker"
        );
        let properties = &task["properties"];
        ensure!(
            properties["执行人 / 代理"] == self.config["executor"],
            "Task executor changed"
        );
        ensure!(
            statuses.contains(&text(&properties["状态"])),
            "Task status changed"
        );
        ensure!(
            properties["模型"]
                .as_str()
                .is_some_and(|model| self.config["model_map"].get(model).is_some()),
            "Selected model has no configured mapping"
        );
        Ok(())
    }

    fn metadata(&mut self, pid: &str, record: Option<&Value>) -> Result<Value> {
        let task = self.notion.get_task_metadata(pid)?;
        ensure!(
            page_id(text(&task["id"]))? == pid,
            "Metadata belongs to a different page"
        );
        let mut statuses = vec![
            "未开始",
            "计划中",
            "待办事项",
            "进行中",
            "阻塞中",
            "待审核",
            RETRY_REQUEST,
        ];
        statuses.extend_from_slice(TERMINAL);
        statuses.extend_from_slice(RETRY_BLOCKED);
        self.verify(&task, &statuses)?;
        if let Some(record) = record
            && !text(&record["thread_id"]).is_empty()
        {
            ensure!(
                thread_id(text(&task["properties"]["AI 会话 / 线程"])).as_deref()
                    == record["thread_id"].as_str(),
                "Execution chat changed"
            );
            if !text(&record["model"]).is_empty() {
                ensure!(
                    task["properties"]["模型"] == record["model"],
                    "Selected model changed during execution"
                );
            }
        }
        Ok(task)
    }

    fn update(&mut self, pid: &str, properties: Value) -> Result<Value> {
        let fields = properties
            .as_object()
            .context("Properties must be an object")?;
        ensure!(
            !fields.is_empty()
                && fields
                    .keys()
                    .all(|field| ["状态", "AI 会话 / 线程"].contains(&field.as_str())),
            "Dispatcher may only write status and execution chat"
        );
        self.notion
            .update_properties(&canonical_id(pid)?, &properties)
    }

    fn diagnostic(
        &mut self,
        pid: &str,
        record: &mut Value,
        kind: &str,
        observed_flags: &[String],
        stage: Option<&str>,
    ) {
        let mut details = record.clone();
        if text(&details["cwd"]).is_empty()
            && let Ok(root) = config_path(&self.config, "task_workspace_root")
        {
            details["cwd"] = json!(root.join(pid));
        }
        let stage = stage.or_else(|| {
            ["execution_blocked", "submitting_blocker"]
                .contains(&phase(&details))
                .then_some("waiting_execution")
        });
        match write_diagnostic(
            &details,
            kind,
            observed_flags,
            stage,
            self.config.get("diagnostic_notify_command"),
        ) {
            Ok(path) => {
                let _ = self.event(
                    "diagnostic_written",
                    json!({"page_id": pid, "round": record.get("round"), "kind": kind, "path": path}),
                );
            }
            Err(_) => {
                let _ = self.event("diagnostic_write_failed", json!({"page_id": pid}));
            }
        }
        if self
            .queue_feedback(pid, record, kind, observed_flags, stage)
            .is_ok()
            && !["read", "write_unknown"].contains(&kind)
        {
            self.flush_feedback(pid, record);
        }
    }

    pub fn run_once(&mut self) -> Result<Vec<Value>> {
        let Some(file) = lock(&self.data.join("scheduler.lock"))? else {
            return Ok(vec![self.event("already_running", json!({}))?]);
        };
        let _guard = SchedulerLock(file);
        self.state = read_json(&self.state_path, json!({}))?;
        ensure!(self.state.is_object(), "Scheduler state must be an object");
        ensure!(
            self.state
                .as_object()
                .is_some_and(|state| state.values().all(Value::is_object)),
            "Scheduler records must be objects"
        );
        let candidates = self.candidate_tasks()?;
        self.process_candidates(candidates)
    }

    fn candidate_tasks(&mut self) -> Result<Vec<String>> {
        let rows = self.notion.query_tasks()?;
        self.event(
            "queue_read",
            json!({"count": rows.len(), "workflow": CONTRACT}),
        )?;
        let mut candidates = Vec::new();
        for row in rows {
            let pid = page_id(text(&row["url"]))?;
            if !candidates.contains(&pid) {
                candidates.push(pid);
            }
        }
        for (pid, record) in self.state.as_object().context("Invalid state")? {
            if !["terminal", "externally_stopped"].contains(&phase(record))
                && !candidates.contains(pid)
            {
                candidates.push(pid.clone());
            }
        }
        Ok(candidates)
    }

    fn page_allowed(&self, pid: &str) -> bool {
        let excluded = self.config["excluded_page_ids"]
            .as_array()
            .is_some_and(|ids| ids.contains(&json!(pid)));
        let disallowed = self
            .config
            .get("allowed_page_ids")
            .filter(|value| !value.is_null())
            .is_some_and(|ids| !ids.as_array().is_some_and(|ids| ids.contains(&json!(pid))));
        !excluded && !disallowed
    }

    fn process_candidates(&mut self, candidates: Vec<String>) -> Result<Vec<Value>> {
        let mut outcomes = Vec::new();
        for pid in candidates {
            if !self.page_allowed(&pid) {
                continue;
            }
            let outcome = match self.process(&pid) {
                Ok(outcome) => outcome,
                Err(error) => self.handle_error(&pid, &error)?,
            };
            let stop = outcome["reconnect_before_next_task"] == true
                || outcome["reconnect_notion"] == true;
            outcomes.push(outcome);
            if stop {
                break;
            }
        }
        Ok(outcomes)
    }

    fn handle_error(&mut self, pid: &str, error: &anyhow::Error) -> Result<Value> {
        let mut record = self
            .state
            .get(pid)
            .cloned()
            .unwrap_or_else(|| json!({"phase": "pending", "last_delivered_round": 0}));
        if START_PHASES.contains(&phase(&record)) {
            return self.defer_start_error(pid, &mut record, error);
        }
        if presend_error(error) && phase(&record) == "executing" {
            self.checkpoint(pid, &mut record, "claimed", json!({}))?;
        }
        if let Some(kind) = deferred_error_kind(&record, error) {
            return self.defer_operation_error(pid, &mut record, error, kind);
        }
        self.interrupt_task(pid, &mut record, error)
    }

    fn defer_start_error(
        &mut self,
        pid: &str,
        record: &mut Value,
        error: &anyhow::Error,
    ) -> Result<Value> {
        let kind = start_error_kind(record, error);
        self.event(
            "start_deferred",
            json!({"page_id": pid, "round": record["round"], "phase": phase(record),
                       "error": signature(error)}),
        )?;
        self.diagnostic(pid, record, kind, &[], None);
        let mut outcome = json!({"page_id": pid, "status": "retry_next_cycle"});
        if reconnect_notion(error) {
            outcome["reconnect_notion"] = json!(true);
        } else if read_error(error)
            || external(error).is_some_and(|error| {
                error.outcome_unknown
                    || [
                        "connection",
                        "timeout",
                        "needs_attention",
                        "CodexConnectionError",
                        "CodexTimeoutError",
                        "CodexNeedsAttention",
                    ]
                    .contains(&error.kind.as_str())
            })
        {
            outcome["reconnect_before_next_task"] = json!(true);
        }
        Ok(outcome)
    }

    fn defer_operation_error(
        &mut self,
        pid: &str,
        record: &mut Value,
        error: &anyhow::Error,
        kind: &str,
    ) -> Result<Value> {
        let error_signature = signature(error);
        self.diagnostic(pid, record, kind, &[], None);
        if record["last_read_error"] != error_signature {
            self.event(
                "operation_deferred",
                json!({"page_id": pid, "error": error_signature, "phase": phase(record)}),
            )?;
        }
        record["last_read_error"] = json!(error_signature);
        self.save(pid, record)?;
        let mut outcome = json!({"page_id": pid, "status": "retry_next_cycle"});
        if reconnect_notion(error) {
            outcome["reconnect_notion"] = json!(true);
        } else if kind == "write_unknown" {
            outcome["reconnect_before_next_task"] = json!(true);
        }
        Ok(outcome)
    }

    fn interrupt_task(
        &mut self,
        pid: &str,
        record: &mut Value,
        error: &anyhow::Error,
    ) -> Result<Value> {
        let error_signature = signature(error);
        let kind = attention_error_kind(error);
        let interrupted_stage = record["interrupted_phase"].as_str().map(str::to_owned);
        self.diagnostic(pid, record, kind, &[], interrupted_stage.as_deref());
        if record["error"] != error_signature {
            let interrupted = phase(record).to_owned();
            self.checkpoint(
                pid,
                record,
                "needs_attention",
                json!({"interrupted_phase": interrupted, "error": error_signature}),
            )?;
        }
        Ok(json!({"page_id": pid, "status": "needs_attention", "error": error_signature}))
    }

    fn process(&mut self, pid: &str) -> Result<Value> {
        let mut record = self.state.get(pid).cloned().unwrap_or_else(|| json!({}));
        let task = self.observe_task_metadata(pid, &mut record)?;
        if let Some(outcome) = self.initial_task_outcome(pid, &mut record, &task)? {
            return Ok(outcome);
        }
        self.flush_feedback(pid, &mut record);
        let route_text = text(&task["properties"]["AI 会话 / 线程"]);
        ensure!(
            route_text.is_empty() || thread_id(route_text).is_some(),
            "Execution chat field is not a complete Codex thread link"
        );
        if let Some(outcome) = self.resume_task_phase(pid, &mut record, &task)? {
            return Ok(outcome);
        }
        let task = self.reconcile_dispatch_metadata(pid, &mut record, task)?;
        if let Some(outcome) = self.process_existing_round(pid, &mut record, &task)? {
            return Ok(outcome);
        }
        self.dispatch_pending_task(pid, &mut record, &task)
    }

    fn observe_task_metadata(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let task = self.metadata(pid, None)?;
        let status = text(&task["properties"]["状态"]);
        self.event(
            "task_metadata_read",
            json!({"page_id": pid, "status": status, "round": round(record)}),
        )?;
        if remove(record, "last_read_error").is_some() {
            self.event(
                "task_metadata_read_recovered",
                json!({"page_id": pid, "round": round(record)}),
            )?;
            self.save(pid, record)?;
        }
        Ok(task)
    }

    fn initial_task_outcome(
        &mut self,
        pid: &str,
        record: &mut Value,
        task: &Value,
    ) -> Result<Option<Value>> {
        let status = text(&task["properties"]["状态"]);
        if TERMINAL.contains(&status) {
            self.checkpoint(pid, record, "terminal", json!({"remote_status": status}))?;
            return Ok(Some(json!({"page_id": pid, "status": status})));
        }
        if ["未开始", "计划中"].contains(&status)
            && [
                "pending",
                "awaiting_review",
                "externally_stopped",
                "terminal",
            ]
            .contains(&phase(record))
        {
            return self
                .event("existing_unmanaged_task", json!({"page_id": pid}))
                .map(Some);
        }
        Ok(None)
    }

    fn resume_task_phase(
        &mut self,
        pid: &str,
        record: &mut Value,
        task: &Value,
    ) -> Result<Option<Value>> {
        let status = text(&task["properties"]["状态"]);
        let current_phase = phase(record).to_owned();
        if START_PHASES.contains(&current_phase.as_str()) {
            self.metadata(pid, Some(record))?;
            if !["待办事项", "进行中"].contains(&status) {
                return Ok(Some(
                    json!({"page_id": pid, "status": "start_state_changed"}),
                ));
            }
            return self.continue_start(pid, record).map(Some);
        }
        if RETRY_PHASES.contains(&current_phase.as_str()) {
            return self.continue_retry(pid, record).map(Some);
        }
        if status == RETRY_REQUEST {
            return self.request_retry(pid, record, false, None).map(Some);
        }
        if returned_to_todo(record, status) {
            return self.prepare_rework(pid, record).map(Some);
        }
        if current_phase == "needs_attention" {
            return Ok(Some(self.task_needs_attention(pid, record)));
        }
        Ok(None)
    }

    fn prepare_rework(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        self.metadata(pid, Some(record))?;
        if !self.executor.is_thread_idle(text(&record["thread_id"]))? {
            return Ok(json!({"page_id": pid, "status": "waiting_previous_execution"}));
        }
        let task = self.metadata(pid, Some(record))?;
        if task["properties"]["状态"] != "待办事项" {
            return Ok(json!({"page_id": pid, "status": "state_changed_before_rework"}));
        }
        *record = self.prepare(pid, record, &task)?;
        self.dispatch(pid, record)
    }

    fn task_needs_attention(&mut self, pid: &str, record: &mut Value) -> Value {
        let current_phase = phase(record).to_owned();
        let kind = if record["error"] == MODEL_ERROR {
            "configuration_rechecked"
        } else {
            "unknown"
        };
        let stage = record["interrupted_phase"]
            .as_str()
            .unwrap_or(&current_phase)
            .to_owned();
        self.diagnostic(pid, record, kind, &[], Some(&stage));
        json!({"page_id": pid, "status": current_phase})
    }

    fn reconcile_dispatch_metadata(
        &mut self,
        pid: &str,
        record: &mut Value,
        mut task: Value,
    ) -> Result<Value> {
        let current_phase = phase(record).to_owned();
        if ["linking_chat", "claiming"].contains(&current_phase.as_str()) {
            task = self.metadata(pid, Some(record))?;
            let status = text(&task["properties"]["状态"]);
            let expected = if current_phase == "linking_chat" {
                "待办事项"
            } else {
                "进行中"
            };
            ensure!(
                status == expected,
                "Interrupted property write was not confirmed; reconcile before dispatch"
            );
            let next = if current_phase == "linking_chat" {
                "prepared"
            } else {
                "claimed"
            };
            self.checkpoint(
                pid,
                record,
                next,
                json!({"metadata_write_reconciled_at": now()}),
            )?;
            if record["diagnostic_feedback_desired"]["kind"] == "write_unknown" {
                self.diagnostic(pid, record, "metadata_reconciled", &[], None);
            }
        }
        Ok(task)
    }

    fn process_existing_round(
        &mut self,
        pid: &str,
        record: &mut Value,
        task: &Value,
    ) -> Result<Option<Value>> {
        let status = text(&task["properties"]["状态"]);
        if [
            "submitting_review",
            "submitting_blocker",
            "restoring_execution",
        ]
        .contains(&phase(record))
        {
            return self.finish_status(pid, record).map(Some);
        }
        if status == "待审核" {
            if phase(record) != "awaiting_review" {
                self.checkpoint(
                    pid,
                    record,
                    "externally_stopped",
                    json!({"remote_status": status}),
                )?;
            }
            return Ok(Some(json!({"page_id": pid, "status": "awaiting_review"})));
        }
        if WAITING.contains(&phase(record)) {
            return self.poll_verified_task(pid, record, status).map(Some);
        }
        Ok(None)
    }

    fn poll_verified_task(&mut self, pid: &str, record: &mut Value, status: &str) -> Result<Value> {
        let latest = self.metadata(pid, Some(record))?;
        if latest["properties"]["状态"] != status {
            return Ok(json!({"page_id": pid, "status": "state_changed_before_poll"}));
        }
        ensure!(
            ["进行中", "阻塞中"].contains(&status) || RETRY_BLOCKED.contains(&status),
            "Active task returned to a different state; reconcile this round"
        );
        self.poll(pid, record)
    }

    fn dispatch_pending_task(
        &mut self,
        pid: &str,
        record: &mut Value,
        task: &Value,
    ) -> Result<Value> {
        ensure!(
            ![
                "result_ready",
                "report_appended",
                "notice_ready",
                "notice_appended"
            ]
            .contains(&phase(record)),
            "Legacy delivery interrupted; execution chat must take over page delivery"
        );
        if [
            "pending",
            "awaiting_review",
            "externally_stopped",
            "terminal",
        ]
        .contains(&phase(record))
        {
            if !self.reserve_task_round(pid, record, task)? {
                return self.event("existing_unmanaged_task", json!({"page_id": pid}));
            }
        } else {
            ensure!(
                ["prepared", "chat_created", "claimed"].contains(&phase(record)),
                "Interrupted dispatch requires reconciliation before sending again"
            );
        }
        self.dispatch(pid, record)
    }

    fn reserve_task_round(&mut self, pid: &str, record: &mut Value, task: &Value) -> Result<bool> {
        if task["properties"]["状态"] != "待办事项"
            || phase(record) == "pending" && !text(&task["properties"]["AI 会话 / 线程"]).is_empty()
        {
            return Ok(false);
        }
        if let Some(bound) = record["thread_id"].as_str() {
            ensure!(
                thread_id(text(&task["properties"]["AI 会话 / 线程"])).as_deref() == Some(bound),
                "Rework chat changed"
            );
        }
        *record = self.prepare(pid, record, task)?;
        Ok(true)
    }

    pub fn prepare(&mut self, pid: &str, previous: &Value, task: &Value) -> Result<Value> {
        self.verify(task, &["待办事项"])?;
        let context = prepare_project_context(&self.config)?;
        let files = self.prepare_round_files(pid, previous)?;
        let mut record = self.round_record(pid, previous, task, &files);
        merge(&mut record, context);
        let mut snapshot_value = json!({"fetched_at": now(), "trigger": self.trigger});
        merge(&mut snapshot_value, task.clone());
        write_json(&files.snapshot, &snapshot_value)?;
        fs::create_dir_all(files.result.parent().context("Invalid receipt path")?)?;
        self.checkpoint(pid, &mut record, "prepared", json!({}))?;
        Ok(record)
    }

    fn prepare_round_files(&self, pid: &str, previous: &Value) -> Result<RoundFiles> {
        let number =
            round(previous).max(previous["last_delivered_round"].as_u64().unwrap_or(0)) + 1;
        let cwd = if let Some(cwd) = previous["cwd"].as_str().filter(|value| !value.is_empty()) {
            PathBuf::from(cwd)
        } else {
            config_path(&self.config, "task_workspace_root")?.join(pid)
        };
        let snapshot = cwd
            .join("data")
            .join(format!("round-{number:03}-dispatch.json"));
        let result = cwd
            .join("outputs")
            .join(format!("round-{number:03}-page-receipt.json"));
        ensure!(
            !snapshot.exists() && !result.exists(),
            "Existing round files require reconciliation"
        );
        self.archive_previous_round(pid, previous, number)?;
        Ok(RoundFiles {
            number,
            cwd,
            snapshot,
            result,
        })
    }

    fn archive_previous_round(&self, pid: &str, previous: &Value, number: u64) -> Result<()> {
        let archive = self
            .data
            .join("previous-rounds")
            .join(pid)
            .join(format!("before-round-{number:03}.json"));
        let has_previous = previous
            .as_object()
            .is_some_and(|record| !record.is_empty());
        ensure!(
            !has_previous || !archive.exists(),
            "Existing round checkpoint requires reconciliation"
        );
        if has_previous {
            write_json(&archive, previous)?;
        }
        Ok(())
    }

    fn round_record(&self, pid: &str, previous: &Value, task: &Value, files: &RoundFiles) -> Value {
        let props = &task["properties"];
        let title = props["任务名称"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(pid);
        let mut record = json!({
            "workflow": CONTRACT, "history_contract": ARTICLE_CONTRACT, "round": files.number,
            "dispatch_protocol": START_PROTOCOL,
            "last_delivered_round": previous["last_delivered_round"].as_u64().unwrap_or(0),
            "cwd": files.cwd, "snapshot": files.snapshot, "result_path": files.result,
            "thread_id": thread_id(text(&props["AI 会话 / 线程"])),
            "model": props["模型"], "title": title,
            "execution_token": Uuid::new_v4().simple().to_string()
        });
        for key in [
            "history_page_id",
            "round_article_page_ids",
            "diagnostic_feedback_publication",
        ] {
            if let Some(value) = previous.get(key).filter(|value| !value.is_null()) {
                record[key] = value.clone();
            }
        }
        record
    }

    fn model(&self, record: &Value) -> Result<String> {
        self.config["model_map"][text(&record["model"])]
            .as_str()
            .map(str::to_owned)
            .context("Selected model has no configured mapping")
    }

    fn dispatch(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let path = self.round_path(record, "data", "page-execution.json")?;
        ensure!(
            !path.exists(),
            "Existing dispatch receipt requires reconciliation before sending"
        );
        let route = self.dispatch_route(pid, record)?;
        let settings = self.execution_settings(record)?;
        self.ensure_execution_thread(pid, record, &settings)?;
        if route.is_none() {
            self.bind_execution_thread(pid, record)?;
        }
        self.executor.resume_thread(
            text(&record["thread_id"]),
            &settings.model,
            &settings.sandbox,
            &settings.reviewer,
        )?;
        if record["dispatch_protocol"] == START_PROTOCOL {
            return self.prepare_start(pid, record);
        }
        self.dispatch_legacy(pid, record, &path, &settings.model)
    }

    fn dispatch_route(&mut self, pid: &str, record: &Value) -> Result<Option<String>> {
        let task = self.metadata(pid, None)?;
        let properties = &task["properties"];
        let expected = if phase(record) == "claimed" {
            "进行中"
        } else {
            "待办事项"
        };
        ensure!(
            properties["状态"] == expected,
            "Status changed before dispatch"
        );
        ensure!(
            properties["模型"] == record["model"],
            "Model changed before dispatch"
        );
        let route = thread_id(text(&properties["AI 会话 / 线程"]));
        if phase(record) == "chat_created" {
            ensure!(
                route.is_none() || route.as_deref() == record["thread_id"].as_str(),
                "Chat was changed before binding"
            );
        } else {
            ensure!(
                route.as_deref() == record["thread_id"].as_str(),
                "Chat changed before claim"
            );
        }
        Ok(route)
    }

    fn execution_settings(&self, record: &Value) -> Result<ExecutionSettings> {
        let model = self.model(record)?;
        let sandbox = self.config["execution_sandbox"]
            .as_str()
            .unwrap_or("workspace-write")
            .to_owned();
        let reviewer = text(&self.config["execution_approvals_reviewer"]).to_owned();
        Ok(ExecutionSettings {
            model,
            sandbox,
            reviewer,
        })
    }

    fn ensure_execution_thread(
        &mut self,
        pid: &str,
        record: &mut Value,
        settings: &ExecutionSettings,
    ) -> Result<()> {
        if text(&record["thread_id"]).is_empty() {
            self.checkpoint(pid, record, "creating_chat", json!({}))?;
            let created = self.executor.start_thread(
                &settings.model,
                text(&record["cwd"]),
                text(&record["title"]),
                &settings.sandbox,
                &settings.reviewer,
            )?;
            let thread = created.get("thread").unwrap_or(&created);
            let tid = canonical_id(thread["id"].as_str().context("Created chat has no ID")?)?;
            self.checkpoint(pid, record, "chat_created", json!({"thread_id": tid}))?;
        }
        Ok(())
    }

    fn bind_execution_thread(&mut self, pid: &str, record: &mut Value) -> Result<()> {
        let latest = self.metadata(pid, None)?;
        ensure!(
            latest["properties"]["状态"] == "待办事项"
                && text(&latest["properties"]["AI 会话 / 线程"]).is_empty()
                && latest["properties"]["模型"] == record["model"],
            "Task changed before chat binding"
        );
        self.checkpoint(pid, record, "linking_chat", json!({}))?;
        self.update(
            pid,
            json!({"AI 会话 / 线程": format!("codex://threads/{}", text(&record["thread_id"]))}),
        )?;
        Ok(())
    }

    fn claim_legacy_task(&mut self, pid: &str, record: &mut Value) -> Result<()> {
        let task = self.metadata(pid, Some(record))?;
        if phase(record) != "claimed" {
            ensure!(
                task["properties"]["状态"] == "待办事项",
                "Task changed before claim"
            );
            self.checkpoint(pid, record, "claiming", json!({}))?;
            self.update(pid, json!({"状态": "进行中"}))?;
            self.checkpoint(pid, record, "claimed", json!({}))?;
        }
        let latest = self.metadata(pid, Some(record))?;
        ensure!(
            latest["properties"]["状态"] == "进行中",
            "Claimed task changed state before sending"
        );
        Ok(())
    }

    fn dispatch_legacy(
        &mut self,
        pid: &str,
        record: &mut Value,
        path: &Path,
        model: &str,
    ) -> Result<Value> {
        self.claim_legacy_task(pid, record)?;
        let prompt = self.worker_prompt(pid, record)?;
        let draft = self
            .executor
            .prepare_dispatch(text(&record["thread_id"]), &prompt)?;
        self.checkpoint(pid, record, "executing", json!({}))?;
        let execution = self.executor.send_dispatch(&draft, &prompt, model)?;
        write_json(path, &execution)?;
        self.checkpoint(
            pid,
            record,
            "waiting_execution",
            json!({"execution_path": path}),
        )?;
        if execution["status"] == "completed" {
            return self.accept_execution(pid, record, &execution);
        }
        Ok(json!({"page_id": pid, "status": "executing",
                  "reconnect_before_next_task": (["needs_attention", "timeout"].contains(&text(&execution["status"])))}))
    }

    fn round_path(&self, record: &Value, folder: &str, suffix: &str) -> Result<PathBuf> {
        Ok(
            Path::new(record["cwd"].as_str().context("Missing task workspace")?)
                .join(folder)
                .join(format!("round-{:03}-{suffix}", round(record))),
        )
    }

    pub fn worker_prompt(&self, pid: &str, record: &Value) -> Result<String> {
        let number = round(record);
        let marker = format!("notion-program/{pid}/round-{number:03}");
        let history = self.worker_history(pid, record)?;
        let context = project_context_prompt(pid, record)?;
        let destination = if history.is_empty() {
            "把完整报告、证据和需要审核的事项直接写入本页，并核验实际写入结果。"
        } else {
            "按下方轮次契约保存完整报告、证据和需要审核的事项，任务主页只保留逐轮摘要与跳转链接，并核验实际写入结果。"
        };
        let example = self.worker_receipt_example(pid, record);
        Ok(format!(
            "派发标记：{marker}。\n\
             Herman 最新分工取代旧规则：调度器只控制状态与聊天路由，不读取、修改或校验页面正文。\
             本执行会话拥有本任务页面正文的完整修改权，可按任务需要重写、整理、增删内容和调整结构。\
             旧的“不要修改 Notion、调度程序负责交付、正文必须冻结不变”要求已失效。\
             任务状态及 AI 会话 / 线程属性由调度器负责，不代替 Herman 审核通过。\n\
             任务 page_id={pid}，data_source_id={}，执行人={}。原聊天继续第 {number} 轮。\
             属性派发记录：{}。请使用 Notion 插件自行 fetch 最新页面，核对范围与指派，读取当前任务和审阅要求，实际执行并验证。\
             正文编辑前读取 notion://docs/notion-flavored-markdown-spec。数据长或截断时查明并处理，不把遗漏内容冒充完整读取。\n\
             {context}{destination}\n{history}\
             把已使用的指令归入该轮历史；核验归档后逐项比对并定向清空当前审阅区仍相同的已消费内容，留下空白供 Herman 下一轮填写。\
             归档或核验失败时保留原文并返回 blocked；不以缺少已验证的原子并发保护为阻塞条件。\
             保留未消费的新指令和已有历史，避免重复交付。不要只给本机路径。调度器不检查固定标题、正文哈希或标记。\
             待办中没有明确可执行的新要求时说明缺少什么并 blocked，不能重复执行旧要求。失败时自行在页面说明原因，不把失败当成功。\n\
             完成后将执行收据写入 {}：{}。该收据是你完成写入和核验的声明；未实际成功不能填写 true。\
             失败则 status=blocked 并填写 reason。保留旧轮次文件。阻塞收据可增加 blocker.category，值为 permission、user_input、tool_connection、external_dependency、task_obstacle 或 unknown；只依据实际证据分类。\
             调度器只公开分类，不读取正文也不公开原始 reason。具体原因、证据及下一步写入本轮文章和主页摘要。\
             即使人工续接也继续本轮并更新此收据，不另开轮次。不要向其他聊天发送消息，除非任务或 Herman 已明确授权。\n\
             本机诊断位置：{}/outputs/round-{number:03}-diagnostic.md；路径或桌面提醒不代表通知送达或已读。\n\
             实际计算收据文件 SHA256，最后回复单独一行：NOTION_RESULT {marker} <实际SHA256值>。",
            text(&self.config["data_source_id"]),
            text(&self.config["executor"]),
            text(&record["snapshot"]),
            text(&record["result_path"]),
            serde_json::to_string(&example)?,
            text(&record["cwd"])
        ))
    }

    fn worker_history(&self, pid: &str, record: &Value) -> Result<String> {
        if ["per_task_history_page_v1", ARTICLE_CONTRACT]
            .contains(&text(&record["history_contract"]))
        {
            history_prompt(pid, record)
        } else {
            Ok(String::new())
        }
    }

    fn worker_receipt_example(&self, pid: &str, record: &Value) -> Value {
        json!({"status":"completed", "page_id":pid, "round":round(record),
            "execution_token":record["execution_token"],
            "notion_delivery":{"report_written":true,"verified":true,"review_handled":true,
                "reference":"实际交付页URL与本轮位置说明","delivered_at":"实际时间"}})
    }

    fn prepare_start(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let task = self.metadata(pid, Some(record))?;
        ensure!(
            task["properties"]["状态"] == "待办事项",
            "Task changed before local reservation"
        );
        let prompt = self.worker_prompt(pid, record)?;
        let draft = self
            .executor
            .prepare_dispatch(text(&record["thread_id"]), &prompt)?;
        let intent = self.round_path(record, "data", "start-intent.json")?;
        ensure!(
            !intent.exists(),
            "Existing start intent requires reconciliation"
        );
        write_json(
            &intent,
            &json!({"at": now(), "draft": draft, "prompt": prompt}),
        )?;
        let path = self.round_path(record, "data", "page-execution.json")?;
        self.checkpoint(
            pid,
            record,
            "start_prepared",
            json!({"start_intent_path": intent, "execution_path": path}),
        )?;
        self.continue_start(pid, record)
    }

    fn continue_start(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        if phase(record) == "start_status_confirming" {
            return self.publish_running(pid, record);
        }
        let path = PathBuf::from(
            record["execution_path"]
                .as_str()
                .context("Missing execution path")?,
        );
        let intent = read_json(Path::new(text(&record["start_intent_path"])), Value::Null)?;
        ensure!(
            intent["draft"].is_object() && intent["prompt"].is_string(),
            "Invalid saved start intent"
        );
        if phase(record) == "start_prepared"
            && let Some(outcome) = self.send_reserved_start(pid, record, &intent, &path)?
        {
            return Ok(outcome);
        }
        let mut receipt = if path.exists() {
            read_json(&path, Value::Null)?
        } else {
            intent["draft"].clone()
        };
        if let Some(anchor) = record.get("dispatched_turn_id") {
            receipt["dispatched_turn_id"] = anchor.clone();
        }
        let execution = self.executor.poll_execution(&receipt)?;
        if !path.exists() && !text(&execution["dispatched_turn_id"]).is_empty() {
            receipt["reconciled_from_history"] = json!(true);
            receipt["dispatched_turn_id"] = execution["dispatched_turn_id"].clone();
            write_json(&path, &receipt)?;
        }
        self.confirm_start(pid, record, &execution)
    }

    fn send_reserved_start(
        &mut self,
        pid: &str,
        record: &mut Value,
        intent: &Value,
        path: &Path,
    ) -> Result<Option<Value>> {
        let latest = self.metadata(pid, Some(record))?;
        ensure!(
            latest["properties"]["状态"] == "待办事项",
            "Task changed before sending reserved execution"
        );
        if path.exists() {
            self.checkpoint(pid, record, "start_confirming", json!({}))?;
            return Ok(None);
        }
        self.checkpoint(pid, record, "start_sending", json!({}))?;
        let execution = self.send_start_dispatch(pid, record, intent)?;
        write_json(path, &execution)?;
        self.checkpoint(pid, record, "start_confirming", json!({}))?;
        if execution["transport"] == "app_server" && execution["turn"].is_object() {
            return self.confirm_start(pid, record, &execution).map(Some);
        }
        Ok(None)
    }

    fn send_start_dispatch(
        &mut self,
        pid: &str,
        record: &mut Value,
        intent: &Value,
    ) -> Result<Value> {
        let model = self.model(record)?;
        match self
            .executor
            .send_dispatch(&intent["draft"], text(&intent["prompt"]), &model)
        {
            Ok(execution) => Ok(execution),
            Err(error) => {
                if presend_error(&error) {
                    self.checkpoint(pid, record, "start_prepared", json!({}))?;
                }
                Err(error)
            }
        }
    }

    fn observe_start(&mut self, pid: &str, record: &mut Value, execution: &Value) -> Result<bool> {
        let observation = self.round_path(record, "data", "start-observation.json")?;
        write_json(&observation, execution)?;
        record["execution_observation_path"] = json!(observation);
        let turn = &execution["turn"];
        let anchor = text(&execution["dispatched_turn_id"]);
        let dispatched = &execution["dispatch_turn"];
        let associated = !anchor.is_empty()
            && (text(&turn["id"]) == anchor
                || execution["continuation"] == true
                    && text(&dispatched["id"]) == anchor
                    && (["inProgress", "completed"].contains(&text(&dispatched["status"]))
                        || !dispatched["startedAt"].is_null()));
        self.event("start_observed", json!({"page_id":pid,"round":round(record),"turn_id":turn["id"],
            "dispatched_turn_id":execution["dispatched_turn_id"],"execution_status":execution["status"],"turn_status":turn["status"]}))?;
        Ok(associated)
    }

    fn confirm_start(&mut self, pid: &str, record: &mut Value, execution: &Value) -> Result<Value> {
        if !self.observe_start(pid, record, execution)? {
            return self.wait_for_start(pid, record);
        }
        record["dispatched_turn_id"] = execution["dispatched_turn_id"].clone();
        let turn = &execution["turn"];
        if execution["status"] == "completed" && turn["status"] == "completed" {
            self.resolve_start(pid, record)?;
            return self.accept_execution(pid, record, execution);
        }
        let observed_flags = flags(execution);
        if ["failed", "interrupted", "needs_attention"].contains(&text(&execution["status"]))
            || !observed_flags.is_empty()
        {
            return self.handle_start_interruption(pid, record, execution);
        }
        if turn["status"] != "inProgress"
            || !["in_progress", "timeout"].contains(&text(&execution["status"]))
            || execution["transport"] != "app_server" && execution["thread_status"] != "active"
        {
            return self.wait_for_start(pid, record);
        }
        self.checkpoint(
            pid,
            record,
            "start_status",
            json!({"start_resolved_at":now(),"execution_started_at":now()}),
        )?;
        self.publish_running(pid, record)
    }

    fn resolve_start(&mut self, pid: &str, record: &mut Value) -> Result<()> {
        self.checkpoint(
            pid,
            record,
            "start_confirming",
            json!({"start_resolved_at":now()}),
        )
    }

    fn wait_for_start(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        self.checkpoint(pid, record, "start_confirming", json!({}))?;
        self.diagnostic(pid, record, "start_pending", &[], None);
        Ok(json!({"page_id":pid,"status":"waiting_start"}))
    }

    fn handle_start_interruption(
        &mut self,
        pid: &str,
        record: &mut Value,
        execution: &Value,
    ) -> Result<Value> {
        self.resolve_start(pid, record)?;
        if self.retry_policy()?.is_some()
            && ["failed", "interrupted"].contains(&text(&execution["status"]))
        {
            let observed = if execution.get("automatic_retry_signal").is_some() {
                execution.clone()
            } else {
                self.executor.poll_execution(execution)?
            };
            self.checkpoint(pid, record, "waiting_execution", json!({}))?;
            if let Some(outcome) = self.automatic_retry(pid, record, &observed)? {
                return Ok(outcome);
            }
            self.checkpoint(pid, record, "start_confirming", json!({}))?;
        }
        self.block(
            pid,
            record,
            "Execution did not start normally",
            text(&execution["status"]),
            &flags(execution),
        )
    }

    fn publish_running(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let task = self.metadata(pid, Some(record))?;
        let status = text(&task["properties"]["状态"]);
        self.event("running_status_read", json!({"page_id":pid,"round":round(record),"observed_status":status,"phase":phase(record)}))?;
        if status == "进行中" {
            return self.confirm_running_status(pid, record);
        }
        ensure!(
            status == "待办事项",
            "Task state changed before publishing execution: {status}"
        );
        if phase(record) == "start_status_confirming" {
            self.diagnostic(pid, record, "write_unknown", &[], None);
            return Ok(json!({"page_id":pid,"status":"waiting_running_status"}));
        }
        self.checkpoint(
            pid,
            record,
            "start_status_confirming",
            json!({"running_status_attempted_at":now()}),
        )?;
        match self.update(pid, json!({"状态":"进行中"})) {
            Ok(receipt) => record["running_status_receipt"] = receipt,
            Err(error) => {
                if external(&error).is_some_and(|error| !error.outcome_unknown) {
                    self.checkpoint(pid, record, "start_status", json!({}))?;
                }
                return Err(error);
            }
        }
        self.checkpoint(pid, record, "start_status_confirming", json!({}))?;
        self.publish_running(pid, record)
    }

    fn confirm_running_status(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        self.checkpoint(
            pid,
            record,
            "waiting_execution",
            json!({"running_status_verified_at":now()}),
        )?;
        if record.get("diagnostic_feedback_desired").is_some()
            || record.get("diagnostic_feedback_publication").is_some()
        {
            self.diagnostic(pid, record, "resumed", &[], None);
        }
        Ok(json!({"page_id":pid,"status":"executing"}))
    }

    fn execution_receipt(&self, record: &Value) -> Result<Value> {
        let mut receipt = read_json(
            Path::new(
                record["execution_path"]
                    .as_str()
                    .context("Missing execution receipt")?,
            ),
            Value::Null,
        )?;
        ensure!(receipt.is_object(), "Execution receipt must be an object");
        if let Some(anchor) = record.get("dispatched_turn_id") {
            receipt["dispatched_turn_id"] = anchor.clone();
        }
        Ok(receipt)
    }

    fn poll(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let receipt = self.execution_receipt(record)?;
        let execution = match self.executor.poll_execution(&receipt) {
            Ok(execution) => execution,
            Err(error) if association_error(&error) => {
                return self.block(pid, record, &error.to_string(), "association", &[]);
            }
            Err(error) => return Err(error),
        };
        let path = self.round_path(record, "data", "page-observation.json")?;
        write_json(&path, &execution)?;
        if !text(&execution["dispatched_turn_id"]).is_empty() {
            record["dispatched_turn_id"] = execution["dispatched_turn_id"].clone();
        }
        record["execution_observation_path"] = json!(path);
        self.save(pid, record)?;
        if execution["status"] == "in_progress" {
            return self.observe_running_execution(pid, record, &execution);
        }
        if execution["status"] != "completed" {
            if let Some(outcome) = self.automatic_retry(pid, record, &execution)? {
                return Ok(outcome);
            }
            return self.block(
                pid,
                record,
                &format!("执行聊天状态：{}", text(&execution["status"])),
                text(&execution["status"]),
                &flags(&execution),
            );
        }
        self.accept_execution(pid, record, &execution)
    }

    fn observe_running_execution(
        &mut self,
        pid: &str,
        record: &mut Value,
        execution: &Value,
    ) -> Result<Value> {
        if phase(record) == "execution_blocked"
            && execution["turn"]["status"] == "inProgress"
            && execution["thread_status"] == "active"
            && flags(execution).is_empty()
        {
            self.checkpoint(pid, record, "restoring_execution", json!({}))?;
            return self.finish_status(pid, record);
        }
        if self
            .round_path(record, "outputs", "diagnostic.md")?
            .exists()
            || record.get("diagnostic_feedback_desired").is_some()
        {
            self.diagnostic(pid, record, "resumed", &[], None);
        }
        Ok(json!({"page_id":pid,"status":"executing"}))
    }

    pub fn read_worker_receipt(&self, pid: &str, record: &Value) -> Result<(Value, String)> {
        ensure!(
            record["workflow"] == CONTRACT,
            "旧轮次尚未确认执行会话自行交付，须在原聊天补交新收据"
        );
        let raw = fs::read(Path::new(
            record["result_path"]
                .as_str()
                .context("Missing worker receipt path")?,
        ))?;
        let result: Value = serde_json::from_slice(&raw)?;
        ensure!(
            result["page_id"] == pid
                && result["round"] == record["round"]
                && result["execution_token"] == record["execution_token"],
            "执行收据不属于本任务本轮"
        );
        ensure!(
            result["status"] == "completed",
            "{}",
            result["reason"].as_str().unwrap_or("执行未完成")
        );
        let delivery = &result["notion_delivery"];
        for field in ["report_written", "verified", "review_handled"] {
            ensure!(delivery[field] == true, "执行会话尚未确认报告已写入并核验");
        }
        for field in ["reference", "delivered_at"] {
            ensure!(
                !text(&delivery[field]).trim().is_empty(),
                "执行会话尚未确认报告已写入并核验"
            );
        }
        validate_history_delivery(pid, record, delivery, &self.state)?;
        validate_project_context_receipt(pid, record, &result)?;
        let hash = digest(&raw);
        if let Some(expected) = record["receipt_sha256"].as_str() {
            ensure!(hash == expected, "完成收据在状态提交期间发生变化");
        }
        Ok((result, hash))
    }

    fn accept_execution(
        &mut self,
        pid: &str,
        record: &mut Value,
        execution: &Value,
    ) -> Result<Value> {
        let validated = self.validated_execution_receipt(pid, record, execution);
        let (result, hash) = match validated {
            Ok(result) => result,
            Err(error) => {
                let kind = feedback::receipt_blocker_kind(pid, record, execution);
                return self.block(pid, record, &error.to_string(), kind, &[]);
            }
        };
        self.record_round_delivery(pid, record, &result)?;
        self.checkpoint(
            pid,
            record,
            "submitting_review",
            json!({"receipt_sha256":hash}),
        )?;
        self.finish_status(pid, record)
    }

    fn validated_execution_receipt(
        &self,
        pid: &str,
        record: &Value,
        execution: &Value,
    ) -> Result<(Value, String)> {
        let (result, hash) = self.read_worker_receipt(pid, record)?;
        let expected = format!(
            "NOTION_RESULT notion-program/{pid}/round-{:03} {hash}",
            round(record)
        );
        ensure!(
            text(&execution["text"])
                .lines()
                .any(|line| line == expected),
            "执行回合尚未确认当前收据的 SHA256"
        );
        Ok((result, hash))
    }

    fn record_round_delivery(
        &mut self,
        pid: &str,
        record: &mut Value,
        result: &Value,
    ) -> Result<()> {
        let history =
            validate_history_delivery(pid, record, &result["notion_delivery"], &self.state)?;
        if record["automatic_recovery"].is_object() {
            record["automatic_recovery"]["state"] = json!("completed");
            record["automatic_recovery"]["completed_at"] = json!(now());
        }
        if self
            .round_path(record, "outputs", "diagnostic.md")?
            .exists()
        {
            self.diagnostic(pid, record, "resolved", &[], None);
        }
        if let Some(history) = history {
            record["history_page_id"] = json!(history);
        }
        if let Some(article) = result["notion_delivery"].get("article") {
            if !record["round_article_page_ids"].is_object() {
                record["round_article_page_ids"] = json!({});
            }
            let number = round(record).to_string();
            record["round_article_page_ids"][number] =
                json!(canonical_id(text(&article["page_id"]))?);
        }
        Ok(())
    }

    fn block(
        &mut self,
        pid: &str,
        record: &mut Value,
        reason: &str,
        kind: &str,
        observed_flags: &[String],
    ) -> Result<Value> {
        record["blocker_kind"] = json!(kind);
        record["blocker_flags"] = json!(
            observed_flags
                .iter()
                .filter(|flag| ["waitingOnApproval", "waitingOnUserInput"].contains(&flag.as_str()))
                .collect::<Vec<_>>()
        );
        self.diagnostic(pid, record, kind, observed_flags, None);
        if record["blocker_reason"] == reason && phase(record) == "execution_blocked" {
            return Ok(json!({"page_id":pid,"status":"execution_blocked"}));
        }
        self.event(
            "execution_attention",
            json!({"page_id":pid,"round":round(record),"reason":reason}),
        )?;
        self.checkpoint(
            pid,
            record,
            "submitting_blocker",
            json!({"blocker_reason":reason}),
        )?;
        self.finish_status(pid, record)
    }

    fn finish_status(&mut self, pid: &str, record: &mut Value) -> Result<Value> {
        let mut submission = StatusSubmission::new(record);
        if submission.completed {
            self.read_worker_receipt(pid, record)?;
        }
        let task = self.metadata(pid, Some(record))?;
        let status = text(&task["properties"]["状态"]);
        if !submission.completed && !submission.restoring && RETRY_BLOCKED.contains(&status) {
            submission.target = status.to_owned();
        }
        if TERMINAL.contains(&status) {
            self.checkpoint(pid, record, "terminal", json!({"remote_status":status}))?;
            return Ok(json!({"page_id":pid,"status":status}));
        }
        if let Some(outcome) = self.reconcile_status_submission(pid, record, status, &submission)? {
            return Ok(outcome);
        }
        self.validate_submission_status(record, status, &submission)?;
        let receipt = self.submit_status(pid, record, status, &submission.target)?;
        if submission.restoring && status != submission.target {
            return self.finish_status(pid, record);
        }
        self.confirm_status_intent(record);
        self.complete_status_submission(pid, record, &submission, receipt)
    }

    fn pending_status_confirmed(
        &mut self,
        pid: &str,
        record: &mut Value,
        status: &str,
        target: &str,
    ) -> Result<bool> {
        if record["pending_status_write"].is_object()
            && record["pending_status_write"]["state"] != "rejected"
        {
            ensure!(
                record["pending_status_write"]["phase"] == phase(record)
                    && record["pending_status_write"]["target"] == target,
                "An earlier status write requires reconciliation"
            );
            if status != target {
                self.diagnostic(pid, record, "write_unknown", &[], None);
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn reconcile_status_submission(
        &mut self,
        pid: &str,
        record: &mut Value,
        status: &str,
        submission: &StatusSubmission,
    ) -> Result<Option<Value>> {
        if !self.pending_status_confirmed(pid, record, status, &submission.target)? {
            return Ok(Some(
                json!({"page_id":pid,"status":"waiting_status_confirmation"}),
            ));
        }
        if submission.restoring && !["阻塞中", "进行中"].contains(&status) {
            self.checkpoint(pid, record, "execution_blocked", json!({}))?;
            return Ok(Some(
                json!({"page_id":pid,"status":"recovery_state_changed"}),
            ));
        }
        Ok(None)
    }

    fn validate_submission_status(
        &self,
        record: &Value,
        status: &str,
        submission: &StatusSubmission,
    ) -> Result<()> {
        let before_running = record["dispatch_protocol"] == START_PROTOCOL
            && !record["start_resolved_at"].is_null()
            && record["running_status_verified_at"].is_null()
            && record["start_final_write_attempted_at"].is_null();
        ensure!(
            ["进行中", "阻塞中", RETRY_REQUEST].contains(&status)
                || status == submission.target
                || RETRY_BLOCKED.contains(&status)
                || before_running && status == "待办事项",
            "Task state changed before submission"
        );
        Ok(())
    }

    fn submit_status(
        &mut self,
        pid: &str,
        record: &mut Value,
        status: &str,
        target: &str,
    ) -> Result<Value> {
        if status != target {
            if record["pending_status_write"].is_object() {
                ensure!(
                    record["pending_status_write"]["phase"] == phase(record)
                        && record["pending_status_write"]["target"] == target,
                    "An earlier status write requires reconciliation"
                );
            }
            if status == "待办事项" {
                let current = phase(record).to_owned();
                self.checkpoint(
                    pid,
                    record,
                    &current,
                    json!({"start_final_write_attempted_at":now()}),
                )?;
            }
            self.update_status_with_intent(pid, record, target)
        } else {
            Ok(json!({"reconciled_by_metadata_read":true,"status":target}))
        }
    }

    fn update_status_with_intent(
        &mut self,
        pid: &str,
        record: &mut Value,
        target: &str,
    ) -> Result<Value> {
        self.write_status_intent(pid, record, target)?;
        let receipt = match self.update(pid, json!({"状态":target})) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.status_error(pid, record, &error)?;
                return Err(error);
            }
        };
        record["pending_status_write"]["state"] = json!("acknowledged");
        record["pending_status_write"]["receipt"] = receipt.clone();
        self.save(pid, record)?;
        Ok(receipt)
    }

    fn complete_status_submission(
        &mut self,
        pid: &str,
        record: &mut Value,
        submission: &StatusSubmission,
        receipt: Value,
    ) -> Result<Value> {
        if submission.restoring {
            return self.finish_execution_restoration(pid, record, receipt);
        }
        if submission.completed {
            self.checkpoint(pid, record, "awaiting_review", json!({"last_delivered_round":round(record),"submitted_at":now(),"submit_receipt":receipt}))?;
        } else {
            self.checkpoint(
                pid,
                record,
                "execution_blocked",
                json!({"blocker_status_receipt":receipt}),
            )?;
        }
        self.submission_feedback(pid, record, submission.completed);
        Ok(json!({"page_id":pid,"status":phase(record),"round":round(record)}))
    }

    fn finish_execution_restoration(
        &mut self,
        pid: &str,
        record: &mut Value,
        receipt: Value,
    ) -> Result<Value> {
        for field in [
            "blocker_kind",
            "blocker_flags",
            "blocker_reason",
            "blocker_status_receipt",
        ] {
            remove(record, field);
        }
        self.checkpoint(
            pid,
            record,
            "waiting_execution",
            json!({"resumed_at":now(),"running_recovery_receipt":receipt}),
        )?;
        self.diagnostic(pid, record, "resumed", &[], None);
        Ok(json!({"page_id":pid,"status":"executing","round":round(record)}))
    }

    fn submission_feedback(&mut self, pid: &str, record: &mut Value, completed: bool) {
        if record.get("diagnostic_feedback_desired").is_some()
            || record.get("diagnostic_feedback_publication").is_some()
        {
            let kind = if completed {
                "resolved".to_owned()
            } else {
                record["blocker_kind"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned()
            };
            let observed = flags(&json!({"active_flags":record["blocker_flags"]}));
            self.diagnostic(pid, record, &kind, &observed, None);
        }
    }

    fn write_status_intent(&mut self, pid: &str, record: &mut Value, target: &str) -> Result<()> {
        record["pending_status_write"] =
            json!({"phase":phase(record),"target":target,"state":"sent","at":now()});
        self.save(pid, record)
    }

    fn status_error(&mut self, pid: &str, record: &mut Value, error: &anyhow::Error) -> Result<()> {
        let rejected = external(error).is_some_and(|error| !error.outcome_unknown);
        record["pending_status_write"]["state"] = json!(if rejected {
            "rejected"
        } else {
            "outcome_unknown"
        });
        if rejected {
            remove(record, "start_final_write_attempted_at");
        }
        self.save(pid, record)
    }

    fn confirm_status_intent(&self, record: &mut Value) {
        if let Some(mut pending) = remove(record, "pending_status_write") {
            pending["confirmed_at"] = json!(now());
            record["last_status_write"] = pending;
        }
    }
}
