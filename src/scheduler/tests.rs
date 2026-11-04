use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

const PID: &str = "11111111-1111-4111-8111-111111111111";
const SOURCE: &str = "22222222-2222-4222-8222-222222222222";
const TID: &str = "33333333-3333-4333-8333-333333333333";
const HISTORY: &str = "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa";
const CONTEXT: &str = "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb";

#[derive(Clone, Copy)]
enum Failure {
    Rejected,
    UnknownBefore,
    UnknownAfter,
}

#[derive(Default)]
struct Store {
    properties: Value,
    writes: Vec<Value>,
    failure: Option<(String, Failure)>,
    reads: usize,
    fail_read: bool,
    empty_discovery: bool,
}

#[derive(Clone)]
struct FakeNotion(Rc<RefCell<Store>>);

fn error(operation: &str, kind: &str, unknown: bool) -> anyhow::Error {
    ExternalError {
        operation: operation.into(),
        kind: kind.into(),
        outcome_unknown: unknown,
        reconnectable: false,
    }
    .into()
}

impl Notion for FakeNotion {
    fn query_tasks(&mut self) -> Result<Vec<Value>> {
        let store = self.0.borrow();
        Ok(if store.empty_discovery {
            vec![]
        } else {
            vec![store.properties.clone()]
        })
    }

    fn get_task_metadata(&mut self, pid: &str) -> Result<Value> {
        let mut store = self.0.borrow_mut();
        store.reads += 1;
        if store.fail_read {
            return Err(error("query", "read", false));
        }
        Ok(json!({"id":pid,"source_id":SOURCE,"properties":store.properties}))
    }

    fn update_properties(&mut self, _pid: &str, properties: &Value) -> Result<Value> {
        ensure!(
            properties.as_object().unwrap().keys().all(|key| {
                ["状态", "AI 会话 / 线程", "调度诊断"].contains(&key.as_str())
            }),
            "Unexpected property mutation"
        );
        let mut store = self.0.borrow_mut();
        store.writes.push(properties.clone());
        let matched = store.failure.as_ref().is_some_and(|(target, _)| {
            properties["状态"] == target.as_str()
                || target == "feedback" && properties.get("调度诊断").is_some()
                || target == "route" && properties.get("AI 会话 / 线程").is_some()
        });
        let failure = if matched {
            store.failure.take().map(|(_, failure)| failure)
        } else {
            None
        };
        if matches!(failure, Some(Failure::Rejected | Failure::UnknownBefore)) {
            return Err(error(
                "update",
                "injected_write",
                matches!(failure, Some(Failure::UnknownBefore)),
            ));
        }
        merge(&mut store.properties, properties.clone());
        if matches!(failure, Some(Failure::UnknownAfter)) {
            return Err(error("update", "injected_write", true));
        }
        Ok(json!({"ok":true}))
    }

    fn fetch(&mut self, _pid: &str) -> Result<Value> {
        panic!("Scheduler fetched task body")
    }

    fn call(&mut self, _role: &str, _args: &Value) -> Result<Value> {
        panic!("Scheduler used arbitrary Notion tool")
    }
}

struct FakeExecutor {
    state_path: PathBuf,
    created: usize,
    sent: usize,
    continued: usize,
    status: String,
    queued: bool,
    active_flags: Vec<String>,
    send_failure: Option<String>,
    continuation_failure: Option<String>,
    transient: bool,
    problem: Option<String>,
    context: Option<Value>,
    prompts: Vec<String>,
    observations: VecDeque<Value>,
    idle_override: Option<bool>,
    idle_failure: bool,
    idle_checks: Vec<String>,
    reject_associated_poll: bool,
}

impl FakeExecutor {
    fn record(&self) -> Value {
        read_json(&self.state_path, json!({})).unwrap()[PID].clone()
    }

    fn result(&self, record: &Value) -> Result<(Value, String)> {
        let number = round(record);
        let mut result = json!({
            "page_id":PID,"round":number,"execution_token":record["execution_token"],"status":"completed",
            "notion_delivery":{"report_written":true,"verified":true,"review_handled":true,
                "reference":"verified article","delivered_at":"2026-11-04T12:00:00+08:00"}
        });
        if !record["history_contract"].is_null() {
            result["notion_delivery"]["history"] = json!({
                "page_id":HISTORY,"task_page_id":PID,"link_verified":true,"report_verified":true,"task_summary_only":true
            });
        }
        if record["history_contract"] == ARTICLE_CONTRACT {
            result["notion_delivery"]["article"] = json!({
                "page_id":format!("44444444-4444-4444-8444-{number:012}"),
                "task_page_id":PID,"history_page_id":HISTORY,"round":number,
                "link_verified":true,"report_verified":true,"instructions_verified":true,"task_summary_verified":true
            });
        }
        if let Some(context) = &self.context {
            result["project_context"] = context.clone();
        }
        self.apply_receipt_problem(&mut result);
        self.write_result(record, result)
    }

    fn apply_receipt_problem(&self, result: &mut Value) {
        match self.problem.as_deref() {
            Some("stale") => result["execution_token"] = json!("stale"),
            Some("unuploaded") => result["notion_delivery"]["verified"] = json!(false),
            Some("blocked") => {
                result["status"] = json!("blocked");
                result["reason"] = json!("PRIVATE_REASON");
                result["blocker"] = json!({"category":"permission"});
            }
            _ => {}
        }
    }

    fn write_result(&self, record: &Value, result: Value) -> Result<(Value, String)> {
        if self.problem.as_deref() == Some("missing") {
            return Ok((result, "done".into()));
        }
        let path = Path::new(text(&record["result_path"]));
        write_json(path, &result)?;
        let hash = digest(&fs::read(path)?);
        Ok((
            result,
            if self.problem.as_deref() == Some("unbound") {
                "done".into()
            } else {
                format!(
                    "NOTION_RESULT notion-program/{PID}/round-{:03} {hash}",
                    round(record)
                )
            },
        ))
    }

    fn observe(&mut self, receipt: &Value) -> Result<Value> {
        if let Some(observation) = self.observations.pop_front() {
            return Ok(observation);
        }
        if self.queued {
            let mut result = receipt.clone();
            result["status"] = json!("in_progress");
            result["thread_status"] = json!("idle");
            return Ok(result);
        }
        let number = round(&self.record());
        let identifier = if receipt["retry_id"].is_string() {
            format!("retry-{}", self.continued)
        } else {
            format!("turn-{number}")
        };
        let mut result = receipt.clone();
        let running = self.status == "in_progress";
        result["status"] = json!(self.status);
        result["dispatched_turn_id"] = json!(identifier);
        result["turn"] =
            json!({"id":identifier,"status":if running {"inProgress"} else {&self.status}});
        result["thread_status"] = json!(if running { "active" } else { "idle" });
        result["active_flags"] = json!(self.active_flags);
        result["waiting_flags"] = json!(self.active_flags);
        result["continuation"] = json!(false);
        result["automatic_retry_signal"] = json!({"eligible":self.transient,
            "error_code":"responseStreamDisconnected","reason":"structured_transport_failure"});
        if self.status == "completed" {
            result["text"] = json!(self.result(&self.record())?.1);
        }
        Ok(result)
    }
}

impl Executor for FakeExecutor {
    fn start_thread(
        &mut self,
        _model: &str,
        _cwd: &str,
        _title: &str,
        _sandbox: &str,
        _reviewer: &str,
    ) -> Result<Value> {
        self.created += 1;
        Ok(json!({"id":TID}))
    }
    fn resume_thread(
        &mut self,
        _thread: &str,
        _model: &str,
        _sandbox: &str,
        _reviewer: &str,
    ) -> Result<Value> {
        Ok(json!({"id":TID}))
    }
    fn prepare_dispatch(&mut self, thread: &str, prompt: &str) -> Result<Value> {
        let record = self.record();
        Ok(json!({"transport":"app_server","thread_id":thread,
            "dispatch_marker":format!("notion-program/{PID}/round-{:03}",round(&record)),
            "previous_turn_id":null,"source_thread_id":null,"dispatch_prompt_sha256":digest(prompt.trim().as_bytes()),
            "model":"model","sandbox":"workspace-write","approvals_reviewer":null}))
    }
    fn send_dispatch(&mut self, draft: &Value, prompt: &str, _model: &str) -> Result<Value> {
        assert!(["start_sending", "executing"].contains(&phase(&self.record())));
        self.prompts.push(prompt.into());
        if let Some(kind) = self.send_failure.take() {
            if !kind.starts_with("pre_send") {
                self.sent += 1;
            }
            return Err(error("dispatch", &kind, !kind.starts_with("pre_send")));
        }
        self.sent += 1;
        self.observe(draft)
    }
    fn poll_execution(&mut self, receipt: &Value) -> Result<Value> {
        ensure!(
            !self.reject_associated_poll,
            "Conflicting execution history"
        );
        self.observe(receipt)
    }
    fn is_thread_idle(&mut self, thread: &str) -> Result<bool> {
        self.idle_checks.push(thread.to_owned());
        ensure!(!self.idle_failure, "Execution idle check unavailable");
        Ok(self
            .idle_override
            .unwrap_or_else(|| self.status != "in_progress" && self.active_flags.is_empty()))
    }
    fn send_continuation(&mut self, draft: &Value, prompt: &str, _model: &str) -> Result<Value> {
        assert_eq!(phase(&self.record()), "retry_sending");
        self.prompts.push(prompt.into());
        if let Some(kind) = self.continuation_failure.take() {
            if !kind.starts_with("pre_send") && kind != "continuation_changed" {
                self.continued += 1;
            }
            return Err(error("continuation", &kind, kind == "lost_reply"));
        }
        self.continued += 1;
        Ok(draft.clone())
    }
}

struct Harness {
    _directory: tempfile::TempDir,
    store: Rc<RefCell<Store>>,
    scheduler: StateScheduler<FakeNotion, FakeExecutor>,
}

impl Harness {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let config = json!({"data_dir":directory.path().join("data"),
            "task_workspace_root":directory.path().join("tasks"),
            "data_source_id":SOURCE,"executor":"Fedora Codex","model_map":{"test":"model"}});
        let store = Rc::new(RefCell::new(Store {
            properties: json!({"url":format!("https://app.notion.com/p/{}",PID.replace('-',"")),
                "状态":"待办事项","任务名称":"测试","模型":"test","执行人 / 代理":"Fedora Codex",
                "AI 会话 / 线程":"","调度诊断":""}),
            ..Store::default()
        }));
        let executor = FakeExecutor {
            state_path: directory.path().join("data/state.json"),
            created: 0,
            sent: 0,
            continued: 0,
            status: "in_progress".into(),
            queued: false,
            active_flags: vec![],
            send_failure: None,
            continuation_failure: None,
            transient: false,
            problem: None,
            context: None,
            prompts: vec![],
            observations: VecDeque::new(),
            idle_override: None,
            idle_failure: false,
            idle_checks: vec![],
            reject_associated_poll: false,
        };
        let scheduler = StateScheduler::new(
            config,
            FakeNotion(store.clone()),
            executor,
            "manual_test".into(),
        )?;
        Ok(Self {
            _directory: directory,
            store,
            scheduler,
        })
    }

    fn tick(&mut self) -> Result<Value> {
        Ok(self
            .scheduler
            .run_once()?
            .into_iter()
            .next()
            .unwrap_or(Value::Null))
    }

    fn record(&self) -> Value {
        self.scheduler.state[PID].clone()
    }

    fn status(&self, status: &str) {
        self.store.borrow_mut().properties["状态"] = json!(status);
    }

    fn writes(&self, status: &str) -> usize {
        self.store
            .borrow()
            .writes
            .iter()
            .filter(|properties| properties["状态"] == status)
            .count()
    }

    fn restart(&mut self) {
        self.scheduler.state = json!({});
    }

    fn interrupted(&mut self) -> Result<()> {
        let started = self.tick()?;
        self.scheduler.executor.status = "interrupted".into();
        let interrupted = self.tick()?;
        assert_eq!(
            phase(&self.record()),
            "execution_blocked",
            "start outcome: {started}; interrupted outcome: {interrupted}"
        );
        self.status(RETRY_REQUEST);
        Ok(())
    }
}

fn context_receipt() -> Value {
    json!({"contract":"project_context_v1","status":"read","task_page_id":PID,
        "context_page_id":CONTEXT,"version":"revision-7","read_at":"2026-11-04T12:00:00+08:00",
        "topics":[{"page_id":HISTORY,"title":"deployment","applied":"preserved configured paths"}],
        "application":"Used context to preserve runtime data."})
}

#[test]
fn only_todo_starts_rounds_and_unmanaged_routes_are_not_adopted() -> Result<()> {
    for status in ["未开始", "计划中", "进行中", "待审核"] {
        let mut harness = Harness::new()?;
        harness.status(status);
        harness.tick()?;
        assert_eq!(harness.scheduler.executor.created, 0);
        assert_eq!(harness.scheduler.executor.sent, 0);
        assert!(harness.store.borrow().writes.is_empty());
    }
    let mut harness = Harness::new()?;
    harness.store.borrow_mut().properties["AI 会话 / 线程"] =
        json!(format!("codex://threads/{TID}"));
    assert_eq!(harness.tick()?["action"], "existing_unmanaged_task");
    assert_eq!(harness.scheduler.executor.created, 0);
    Ok(())
}

#[test]
fn confirms_running_then_completion_and_quiet_review() -> Result<()> {
    let mut harness = Harness::new()?;
    assert_eq!(harness.tick()?["status"], "executing");
    assert_eq!(harness.store.borrow().properties["状态"], "进行中");
    assert!(harness.record()["running_status_verified_at"].is_string());
    let snapshot = read_json(Path::new(text(&harness.record()["snapshot"])), Value::Null)?;
    assert!(snapshot.get("content").is_none());
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    let writes = harness.store.borrow().writes.len();
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    assert_eq!(harness.store.borrow().writes.len(), writes);
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert_eq!(harness.record()["last_delivered_round"], 1);
    Ok(())
}

#[test]
fn queued_and_lost_send_keep_original_intent_without_resend() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.executor.queued = true;
    assert_eq!(harness.tick()?["status"], "waiting_start");
    let original = harness.record();
    for _ in 0..3 {
        harness.restart();
        assert_eq!(harness.tick()?["status"], "waiting_start");
    }
    assert_eq!(harness.writes("进行中"), 0);
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert_eq!(
        harness.record()["execution_token"],
        original["execution_token"]
    );
    harness.scheduler.executor.queued = false;
    assert_eq!(harness.tick()?["status"], "executing");

    let mut harness = Harness::new()?;
    harness.scheduler.executor.send_failure = Some("lost_reply".into());
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    assert_eq!(phase(&harness.record()), "start_sending");
    harness.restart();
    assert_eq!(harness.tick()?["status"], "executing");
    assert_eq!(harness.scheduler.executor.sent, 1);
    Ok(())
}

#[test]
fn definitely_unsent_reuses_frozen_prompt() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.executor.send_failure = Some("pre_send_read".into());
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    assert_eq!(phase(&harness.record()), "start_prepared");
    let path = PathBuf::from(text(&harness.record()["start_intent_path"]));
    let original = fs::read(&path)?;
    harness.scheduler.config["project_context"] =
        json!({"enabled":true,"helper_command":["new-helper"]});
    harness.restart();
    assert_eq!(harness.tick()?["status"], "executing");
    assert_eq!(fs::read(path)?, original);
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert_eq!(
        harness.scheduler.executor.prompts[0],
        harness.scheduler.executor.prompts[1]
    );
    Ok(())
}

#[test]
fn running_write_rejected_retries_but_unknown_waits_for_readback() -> Result<()> {
    for failure in [
        Failure::Rejected,
        Failure::UnknownBefore,
        Failure::UnknownAfter,
    ] {
        let mut harness = Harness::new()?;
        harness.store.borrow_mut().failure = Some(("进行中".into(), failure));
        assert_eq!(harness.tick()?["status"], "retry_next_cycle");
        harness.restart();
        let status = harness.tick()?["status"].clone();
        match failure {
            Failure::Rejected => {
                assert_eq!(status, "executing");
                assert_eq!(harness.writes("进行中"), 2);
            }
            Failure::UnknownBefore => {
                assert_eq!(status, "waiting_running_status");
                assert_eq!(harness.writes("进行中"), 1);
                harness.status("进行中");
                assert_eq!(harness.tick()?["status"], "executing");
            }
            Failure::UnknownAfter => {
                assert_eq!(status, "executing");
                assert_eq!(harness.writes("进行中"), 1);
            }
        }
        assert_eq!(harness.scheduler.executor.sent, 1);
    }
    Ok(())
}

#[test]
fn fast_completion_and_invalid_receipts_never_publish_fake_running() -> Result<()> {
    for problem in [
        None,
        Some("stale"),
        Some("unuploaded"),
        Some("missing"),
        Some("blocked"),
        Some("unbound"),
    ] {
        let mut harness = Harness::new()?;
        harness.scheduler.executor.status = "completed".into();
        harness.scheduler.executor.problem = problem.map(str::to_owned);
        assert_eq!(
            harness.tick()?["status"],
            if problem.is_none() {
                "awaiting_review"
            } else {
                "execution_blocked"
            }
        );
        assert_eq!(harness.writes("进行中"), 0);
    }
    Ok(())
}

#[test]
fn unknown_final_write_preserves_round_even_when_returned_to_todo() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.executor.status = "completed".into();
    harness.store.borrow_mut().failure = Some(("待审核".into(), Failure::UnknownBefore));
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    let token = harness.record()["execution_token"].clone();
    for _ in 0..3 {
        harness.status("待办事项");
        harness.restart();
        assert_eq!(harness.tick()?["status"], "waiting_status_confirmation");
    }
    assert_eq!(harness.record()["execution_token"], token);
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert_eq!(harness.writes("待审核"), 1);
    harness.status("待审核");
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    Ok(())
}

#[test]
fn receipt_mutation_during_status_recovery_is_rejected() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.executor.status = "completed".into();
    harness.store.borrow_mut().failure = Some(("待审核".into(), Failure::UnknownAfter));
    harness.tick()?;
    let path = PathBuf::from(text(&harness.record()["result_path"]));
    let mut receipt = read_json(&path, Value::Null)?;
    receipt["additional"] = json!("changed");
    write_json(&path, &receipt)?;
    let writes = harness.writes("待审核");
    assert_eq!(harness.tick()?["status"], "needs_attention");
    assert_eq!(harness.writes("待审核"), writes);
    Ok(())
}

#[test]
fn rework_preserves_previous_files_and_reuses_chat() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.executor.status = "completed".into();
    harness.tick()?;
    let record = harness.record();
    let old_path = PathBuf::from(text(&record["result_path"]));
    let old_bytes = fs::read(&old_path)?;
    harness.status("待办事项");
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    assert_eq!(harness.record()["round"], 2);
    assert_eq!(harness.scheduler.executor.created, 1);
    assert_eq!(harness.scheduler.executor.sent, 2);
    assert_eq!(fs::read(old_path)?, old_bytes);
    let backup = harness
        .scheduler
        .data
        .join("previous-rounds")
        .join(PID)
        .join("before-round-002.json");
    assert_eq!(read_json(&backup, Value::Null)?, record);
    assert_eq!(
        harness.record()["round_article_page_ids"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    Ok(())
}

#[test]
fn running_round_returned_to_queue_waits_then_supersedes_old_result() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.tick()?;
    harness.status("待办事项");
    assert_eq!(harness.tick()?["status"], "waiting_previous_execution");
    assert_eq!(harness.scheduler.executor.sent, 1);
    harness.scheduler.executor.status = "interrupted".into();
    assert_eq!(harness.tick()?["status"], "execution_blocked");
    assert_eq!(harness.record()["round"], 2);
    assert_eq!(harness.scheduler.executor.created, 1);
    assert_eq!(harness.scheduler.executor.sent, 2);
    Ok(())
}

#[test]
fn returned_todo_idle_owner_allows_rework_despite_conflicting_terminal_history() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.tick()?;
    let original = harness.record();
    harness.status("待办事项");
    harness.scheduler.executor.idle_override = Some(true);
    harness.scheduler.executor.reject_associated_poll = true;
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    assert_eq!(harness.scheduler.executor.idle_checks, vec![TID]);
    assert_eq!(harness.record()["round"], 2);
    assert_eq!(harness.scheduler.executor.created, 1);
    assert_eq!(harness.scheduler.executor.sent, 2);
    assert_ne!(
        harness.record()["execution_token"],
        original["execution_token"]
    );
    let archive = harness
        .scheduler
        .data
        .join("previous-rounds")
        .join(PID)
        .join("before-round-002.json");
    assert_eq!(read_json(&archive, Value::Null)?, original);
    Ok(())
}

#[test]
fn returned_todo_active_owner_prevents_rework_despite_terminal_history() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.tick()?;
    let original = harness.record();
    harness.status("待办事项");
    harness.scheduler.executor.idle_override = Some(false);
    harness.scheduler.executor.reject_associated_poll = true;
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "waiting_previous_execution");
    assert_eq!(harness.scheduler.executor.idle_checks, vec![TID]);
    assert_eq!(harness.record(), original);
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert_eq!(harness.store.borrow().properties["状态"], "待办事项");
    Ok(())
}

#[test]
fn returned_todo_unavailable_idle_check_does_not_authorize_rework() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.tick()?;
    harness.status("待办事项");
    harness.scheduler.executor.idle_failure = true;
    assert_eq!(harness.tick()?["status"], "needs_attention");
    assert_eq!(harness.record()["round"], 1);
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert_eq!(harness.store.borrow().properties["状态"], "待办事项");
    Ok(())
}

#[test]
fn known_blocked_round_is_polled_outside_discovery_and_can_restore_running() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    harness.status("阻塞中");
    harness.store.borrow_mut().empty_discovery = true;
    harness.scheduler.executor.status = "in_progress".into();
    assert_eq!(harness.tick()?["status"], "executing");
    assert_eq!(phase(&harness.record()), "waiting_execution");
    assert_eq!(harness.store.borrow().properties["状态"], "进行中");
    assert_eq!(harness.scheduler.executor.sent, 1);
    Ok(())
}

#[test]
fn unknown_restoration_write_does_not_replay_or_create_rework() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    harness.status("阻塞中");
    harness.scheduler.executor.status = "in_progress".into();
    harness.store.borrow_mut().failure = Some(("进行中".into(), Failure::UnknownBefore));
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    harness.status("待办事项");
    harness.restart();
    assert_eq!(harness.tick()?["status"], "waiting_status_confirmation");
    assert_eq!(harness.record()["round"], 1);
    assert_eq!(harness.scheduler.executor.sent, 1);
    Ok(())
}

#[test]
fn manual_retry_keeps_round_and_records_parent_then_repeated_click_only_observes() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    let original = harness.record();
    harness.scheduler.executor.status = "in_progress".into();
    harness.scheduler.executor.observations.push_back(json!({
        "status":"interrupted","thread_status":"idle","active_flags":[],
        "dispatched_turn_id":"turn-1","turn":{"id":"turn-1","status":"interrupted"}
    }));
    assert_eq!(harness.tick()?["status"], "retry_started");
    for field in [
        "round",
        "thread_id",
        "execution_token",
        "snapshot",
        "result_path",
    ] {
        assert_eq!(harness.record()[field], original[field]);
    }
    assert_eq!(harness.scheduler.executor.continued, 1);
    let original_receipt = fs::read(text(&original["execution_path"]))?;
    harness.status(RETRY_REQUEST);
    assert_eq!(harness.tick()?["status"], "already_running");
    assert_eq!(harness.scheduler.executor.continued, 1);
    assert_eq!(
        fs::read(text(&original["execution_path"]))?,
        original_receipt
    );
    Ok(())
}

#[test]
fn approvals_inputs_unknown_flags_and_worker_block_prevent_retry() -> Result<()> {
    for (flag, expected) in [
        ("waitingOnApproval", "retry_approval"),
        ("waitingOnUserInput", "retry_input"),
        ("unknownFlag", "retry_unknown"),
    ] {
        let mut harness = Harness::new()?;
        harness.interrupted()?;
        harness.scheduler.executor.active_flags = vec![flag.into()];
        assert_eq!(harness.tick()?["status"], expected);
        assert_eq!(harness.scheduler.executor.continued, 0);
        assert_eq!(harness.record()["round"], 1);
    }
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    write_json(
        Path::new(text(&harness.record()["result_path"])),
        &json!({"status":"blocked"}),
    )?;
    assert_eq!(harness.tick()?["status"], "retry_ineligible");
    assert_eq!(harness.scheduler.executor.continued, 0);
    Ok(())
}

#[test]
fn uncertain_retry_send_is_observed_after_restart_without_second_send() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    harness.scheduler.executor.continuation_failure = Some("lost_reply".into());
    assert_eq!(harness.tick()?["status"], "retry_send_unknown");
    assert_eq!(phase(&harness.record()), "retry_sending");
    harness.restart();
    harness.scheduler.executor.status = "in_progress".into();
    assert_eq!(harness.tick()?["status"], "retry_started");
    assert_eq!(harness.scheduler.executor.continued, 1);
    Ok(())
}

#[test]
fn unknown_retry_status_write_does_not_repeat_when_old_value_remains() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    harness.scheduler.executor.observations.push_back(json!({
        "status":"interrupted","thread_status":"idle","active_flags":[],
        "dispatched_turn_id":"turn-1","turn":{"id":"turn-1","status":"interrupted"}
    }));
    harness.scheduler.executor.status = "in_progress".into();
    harness.store.borrow_mut().failure = Some(("进行中".into(), Failure::UnknownBefore));
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    let writes = harness.writes("进行中");
    harness.restart();
    assert_eq!(harness.tick()?["status"], "waiting_status_confirmation");
    assert_eq!(harness.writes("进行中"), writes);
    assert_eq!(harness.scheduler.executor.continued, 1);
    harness.status("进行中");
    assert_eq!(harness.tick()?["status"], "retry_started");
    Ok(())
}

#[test]
fn retry_presend_failure_reuses_intent_and_changed_state_returns_specific_feedback() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.interrupted()?;
    harness.scheduler.executor.continuation_failure = Some("pre_send_read".into());
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    let intent = harness.record()["retry"].clone();
    assert_eq!(phase(&harness.record()), "retry_prepared");
    harness.scheduler.executor.continuation_failure = Some("continuation_changed".into());
    harness.scheduler.executor.active_flags = vec!["waitingOnApproval".into()];
    assert_eq!(harness.tick()?["status"], "retry_approval");
    assert_eq!(harness.scheduler.executor.continued, 0);
    assert_eq!(harness.record()["retry"]["id"], intent["id"]);
    assert_eq!(harness.store.borrow().properties["状态"], "续试需审批");
    Ok(())
}

#[test]
fn automatic_budget_backoff_and_exhaustion_survive_restart_and_manual_clicks() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.config["automatic_retry"] =
        json!({"enabled":true,"max_attempts":3,"delays_seconds":[60,180,300]});
    harness.tick()?;
    harness.scheduler.executor.status = "failed".into();
    harness.scheduler.executor.transient = true;
    harness.tick()?;
    assert_eq!(harness.scheduler.executor.continued, 0);
    harness.status(RETRY_REQUEST);
    harness.tick()?;
    assert_eq!(harness.scheduler.executor.continued, 0);
    for index in 1..=3 {
        let mut record = harness.record();
        record["automatic_recovery"]["next_attempt_at"] = json!(0);
        harness.scheduler.save(PID, &record)?;
        harness.restart();
        harness.tick()?;
        assert_eq!(harness.scheduler.executor.continued, index);
    }
    assert_eq!(harness.record()["automatic_recovery"]["state"], "exhausted");
    assert_eq!(
        harness.record()["automatic_recovery"]["notice"]["state"],
        "unconfigured"
    );
    harness.tick()?;
    assert_eq!(harness.scheduler.executor.continued, 3);
    assert_eq!(harness.record()["round"], 1);
    assert_eq!(harness.store.borrow().properties["状态"], "阻塞中");
    Ok(())
}

#[test]
fn automatic_unsent_cancellation_releases_reservation() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.config["automatic_retry"] = json!({"enabled":true});
    harness.tick()?;
    harness.scheduler.executor.status = "failed".into();
    harness.scheduler.executor.transient = true;
    harness.tick()?;
    let mut record = harness.record();
    record["automatic_recovery"]["next_attempt_at"] = json!(0);
    harness.scheduler.save(PID, &record)?;
    harness.scheduler.executor.continuation_failure = Some("pre_send_read".into());
    harness.tick()?;
    assert_eq!(phase(&harness.record()), "retry_prepared");
    harness.status("待审核");
    assert_eq!(harness.tick()?["status"], "retry_cancelled");
    assert_eq!(
        harness.record()["automatic_recovery"]["attempts"][0]["counts_toward_limit"],
        false
    );
    assert_eq!(harness.scheduler.executor.continued, 0);
    Ok(())
}

#[test]
fn automatic_signal_accepts_only_structured_transient_settled_operations() {
    let current = json!({"thread":{"status":{"type":"idle","activeFlags":[]}},"turns":[]});
    let turn = json!({"id":"turn","status":"failed","error":{"codexErrorInfo":{"httpConnectionFailed":{"httpStatusCode":502}}},"items":[]});
    assert_eq!(retry_signal(&current, &turn, None)["eligible"], true);
    for items in [
        json!([{"type":"mcpToolCall","status":"completed"}]),
        json!([{"type":"commandExecution","status":"completed","exitCode":null}]),
        json!([{"type":"functionCallOutput","output":{"truncated":true}}]),
    ] {
        let mut turn = turn.clone();
        turn["items"] = items;
        assert_eq!(retry_signal(&current, &turn, None)["eligible"], false);
    }
    for code in [
        json!("internalServerError"),
        json!({"httpConnectionFailed":{"httpStatusCode":401}}),
    ] {
        let mut turn = turn.clone();
        turn["error"]["codexErrorInfo"] = code;
        assert_eq!(retry_signal(&current, &turn, None)["eligible"], false);
    }
    let interrupted =
        json!({"id":"turn","status":"interrupted","startedAt":1000,"completedAt":1600,"items":[]});
    let receipt = json!({"local_timeout":{"source":"owned_app_server_wait","turn_id":"turn","seconds":600,"observed_at":1600}});
    assert_eq!(
        retry_signal(&current, &interrupted, None)["eligible"],
        false
    );
    assert_eq!(
        retry_signal(&current, &interrupted, Some(&receipt))["eligible"],
        true
    );
}

#[test]
fn context_validation_rejects_bad_ownership_topics_and_forged_unassigned() -> Result<()> {
    let record = json!({"project_context_contract":"project_context_v1"});
    let mut result = json!({"status":"completed","project_context":context_receipt()});
    validate_project_context_receipt(PID, &record, &result)?;
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
        let mut bad = result.clone();
        bad["project_context"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(validate_project_context_receipt(PID, &record, &bad).is_err());
    }
    result["project_context"]["task_page_id"] = json!(SOURCE);
    assert!(validate_project_context_receipt(PID, &record, &result).is_err());
    result["project_context"] = context_receipt();
    result["project_context"]["topics"]
        .as_array_mut()
        .unwrap()
        .push(context_receipt()["topics"][0].clone());
    assert!(validate_project_context_receipt(PID, &record, &result).is_err());
    result["project_context"] = json!({"contract":"project_context_v1","status":"unassigned",
        "task_page_id":PID,"context_page_id":null,"version":null,"read_at":"actual time","topics":[],"application":"","reason":"helper confirms no project"});
    validate_project_context_receipt(PID, &record, &result)?;
    result["project_context"]["context_page_id"] = json!(CONTEXT);
    assert!(validate_project_context_receipt(PID, &record, &result).is_err());
    validate_project_context_receipt(PID, &json!({}), &result)?;
    Ok(())
}

#[test]
fn context_is_frozen_and_failure_blocks_even_after_configuration_disabled() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.config["project_context"] =
        json!({"enabled":true,"helper_command":["/missing helper","--config","a 'quoted' path"]});
    harness.tick()?;
    let original = harness.record();
    let prompt = harness.scheduler.worker_prompt(PID, &original)?;
    assert!(prompt.contains("context --task"));
    assert!(prompt.contains("'\"'\"'"));
    harness.scheduler.config["project_context"]["enabled"] = json!(false);
    assert_eq!(harness.scheduler.worker_prompt(PID, &original)?, prompt);
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "execution_blocked");
    harness.scheduler.executor.context = Some(context_receipt());
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    assert_eq!(
        harness.record()["execution_token"],
        original["execution_token"]
    );
    assert_eq!(harness.scheduler.executor.sent, 1);
    assert!(
        !harness
            .scheduler
            .round_path(&original, "data", "project-context.json")?
            .exists()
    );
    Ok(())
}

#[test]
fn malformed_context_config_leaves_no_round_files() -> Result<()> {
    let mut harness = Harness::new()?;
    for helper in [
        json!([]),
        json!("shell command"),
        json!(["bad\0argument"]),
        json!([false]),
    ] {
        harness.scheduler.config["project_context"] =
            json!({"enabled":true,"helper_command":helper});
        let task = harness.scheduler.metadata(PID, None)?;
        assert!(harness.scheduler.prepare(PID, &json!({}), &task).is_err());
        assert!(
            !config_path(&harness.scheduler.config, "task_workspace_root")?
                .join(PID)
                .exists()
        );
    }
    Ok(())
}

#[test]
fn feedback_deduplicates_preserves_human_edits_and_reconciles_unknown_writes() -> Result<()> {
    for failure in [
        None,
        Some(Failure::UnknownBefore),
        Some(Failure::UnknownAfter),
    ] {
        verify_feedback_reconciliation(failure)?;
    }
    Ok(())
}

fn verify_feedback_reconciliation(failure: Option<Failure>) -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.config["diagnostic_feedback_property"] = json!("调度诊断");
    harness.tick()?;
    if let Some(failure) = failure {
        harness.store.borrow_mut().failure = Some(("feedback".into(), failure));
    }
    let mut record = harness.record();
    harness
        .scheduler
        .diagnostic(PID, &mut record, "interrupted", &[], None);
    let writes = feedback_writes(&harness);
    harness
        .scheduler
        .diagnostic(PID, &mut record, "interrupted", &[], None);
    assert_eq!(feedback_writes(&harness), writes);
    assert_eq!(harness.scheduler.executor.sent, 1);
    if !matches!(failure, Some(Failure::UnknownBefore)) {
        assert_eq!(
            record["diagnostic_feedback_publication"]["state"],
            "verified"
        );
    }
    harness.store.borrow_mut().properties["调度诊断"] = json!("Herman wrote instructions");
    harness
        .scheduler
        .diagnostic(PID, &mut record, "resolved", &[], None);
    assert_eq!(
        harness.store.borrow().properties["调度诊断"],
        "Herman wrote instructions"
    );
    Ok(())
}

fn feedback_writes(harness: &Harness) -> usize {
    harness
        .store
        .borrow()
        .writes
        .iter()
        .filter(|value| value.get("调度诊断").is_some())
        .count()
}

#[test]
fn feedback_is_bounded_and_never_echoes_raw_receipt_reason() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.config["diagnostic_feedback_property"] = json!("调度诊断");
    harness.scheduler.executor.status = "completed".into();
    harness.scheduler.executor.problem = Some("blocked".into());
    harness.tick()?;
    let feedback = harness.store.borrow().properties["调度诊断"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(feedback.contains("权限或审批"));
    assert!(!feedback.contains("PRIVATE_REASON"));
    assert!(feedback.chars().count() <= 1900);
    let mut record = harness.record();
    record["automatic_recovery"] = json!({"maximum":3,"attempts":(0..100).map(|number| json!({
        "number":number,"counts_toward_limit":false,"result":"not_sent_state_changed",
        "error_code":"responseStreamDisconnected","prepared_at":"2026-11-04T12:00:00+08:00"
    })).collect::<Vec<_>>()});
    assert!(
        feedback::render_feedback(PID, &record, "automatic_retry_exhausted", &[], None)
            .chars()
            .count()
            < 1900
    );
    Ok(())
}

#[test]
fn scope_changes_and_terminal_states_do_not_mutate_or_dispatch() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.tick()?;
    harness.store.borrow_mut().properties["执行人 / 代理"] = json!("Herman");
    let writes = harness.store.borrow().writes.len();
    assert_eq!(harness.tick()?["status"], "needs_attention");
    assert_eq!(harness.store.borrow().writes.len(), writes);
    assert_eq!(harness.scheduler.executor.sent, 1);
    let mut harness = Harness::new()?;
    harness.tick()?;
    harness.status("已完成待归档");
    let writes = harness.store.borrow().writes.len();
    assert_eq!(harness.tick()?["status"], "已完成待归档");
    assert_eq!(harness.store.borrow().writes.len(), writes);
    assert_eq!(phase(&harness.record()), "terminal");
    Ok(())
}

#[test]
fn lock_prevents_duplicate_dispatch_and_unknown_state_fields_survive() -> Result<()> {
    let mut harness = Harness::new()?;
    let guard = lock(&harness.scheduler.data.join("scheduler.lock"))?.map(SchedulerLock);
    assert_eq!(harness.tick()?["action"], "already_running");
    assert_eq!(harness.scheduler.executor.sent, 0);
    drop(guard);
    harness.tick()?;
    let mut record = harness.record();
    record["future_extension"] = json!({"preserve":"yes"});
    harness.scheduler.save(PID, &record)?;
    harness.restart();
    harness.tick()?;
    assert_eq!(
        harness.record()["future_extension"],
        record["future_extension"]
    );
    Ok(())
}

#[test]
fn scheduler_lock_releases_before_inherited_descriptor_closes() -> Result<()> {
    for return_error in [false, true] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("scheduler.lock");
        let mut inherited = None;
        let outcome = (|| -> Result<()> {
            let guard = SchedulerLock(lock(&path)?.context("Initial scheduler lock unavailable")?);
            inherited = Some(guard.0.try_clone()?);
            assert!(lock(&path)?.is_none());
            ensure!(!return_error, "Injected cycle failure");
            Ok(())
        })();
        assert_eq!(outcome.is_err(), return_error);
        let replacement = lock(&path)?.map(SchedulerLock);
        assert!(
            replacement.is_some(),
            "Finished cycle still holds its lock through the inherited descriptor"
        );
        drop(inherited);
        assert!(lock(&path)?.is_none());
        drop(replacement);
        assert!(lock(&path)?.is_some());
    }
    Ok(())
}

#[test]
fn legacy_claimed_checkpoint_dispatches_without_reclaiming_or_reading_bodies() -> Result<()> {
    let mut harness = Harness::new()?;
    let task = harness.scheduler.metadata(PID, None)?;
    let mut record = harness.scheduler.prepare(PID, &json!({}), &task)?;
    remove(&mut record, "dispatch_protocol");
    record["thread_id"] = json!(TID);
    harness.status("进行中");
    harness.store.borrow_mut().properties["AI 会话 / 线程"] =
        json!(format!("codex://threads/{TID}"));
    harness
        .scheduler
        .checkpoint(PID, &mut record, "claimed", json!({}))?;
    assert_eq!(harness.tick()?["status"], "executing");
    assert_eq!(harness.writes("进行中"), 0);
    assert_eq!(harness.scheduler.executor.created, 0);
    assert_eq!(harness.scheduler.executor.sent, 1);
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    Ok(())
}

#[test]
fn legacy_history_contract_remains_v1_until_next_round() -> Result<()> {
    let mut harness = Harness::new()?;
    let task = harness.scheduler.metadata(PID, None)?;
    let mut record = harness.scheduler.prepare(PID, &json!({}), &task)?;
    record["history_contract"] = json!("per_task_history_page_v1");
    harness.scheduler.save(PID, &record)?;
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    assert_eq!(
        harness.record()["history_contract"],
        "per_task_history_page_v1"
    );
    assert!(harness.record().get("round_article_page_ids").is_none());
    harness.status("待办事项");
    assert_eq!(harness.tick()?["status"], "awaiting_review");
    assert_eq!(harness.record()["history_contract"], ARTICLE_CONTRACT);
    assert_eq!(harness.record()["history_page_id"], HISTORY);
    Ok(())
}

#[test]
fn historical_body_delivery_and_old_workflow_are_not_silently_submitted() -> Result<()> {
    for interrupted in [
        "result_ready",
        "report_appended",
        "notice_ready",
        "notice_appended",
        "executing",
    ] {
        let mut harness = Harness::new()?;
        harness.tick()?;
        let mut record = harness.record();
        record["phase"] = json!(interrupted);
        harness.scheduler.save(PID, &record)?;
        assert_eq!(harness.tick()?["status"], "needs_attention");
        assert_eq!(harness.scheduler.executor.sent, 1);
        assert_eq!(harness.writes("待审核"), 0);
    }
    let mut harness = Harness::new()?;
    harness.tick()?;
    let mut record = harness.record();
    remove(&mut record, "workflow");
    harness.scheduler.save(PID, &record)?;
    harness.scheduler.executor.status = "completed".into();
    assert_eq!(harness.tick()?["status"], "execution_blocked");
    assert_eq!(harness.writes("待审核"), 0);
    Ok(())
}

#[test]
fn unknown_chat_binding_reconciles_without_creating_a_second_chat() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.store.borrow_mut().failure = Some(("route".into(), Failure::UnknownAfter));
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    assert_eq!(phase(&harness.record()), "linking_chat");
    assert_eq!(harness.scheduler.executor.sent, 0);
    harness.restart();
    assert_eq!(harness.tick()?["status"], "executing");
    assert_eq!(harness.scheduler.executor.created, 1);
    assert_eq!(harness.scheduler.executor.sent, 1);
    Ok(())
}

#[test]
fn changed_metadata_after_reservation_cancels_send_and_retains_checkpoint() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.executor.send_failure = Some("pre_send_read".into());
    harness.tick()?;
    let intent_path = PathBuf::from(text(&harness.record()["start_intent_path"]));
    let intent = fs::read(&intent_path)?;
    harness.status("计划中");
    assert_eq!(harness.tick()?["status"], "start_state_changed");
    harness.status("待办事项");
    harness.store.borrow_mut().properties["AI 会话 / 线程"] =
        json!(format!("codex://threads/{HISTORY}"));
    assert_eq!(harness.tick()?["status"], "retry_next_cycle");
    assert_eq!(phase(&harness.record()), "start_prepared");
    assert_eq!(harness.scheduler.executor.sent, 0);
    assert_eq!(fs::read(intent_path)?, intent);
    Ok(())
}

#[test]
fn corrected_model_mapping_does_not_reset_unreconciled_checkpoint() -> Result<()> {
    let mut harness = Harness::new()?;
    harness.scheduler.config["model_map"] = json!({});
    assert_eq!(harness.tick()?["status"], "needs_attention");
    assert_eq!(harness.record()["error"], MODEL_ERROR);
    harness.scheduler.config["model_map"] = json!({"test":"model"});
    assert_eq!(harness.tick()?["status"], "needs_attention");
    assert_eq!(harness.scheduler.executor.sent, 0);
    Ok(())
}

#[test]
fn invalid_active_flags_never_authorize_retry() -> Result<()> {
    for active_flags in [json!(null), json!([{}]), json!({"waitingOnApproval":true})] {
        let mut harness = Harness::new()?;
        harness.interrupted()?;
        harness.scheduler.executor.observations.push_back(json!({
            "status":"interrupted","thread_status":"idle","active_flags":active_flags,
            "dispatched_turn_id":"turn-1","turn":{"id":"turn-1","status":"interrupted"}
        }));
        assert_eq!(harness.tick()?["status"], "retry_unknown");
        assert_eq!(harness.scheduler.executor.continued, 0);
    }
    Ok(())
}
