use super::notion::unescape;
use super::rpc::RpcFailure;
use super::{Executor, ExternalError, RpcClient, external, required, truthy};
use crate::common::digest;
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

fn marker_expression() -> Regex {
    Regex::new(r"notion-program/[0-9a-f-]+/round-[0-9]+").unwrap()
}

fn frozen(model: &str, sandbox: &str, reviewer: &str) -> Value {
    let optional = |text: &str| {
        if text.is_empty() {
            Value::Null
        } else {
            json!(text)
        }
    };
    json!({
        "model": optional(model),
        "sandbox": optional(sandbox),
        "approvals_reviewer": optional(reviewer)
    })
}

fn settings(value: &Value) -> Result<Value> {
    let mut result = Map::new();
    for field in ["model", "sandbox", "approvals_reviewer"] {
        let value = value
            .get(field)
            .filter(|value| value.is_null() || value.is_string())
            .ok_or_else(|| external("dispatch", "pre_send_settings", false, false))?;
        result.insert(field.to_owned(), value.clone());
    }
    Ok(Value::Object(result))
}

fn merge(original: &Value, added: &Value) -> Value {
    let mut result = original.as_object().cloned().unwrap_or_default();
    if let Some(added) = added.as_object() {
        result.extend(
            added
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    Value::Object(result)
}

fn bound_outcome(draft: &Value, outcome: &Value) -> Value {
    let mut result = merge(outcome, draft);
    result["dispatched_turn_id"] = if outcome["start_response_received"] == false {
        Value::Null
    } else {
        outcome["turn"]["id"].clone()
    };
    result
}

fn idle(current: &Value) -> bool {
    matches!(
        current["thread"]["status"]["type"].as_str(),
        Some("idle" | "notLoaded")
    ) && !truthy(&current["thread"]["status"]["activeFlags"])
        && current["turns"]
            .as_array()
            .is_none_or(|turns| turns.iter().all(|turn| turn["status"] != "inProgress"))
}

fn turn_summary(turn: &Value) -> Value {
    json!({
        "id": turn["id"], "status": turn["status"],
        "startedAt": turn["startedAt"], "completedAt": turn["completedAt"]
    })
}

pub struct RoutingExecutor {
    config: Value,
    primary: RpcClient,
    desktop: Option<RpcClient>,
    routes: HashMap<String, Value>,
    thread_settings: HashMap<String, Value>,
    native_clients: HashMap<String, RpcClient>,
    native_observers: HashMap<String, RpcClient>,
}

impl RoutingExecutor {
    pub fn new(config: Value) -> Result<Self> {
        let primary = RpcClient::from_config(&config)?;
        if let Some(pages) = config.get("execution_history_pages") {
            ensure!(
                pages.as_u64().is_some_and(|pages| pages > 0),
                "Invalid history page bound"
            );
        }
        Ok(Self {
            config,
            primary,
            desktop: None,
            routes: HashMap::new(),
            thread_settings: HashMap::new(),
            native_clients: HashMap::new(),
            native_observers: HashMap::new(),
        })
    }

    fn persistent(&self) -> bool {
        matches!(
            self.config["execution_transport"].as_str(),
            Some("persistent_proxy" | "persistent_daemon")
        )
    }

    fn native(&mut self, thread: &str) -> Result<&mut RpcClient> {
        if !self.persistent() {
            return Ok(&mut self.primary);
        }
        if !self.native_clients.contains_key(thread) {
            let client = RpcClient::from_config(&self.config)?;
            self.native_clients.insert(thread.to_owned(), client);
        }
        Ok(self.native_clients.get_mut(thread).unwrap())
    }

    fn desktop_tool(&mut self, name: &str, arguments: &Value) -> Result<Value> {
        let caller = required(&self.config, "desktop_caller_thread_id")?.to_owned();
        if self.desktop.is_none() {
            self.desktop = Some(RpcClient::desktop(&self.config)?);
        }
        let write = name != "read_thread";
        let result = self.desktop.as_mut().unwrap().request(
            "tools/call",
            &json!({
                "name": name, "arguments": arguments,
                "_meta": {"openai/threadId": caller}
            }),
        );
        let raw = result.map_err(|error| {
            let detail = error.downcast_ref::<ExternalError>();
            external(
                name,
                detail.map_or("desktop_tool_request", |error| error.kind.as_str()),
                write,
                detail.is_some_and(|error| error.reconnectable),
            )
        })?;
        if truthy(&raw["isError"]) {
            return Err(external(name, "desktop_tool_error", write, !write));
        }
        let parts = raw["content"]
            .as_array()
            .filter(|parts| !parts.is_empty())
            .ok_or_else(|| external(name, "unsupported_desktop_response", write, false))?;
        let mut text = String::new();
        for part in parts {
            if part["type"] != "text" {
                return Err(external(name, "unsupported_desktop_response", write, false));
            }
            text.push_str(
                part["text"]
                    .as_str()
                    .ok_or_else(|| external(name, "unsupported_desktop_response", write, false))?,
            );
        }
        match serde_json::from_str(&text) {
            Ok(result) => Ok(result),
            Err(_) if name == "send_message_to_thread" => Ok(json!({"text": text})),
            Err(_) => Err(external(name, "invalid_desktop_json", write, false)),
        }
    }

    fn read_native(&mut self, thread: &str, messages: bool, cursor: Option<&str>) -> Result<Value> {
        if !self.native_observers.contains_key(thread) {
            let client = RpcClient::from_config(&self.config)?;
            self.native_observers.insert(thread.to_owned(), client);
        }
        let client = self.native_observers.get_mut(thread).unwrap();
        let result = (|| -> Result<Value> {
            let response = client.request(
                "thread/read",
                &json!({
                    "threadId": thread, "includeTurns": false
                }),
            )?;
            let owner = &response["thread"];
            if owner["id"] != thread
                || !owner["status"].is_object()
                || !matches!(
                    owner["status"]["type"].as_str(),
                    Some("active" | "idle" | "notLoaded")
                )
                || owner["status"]
                    .get("activeFlags")
                    .is_some_and(|flags| !flags.is_array())
            {
                return Err(external("thread/read", "invalid_owner_state", false, false));
            }
            let mut arguments = json!({
                "threadId": thread, "limit": if messages { 5 } else { 2 },
                "sortDirection": "desc", "itemsView": if messages { "full" } else { "summary" }
            });
            if let Some(cursor) = cursor {
                arguments["cursor"] = json!(cursor);
            }
            let history = client.request("thread/turns/list", &arguments)?;
            let turns = history["data"].as_array().ok_or_else(|| {
                external("thread/turns/list", "invalid_owner_history", false, false)
            })?;
            let next = &history["nextCursor"];
            if !next.is_null() && !next.is_string()
                || turns.iter().any(|turn| {
                    !turn.is_object()
                        || turn["id"].as_str().is_none_or(str::is_empty)
                        || !turn["items"].is_array()
                        || messages && turn.get("itemsView").is_some_and(|view| view != "full")
                })
            {
                return Err(external(
                    "thread/turns/list",
                    "invalid_owner_history",
                    false,
                    false,
                ));
            }
            Ok(json!({
                "thread": owner, "turns": turns, "observation_source": "app_server",
                "page": {"hasMore": !next.is_null(), "nextCursor": next}
            }))
        })();
        result.map_err(|error| {
            let detail = error.downcast_ref::<ExternalError>();
            external(
                "read",
                "desktop_read",
                false,
                detail.is_some_and(|error| error.reconnectable),
            )
        })
    }

    fn read(
        &mut self,
        thread: &str,
        messages: bool,
        cursor: Option<&str>,
        transport: Option<&str>,
    ) -> Result<Value> {
        if transport == Some("app_server") && self.persistent() {
            return self.read_native(thread, messages, cursor);
        }
        let mut arguments = json!({
            "threadId": thread, "hostId": "local", "turnLimit": if messages { 5 } else { 2 },
            "includeOutputs": messages, "maxOutputCharsPerItem": if messages { 16000 } else { 0 }
        });
        if let Some(cursor) = cursor {
            arguments["cursor"] = json!(cursor);
        }
        let result = self
            .desktop_tool("read_thread", &arguments)
            .map_err(|error| {
                let reconnectable = error
                    .downcast_ref::<ExternalError>()
                    .is_some_and(|error| error.reconnectable);
                external("read", "desktop_read", false, reconnectable)
            })?;
        if result["thread"]["id"] != thread
            || !result["thread"]["status"].is_object()
            || !result["turns"].is_array()
            || result["turns"]
                .as_array()
                .unwrap()
                .iter()
                .any(|turn| !turn.is_object() || turn["id"].as_str().is_none_or(str::is_empty))
        {
            return Err(external("read", "desktop_read", false, false));
        }
        Ok(result)
    }

    pub fn is_thread_idle(&mut self, thread: &str) -> Result<bool> {
        if self.persistent() && !idle(&self.read_native(thread, false, None)?) {
            return Ok(false);
        }
        Ok(idle(&self.read(thread, false, None, None)?))
    }

    fn native_turn(&mut self, thread: &str, prompt: &str) -> Result<Value> {
        if self.persistent() {
            let direct_socket = self.config["daemon_socket"].as_str().is_some();
            let arguments: Vec<_> = self.config["codex_command"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let bridge = arguments.iter().any(|argument| {
                std::path::Path::new(argument)
                    .file_name()
                    .is_some_and(|name| name == "daemon_bridge.py")
            });
            ensure!(
                (direct_socket || bridge || arguments.contains(&"proxy"))
                    && !arguments.contains(&"--stdio"),
                "Nonblocking execution requires a persistent daemon"
            );
            self.native(thread)?.start_turn(thread, prompt)
        } else {
            let timeout = self.config["turn_timeout"].as_f64().unwrap_or(600.0);
            let mut result = self.native(thread)?.run_turn(thread, prompt, timeout)?;
            if result["status"] == "timeout" && result["turn"]["id"].is_string() {
                let observed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs_f64();
                result["local_timeout"] = json!({
                    "source": "owned_app_server_wait", "seconds": timeout,
                    "observed_at": observed, "turn_id": result["turn"]["id"]
                });
            }
            Ok(result)
        }
    }

    fn resume_frozen(&mut self, thread: &str, frozen: &Value) -> Result<()> {
        let resumed = self
            .native(thread)?
            .resume_thread(
                thread,
                frozen["model"].as_str().unwrap_or(""),
                frozen["sandbox"].as_str().unwrap_or(""),
                frozen["approvals_reviewer"].as_str().unwrap_or(""),
            )
            .map_err(|_| external("dispatch", "pre_send_resume", false, true))?;
        if resumed["id"] != thread {
            return Err(external("dispatch", "pre_send_wrong_thread", false, false));
        }
        self.thread_settings
            .insert(thread.to_owned(), frozen.clone());
        Ok(())
    }
}

impl Executor for RoutingExecutor {
    fn is_thread_idle(&mut self, thread: &str) -> Result<bool> {
        RoutingExecutor::is_thread_idle(self, thread)
    }

    fn start_thread(
        &mut self,
        model: &str,
        cwd: &str,
        title: &str,
        sandbox: &str,
        reviewer: &str,
    ) -> Result<Value> {
        let thread = if self.persistent() {
            let mut client = RpcClient::from_config(&self.config)?;
            let thread = client.start_thread(model, cwd, title, sandbox, reviewer)?;
            let identifier = required(&thread, "id")?.to_owned();
            self.native_clients.insert(identifier, client);
            thread
        } else {
            self.primary
                .start_thread(model, cwd, title, sandbox, reviewer)?
        };
        self.thread_settings.insert(
            required(&thread, "id")?.to_owned(),
            frozen(model, sandbox, reviewer),
        );
        Ok(thread)
    }

    fn resume_thread(
        &mut self,
        thread: &str,
        model: &str,
        sandbox: &str,
        reviewer: &str,
    ) -> Result<Value> {
        let frozen = frozen(model, sandbox, reviewer);
        let result = self
            .native(thread)?
            .resume_thread(thread, model, sandbox, reviewer);
        match result {
            Ok(result) => {
                self.thread_settings.insert(thread.to_owned(), frozen);
                self.routes.remove(thread);
                return Ok(result);
            }
            Err(error) => {
                let active_writer = error.downcast_ref::<RpcFailure>().is_some_and(|failure| {
                    failure.response["error"]["code"] == -32600
                        && failure.response["error"]["message"]
                            .as_str()
                            .is_some_and(|text| text.contains("already has an active writer"))
                });
                if !active_writer || !truthy(&self.config["desktop_tools_command"]) {
                    return Err(error);
                }
            }
        }
        let current = self.read(thread, false, None, None)?;
        if current["thread"]["status"]["type"] != "idle" || !idle(&current) {
            return Err(external("resume", "pre_send_busy", false, false));
        }
        let previous = current["turns"][0]["id"].clone();
        let route = merge(&frozen, &json!({"previous_turn_id": previous}));
        self.routes.insert(thread.to_owned(), route);
        self.thread_settings.insert(thread.to_owned(), frozen);
        Ok(current["thread"].clone())
    }

    fn prepare_dispatch(&mut self, thread: &str, prompt: &str) -> Result<Value> {
        ensure!(!thread.is_empty(), "Thread cannot be empty");
        let marker = marker_expression()
            .find(prompt)
            .context("Missing unique dispatch marker")?
            .as_str()
            .to_owned();
        let route = self.routes.get(thread);
        let frozen =
            self.thread_settings.get(thread).cloned().unwrap_or_else(
                || json!({"model": null, "sandbox": null, "approvals_reviewer": null}),
            );
        Ok(merge(
            &frozen,
            &json!({
                "transport": if route.is_some() { "desktop_tools" } else { "app_server" },
                "thread_id": thread, "previous_turn_id": route.map(|route| &route["previous_turn_id"]),
                "dispatch_marker": marker, "source_thread_id": self.config["desktop_caller_thread_id"],
                "dispatch_prompt_sha256": digest(prompt.trim().as_bytes())
            }),
        ))
    }

    fn send_dispatch(&mut self, draft: &Value, prompt: &str, model: &str) -> Result<Value> {
        let thread = required(draft, "thread_id")?.to_owned();
        if draft.get("send_receipt").is_some() || truthy(&draft["dispatched_turn_id"]) {
            return Err(external("dispatch", "already_sent", true, false));
        }
        let expected = self.prepare_dispatch(&thread, prompt)?;
        let valid_transport = matches!(
            draft["transport"].as_str(),
            Some("app_server" | "desktop_tools")
        );
        if !valid_transport
            || [
                "thread_id",
                "dispatch_marker",
                "source_thread_id",
                "dispatch_prompt_sha256",
            ]
            .iter()
            .any(|key| draft[*key] != expected[*key])
            || draft.get("previous_turn_id").is_none()
        {
            return Err(external(
                "dispatch",
                "pre_send_intent_changed",
                false,
                false,
            ));
        }
        let frozen = settings(draft)?;
        if self
            .thread_settings
            .get(&thread)
            .is_some_and(|known| known != &frozen)
            || !model.is_empty()
                && draft["model"]
                    .as_str()
                    .is_some_and(|expected| expected != model)
        {
            return Err(external("dispatch", "pre_send_settings", false, false));
        }
        if draft["transport"] == "app_server" {
            if self.routes.contains_key(&thread) {
                return Err(external(
                    "dispatch",
                    "pre_send_transport_changed",
                    false,
                    false,
                ));
            }
            if !self.thread_settings.contains_key(&thread) {
                self.resume_frozen(&thread, &frozen)?;
            }
            let result = self.native_turn(&thread, prompt)?;
            return Ok(bound_outcome(draft, &result));
        }
        if self
            .routes
            .get(&thread)
            .is_some_and(|route| route["previous_turn_id"] != draft["previous_turn_id"])
        {
            return Err(external(
                "dispatch",
                "pre_send_boundary_changed",
                false,
                false,
            ));
        }
        let current = self
            .read(&thread, false, None, None)
            .map_err(|_| external("dispatch", "pre_send_read", false, true))?;
        if current["thread"]["status"]["type"] != "idle"
            || !idle(&current)
            || current["turns"][0]["id"] != draft["previous_turn_id"]
        {
            return Err(external("dispatch", "pre_send_state_changed", false, false));
        }
        let mut arguments = json!({"threadId": thread, "hostId": "local", "prompt": prompt});
        if truthy(&draft["model"]) {
            arguments["model"] = draft["model"].clone();
        }
        let receipt = self.desktop_tool("send_message_to_thread", &arguments)?;
        Ok(merge(
            draft,
            &json!({"status": "in_progress", "send_receipt": receipt}),
        ))
    }

    fn poll_execution(&mut self, receipt: &Value) -> Result<Value> {
        let transport = required(receipt, "transport")?;
        ensure!(
            matches!(transport, "app_server" | "desktop_tools"),
            "Invalid pending transport"
        );
        let thread = required(receipt, "thread_id")?;
        let current = self.read(thread, true, None, Some(transport))?;
        let status = &current["thread"]["status"];
        let flags = status
            .get("activeFlags")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let mut receipt = merge(
            receipt,
            &json!({
                "thread_status": status["type"], "active_flags": flags
            }),
        );
        if current.get("observation_source").is_some() {
            receipt["observation_source"] = current["observation_source"].clone();
        }
        let turns = current["turns"]
            .as_array()
            .context("Missing execution turns")?;
        let waiting: Vec<_> = flags
            .as_array()
            .context("Invalid execution flags")?
            .iter()
            .filter(|flag| {
                matches!(
                    flag.as_str(),
                    Some("waitingOnApproval" | "waitingOnUserInput")
                )
            })
            .cloned()
            .collect();
        if !waiting.is_empty() {
            return Ok(merge(
                &receipt,
                &json!({
                    "status": "needs_attention", "waiting_flags": waiting,
                    "turn": turn_summary(turns.first().unwrap_or(&Value::Null))
                }),
            ));
        }
        if turns.is_empty() && current["observation_source"] == "app_server" {
            return Err(external("read", "desktop_read", false, true));
        }
        if turns.is_empty() || turns[0]["id"] == receipt["previous_turn_id"] {
            return Ok(merge(&receipt, &json!({"status": "in_progress"})));
        }
        let latest = turns[0].clone();
        let marker = required(&receipt, "dispatch_marker")?.to_owned();
        let anchor = receipt["dispatched_turn_id"].as_str().map(str::to_owned);
        let expected_prompt = receipt["dispatch_prompt_sha256"]
            .as_str()
            .map(str::to_owned);
        let source = receipt["source_thread_id"]
            .as_str()
            .or_else(|| self.config["desktop_caller_thread_id"].as_str())
            .map(str::to_owned);
        let mut page = current.clone();
        let mut seen = HashSet::new();
        let mut found = None;
        let pages = self.config["execution_history_pages"].as_u64().unwrap_or(8);
        let mut exhausted = true;
        for _ in 0..pages {
            let mut boundary = false;
            for turn in page["turns"]
                .as_array()
                .context("Invalid execution history page")?
            {
                if turn["id"] == receipt["previous_turn_id"] {
                    boundary = true;
                    break;
                }
                let prompt = dispatch_prompt(turn, source.as_deref());
                let markers: HashSet<_> = marker_expression()
                    .find_iter(&prompt)
                    .map(|matched| matched.as_str().to_owned())
                    .collect();
                if markers.iter().any(|candidate| candidate != &marker) {
                    return Err(external(
                        "poll",
                        "execution_association_conflict",
                        false,
                        false,
                    ));
                }
                if anchor
                    .as_ref()
                    .is_some_and(|anchor| turn["id"] == anchor.as_str())
                {
                    found = Some(turn.clone());
                    break;
                }
                if anchor.is_none() && markers.contains(&marker) {
                    if let Some(expected) = &expected_prompt {
                        if digest(prompt.trim().as_bytes()) == *expected
                            || digest(unescape(prompt.trim()).as_bytes()) == *expected
                        {
                            found = Some(turn.clone());
                            break;
                        }
                    } else {
                        found = Some(turn.clone());
                    }
                }
            }
            if boundary || found.is_some() && (anchor.is_some() || expected_prompt.is_some()) {
                exhausted = false;
                break;
            }
            if !truthy(&page["page"]["hasMore"]) {
                exhausted = false;
                break;
            }
            let cursor = required(&page["page"], "nextCursor")?.to_owned();
            if !seen.insert(cursor.clone()) {
                return Err(external(
                    "poll",
                    "execution_pagination_incomplete",
                    false,
                    false,
                ));
            }
            page = self.read(thread, true, Some(&cursor), Some(transport))?;
        }
        if exhausted {
            return Err(external("poll", "execution_history_bound", false, false));
        }
        let dispatch =
            found.ok_or_else(|| external("poll", "execution_dispatch_not_found", false, false))?;
        receipt["dispatched_turn_id"] = dispatch["id"].clone();
        receipt["continuation"] = json!(latest["id"] != dispatch["id"]);
        receipt["dispatch_turn"] = turn_summary(&dispatch);
        receipt["turn"] = turn_summary(&latest);
        if status["type"] == "active" {
            receipt["status"] = json!("in_progress");
        } else if latest["status"] == "completed"
            && matches!(status["type"].as_str(), Some("idle" | "notLoaded"))
        {
            receipt["status"] = json!("completed");
            receipt["text"] = json!(latest["items"]
                .as_array()
                .context("Missing final output items")?
                .iter()
                .filter(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer")
                .filter_map(|item| item["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"));
        } else if matches!(latest["status"].as_str(), Some("failed" | "interrupted")) {
            receipt["status"] = latest["status"].clone();
            receipt["turn"]["error"] = latest["error"].clone();
            receipt["automatic_retry_signal"] = retry_signal(&current, &latest, &receipt);
        } else {
            receipt["status"] = json!("in_progress");
        }
        Ok(receipt)
    }

    fn send_continuation(&mut self, draft: &Value, prompt: &str, model: &str) -> Result<Value> {
        if draft.get("send_receipt").is_some() || truthy(&draft["dispatched_turn_id"]) {
            return Err(external("continuation", "already_sent", true, false));
        }
        if digest(prompt.trim().as_bytes()) != draft["dispatch_prompt_sha256"]
            || !prompt.contains(required(draft, "dispatch_marker")?)
            || !truthy(&draft["retry_id"])
        {
            return Err(external(
                "continuation",
                "pre_send_intent_changed",
                false,
                false,
            ));
        }
        let thread = required(draft, "thread_id")?.to_owned();
        let transport = required(draft, "transport")?.to_owned();
        ensure!(
            matches!(transport.as_str(), "app_server" | "desktop_tools"),
            "Invalid continuation transport"
        );
        let native = transport == "app_server" && self.persistent();
        if self.persistent() {
            let owner = self
                .read_native(&thread, true, None)
                .map_err(|_| external("continuation", "pre_send_read", false, true))?;
            if !idle(&owner) {
                return Err(external(
                    "continuation",
                    "continuation_changed",
                    false,
                    false,
                ));
            }
        }
        let current = self
            .read(&thread, true, None, Some(&transport))
            .map_err(|_| external("continuation", "pre_send_read", false, true))?;
        continuation_allowed(&current, draft)?;
        if native {
            let frozen = settings(draft)?;
            if frozen["model"] != model {
                return Err(external("continuation", "pre_send_settings", false, false));
            }
            self.resume_frozen(&thread, &frozen)?;
            let current = self
                .read_native(&thread, true, None)
                .map_err(|_| external("continuation", "pre_send_read", false, true))?;
            continuation_allowed(&current, draft)?;
            return Ok(bound_outcome(draft, &self.native_turn(&thread, prompt)?));
        }
        let receipt = self.desktop_tool(
            "send_message_to_thread",
            &json!({
                "threadId": thread, "hostId": "local", "prompt": prompt, "model": model
            }),
        )?;
        Ok(merge(draft, &json!({"send_receipt": receipt})))
    }
}

fn continuation_allowed(current: &Value, draft: &Value) -> Result<()> {
    let latest = &current["turns"][0];
    let terminal = if draft["retry_source"] == "automatic" {
        retry_signal(current, latest, draft)["eligible"] == true
    } else {
        latest["status"] == "interrupted"
    };
    if !idle(current)
        || latest["id"].is_null()
        || latest["id"] != draft["previous_turn_id"]
        || !terminal
    {
        return Err(external(
            "continuation",
            "continuation_changed",
            false,
            false,
        ));
    }
    Ok(())
}

fn delegation(text: &str, source: Option<&str>) -> Option<String> {
    let capture = Regex::new(
        r"(?s)^<codex_delegation>\s*<source_thread_id>([^<]+)</source_thread_id>\s*<input>(.*?)</input>\s*</codex_delegation>$",
    )
    .unwrap()
    .captures(text.trim())?;
    if Some(&capture[1]) != source {
        return None;
    }
    Some(capture[2].to_owned())
}

fn dispatch_prompt(turn: &Value, source: Option<&str>) -> String {
    let Some(items) = turn["items"].as_array() else {
        return String::new();
    };
    let mut texts = Vec::new();
    for item in items {
        if item["type"] != "userMessage" {
            continue;
        }
        for part in item["content"].as_array().into_iter().flatten() {
            if part["type"] != "text" {
                continue;
            }
            let metadata = part
                .get("codexDelegation")
                .or_else(|| item.get("codexDelegation"));
            if metadata.is_some_and(|metadata| metadata["sourceThreadId"].as_str() != source) {
                continue;
            }
            let Some(text) = part["text"].as_str() else {
                continue;
            };
            if text.contains("<codex_delegation>") {
                if let Some(text) = delegation(text, source) {
                    texts.push(text);
                }
            } else {
                texts.push(text.to_owned());
            }
        }
    }
    let mut prompt = texts.join("\n");
    for item in items {
        if item["type"] == "functionCallOutput"
            && item["name"] == "send_message_to_thread"
            && item["namespace"] == "codex_app"
            && item["output"]["truncated"] == false
            && let Some(text) = item["output"]["text"]
                .as_str()
                .and_then(|text| delegation(text, source))
        {
            prompt.push('\n');
            prompt.push_str(&text);
        }
    }
    prompt
}

pub fn retry_signal(current: &Value, turn: &Value, receipt: &Value) -> Value {
    let mut code = turn["error"]["codexErrorInfo"].clone();
    let mut http_status = Value::Null;
    if let Some(object) = code.as_object()
        && object.len() == 1
    {
        let (kind, details) = object.iter().next().unwrap();
        http_status = details["httpStatusCode"].clone();
        code = json!(kind);
    }
    let mut result = json!({
        "eligible": false, "reason": "unclassified_terminal", "error_code": code
    });
    if !matches!(turn["status"].as_str(), Some("failed" | "interrupted")) || !idle(current) {
        result["reason"] = json!("not_idle_or_waiting");
        return result;
    }
    if turn["status"] == "interrupted" {
        let proof = &receipt["local_timeout"];
        let times = (
            turn["startedAt"].as_f64(),
            turn["completedAt"].as_f64(),
            proof["observed_at"].as_f64(),
            proof["seconds"].as_f64(),
        );
        let (Some(started), Some(completed), Some(observed), Some(seconds)) = times else {
            return result;
        };
        if truthy(&turn["error"])
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
        if !matches!(
            code.as_str(),
            Some(
                "httpConnectionFailed"
                    | "responseStreamConnectionFailed"
                    | "responseStreamDisconnected"
            )
        ) {
            return result;
        }
        if !http_status.is_null()
            && !matches!(http_status.as_u64(), Some(408 | 500 | 502 | 503 | 504))
        {
            result["reason"] = json!("nontransient_http_status");
            return result;
        }
    }
    let Some(items) = turn["items"].as_array() else {
        result["reason"] = json!("operation_history_missing");
        return result;
    };
    for item in items {
        let allowed = matches!(
            item["type"].as_str(),
            Some("userMessage" | "agentMessage" | "reasoning" | "error" | "contextCompaction")
        ) || item["type"] == "fileChange" && item["status"] == "completed"
            || item["type"] == "commandExecution"
                && item["status"] == "completed"
                && item["exitCode"].is_i64()
                && !truthy(&item["processId"]);
        if !allowed {
            result["reason"] = json!("operation_outcome_unknown");
            return result;
        }
    }
    result["eligible"] = json!(true);
    if turn["status"] != "interrupted" {
        result["reason"] = json!("structured_transport_failure");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::test_support::{MARKER, THREAD, config, snapshot};

    const CALLER: &str = "44444444-4444-4444-8444-444444444444";

    fn desktop_config(options: Value) -> Value {
        let mut native = config(json!({"active_writer": true}));
        let mut options = options;
        options["mcp"] = json!(true);
        native["desktop_tools_command"] = config(options)["codex_command"].clone();
        native["desktop_caller_thread_id"] = json!(CALLER);
        native
    }

    fn persistent_config(options: Value) -> Value {
        let mut config = config(options);
        config["execution_transport"] = json!("persistent_daemon");
        config["codex_command"]
            .as_array_mut()
            .unwrap()
            .push(json!("proxy"));
        config["desktop_caller_thread_id"] = json!(CALLER);
        config
    }

    fn receipt() -> Value {
        json!({
            "transport": "app_server", "thread_id": THREAD,
            "previous_turn_id": "old", "dispatch_marker": MARKER,
            "dispatched_turn_id": "native"
        })
    }

    fn recorded(path: &std::path::Path) -> Result<Vec<Value>> {
        std::fs::read_to_string(path)?
            .lines()
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect()
    }

    #[test]
    fn desktop_active_writer_fallback_sends_once_with_mcp_thread_metadata() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("calls.jsonl");
        let settings = desktop_config(json!({
            "desktop": snapshot("old", "completed", "", "idle"), "log": log
        }));
        let mut executor = RoutingExecutor::new(settings)?;
        executor.resume_thread(THREAD, "selected-model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = executor.prepare_dispatch(THREAD, &prompt)?;
        assert_eq!(draft["transport"], "desktop_tools");
        assert_eq!(draft["previous_turn_id"], "old");
        let outcome = executor.send_dispatch(&draft, &prompt, "selected-model")?;
        assert_eq!(outcome["send_receipt"]["queued"], true);
        assert!(outcome["dispatched_turn_id"].is_null());
        let calls = recorded(&log)?;
        let sends: Vec<_> = calls
            .iter()
            .filter(|call| call["params"]["name"] == "send_message_to_thread")
            .collect();
        assert_eq!(sends.len(), 1);
        assert_eq!(
            sends[0]["params"]["_meta"],
            json!({"openai/threadId": CALLER})
        );
        assert_eq!(sends[0]["params"]["arguments"]["model"], "selected-model");
        assert!(sends[0]["params"]["arguments"].get("sandbox").is_none());
        assert!(
            executor
                .send_dispatch(&outcome, &prompt, "selected-model")
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn prepared_desktop_draft_survives_restart_and_rechecks_boundary() -> Result<()> {
        let settings = desktop_config(json!({"desktop": snapshot("old", "completed", "", "idle")}));
        let mut first = RoutingExecutor::new(settings.clone())?;
        first.resume_thread(THREAD, "selected-model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = first.prepare_dispatch(THREAD, &prompt)?;
        drop(first);
        let mut restarted = RoutingExecutor::new(settings)?;
        assert_eq!(
            restarted.send_dispatch(&draft, &prompt, "selected-model")?["send_receipt"]["queued"],
            true
        );
        let mut changed =
            desktop_config(json!({"desktop": snapshot("changed", "completed", "", "idle")}));
        changed["execution_sandbox"] = json!("read-only");
        let mut restarted = RoutingExecutor::new(changed)?;
        let error = restarted
            .send_dispatch(&draft, &prompt, "selected-model")
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ExternalError>().unwrap().kind,
            "pre_send_read"
        );
        assert!(
            !error
                .downcast_ref::<ExternalError>()
                .unwrap()
                .outcome_unknown
        );
        Ok(())
    }

    #[test]
    fn altered_intent_settings_and_transport_prevent_send() -> Result<()> {
        let settings = desktop_config(json!({"desktop": snapshot("old", "completed", "", "idle")}));
        let mut executor = RoutingExecutor::new(settings)?;
        executor.resume_thread(THREAD, "model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = executor.prepare_dispatch(THREAD, &prompt)?;
        for (key, changed) in [
            ("dispatch_marker", json!("wrong")),
            ("dispatch_prompt_sha256", json!("wrong")),
            ("source_thread_id", json!("other")),
            ("model", json!("other")),
            ("sandbox", json!("read-only")),
            ("approvals_reviewer", json!("user")),
            ("transport", json!("app_server")),
            ("previous_turn_id", json!("changed")),
        ] {
            let mut changed_draft = draft.clone();
            changed_draft[key] = changed;
            let error = executor
                .send_dispatch(&changed_draft, &prompt, "model")
                .unwrap_err();
            assert!(
                !error
                    .downcast_ref::<ExternalError>()
                    .unwrap()
                    .outcome_unknown
            );
        }
        Ok(())
    }

    #[test]
    fn busy_desktop_never_sends() -> Result<()> {
        let settings =
            desktop_config(json!({"desktop": snapshot("old", "inProgress", "", "active")}));
        let mut executor = RoutingExecutor::new(settings)?;
        let error = executor
            .resume_thread(THREAD, "model", "workspace-write", "auto_review")
            .unwrap_err();
        assert!(
            !error
                .downcast_ref::<ExternalError>()
                .unwrap()
                .outcome_unknown
        );
        Ok(())
    }

    #[test]
    fn trait_idle_probe_delegates_to_owner_checks_without_resuming() -> Result<()> {
        for (owner, status, expected) in
            [("idle", "completed", true), ("active", "inProgress", false)]
        {
            let directory = tempfile::tempdir()?;
            let log = directory.path().join("idle.jsonl");
            let settings = desktop_config(json!({
                "desktop": snapshot("old", status, "", owner), "log": log
            }));
            let mut executor = RoutingExecutor::new(settings)?;
            let client: &mut dyn Executor = &mut executor;
            assert_eq!(client.is_thread_idle(THREAD)?, expected);
            let calls = recorded(&log)?;
            assert_eq!(
                calls
                    .iter()
                    .filter(|call| call["method"] == "tools/call")
                    .count(),
                1
            );
            assert!(
                calls
                    .iter()
                    .filter(|call| call["method"] == "tools/call")
                    .all(|call| call["params"]["name"] == "read_thread")
            );
            assert!(executor.native_clients.is_empty());
            assert!(executor.primary.response_history().is_empty());
        }
        Ok(())
    }

    #[test]
    fn unknown_desktop_send_does_not_fall_back_and_reconciles_by_prompt_digest() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("calls.jsonl");
        let settings = desktop_config(json!({
            "desktop": snapshot("old", "completed", "", "idle"), "send_exit": true, "log": log
        }));
        let mut executor = RoutingExecutor::new(settings)?;
        executor.resume_thread(THREAD, "model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = executor.prepare_dispatch(THREAD, &prompt)?;
        let error = executor
            .send_dispatch(&draft, &prompt, "model")
            .unwrap_err();
        assert!(
            error
                .downcast_ref::<ExternalError>()
                .unwrap()
                .outcome_unknown
        );
        drop(executor);
        let settings = desktop_config(json!({
            "desktop": snapshot("new", "inProgress", &prompt, "active"), "log": log
        }));
        let mut executor = RoutingExecutor::new(settings)?;
        let result = executor.poll_execution(&draft)?;
        assert_eq!(result["dispatched_turn_id"], "new");
        let calls = recorded(&log)?;
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["params"]["name"] == "send_message_to_thread")
                .count(),
            1
        );
        assert!(!calls.iter().any(|call| call["method"] == "turn/start"));
        Ok(())
    }

    #[test]
    fn native_owner_state_overrides_terminal_history_and_never_uses_desktop() -> Result<()> {
        for terminal in ["interrupted", "failed", "completed"] {
            let current = snapshot("native", terminal, MARKER, "active");
            let mut executor = RoutingExecutor::new(persistent_config(json!({"native": current})))?;
            let result = executor.poll_execution(&receipt())?;
            assert_eq!(result["status"], "in_progress");
            assert_eq!(result["observation_source"], "app_server");
            assert!(executor.desktop.is_none());
            assert!(executor.native_clients.is_empty());
        }
        Ok(())
    }

    #[test]
    fn native_completion_reads_full_items_and_keeps_dispatch_anchor() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("native.jsonl");
        let mut executor = RoutingExecutor::new(persistent_config(json!({
            "native": snapshot("native", "completed", MARKER, "idle"), "log": log
        })))?;
        let result = executor.poll_execution(&receipt())?;
        assert_eq!(result["status"], "completed");
        assert_eq!(result["text"], "完整结果");
        let calls = recorded(&log)?;
        let history = calls
            .iter()
            .find(|call| call["method"] == "thread/turns/list")
            .unwrap();
        assert_eq!(history["params"]["itemsView"], "full");
        assert_eq!(history["params"]["sortDirection"], "desc");
        assert!(!calls.iter().any(|call| call["method"] == "thread/resume"));
        Ok(())
    }

    #[test]
    fn waiting_flags_precede_history_and_are_not_retry_signals() -> Result<()> {
        for flag in ["waitingOnApproval", "waitingOnUserInput"] {
            let mut current = snapshot("native", "interrupted", MARKER, "idle");
            current["thread"]["status"]["activeFlags"] = json!([flag]);
            current["turns"] = json!([]);
            let mut executor = RoutingExecutor::new(persistent_config(json!({"native": current})))?;
            let outcome = executor.poll_execution(&receipt())?;
            assert_eq!(outcome["status"], "needs_attention");
            assert_eq!(outcome["waiting_flags"], json!([flag]));
            assert!(outcome.get("automatic_retry_signal").is_none());
        }
        Ok(())
    }

    #[test]
    fn native_empty_or_summarized_history_is_a_read_error() -> Result<()> {
        for current in [
            json!({"thread": {"id": THREAD, "status": {"type": "idle", "activeFlags": []}}, "turns": []}),
            json!({"thread": {"id": THREAD, "status": {"type": "idle"}}, "turns": [
                {"id": "native", "items": [], "itemsView": "summary"}
            ]}),
        ] {
            let mut executor = RoutingExecutor::new(persistent_config(json!({"native": current})))?;
            let error = executor.poll_execution(&receipt()).unwrap_err();
            let error = error.downcast_ref::<ExternalError>().unwrap();
            assert_eq!(error.operation, "read");
            assert_eq!(error.kind, "desktop_read");
            assert!(!error.outcome_unknown);
        }
        Ok(())
    }

    #[test]
    fn observer_connections_are_distinct_from_execution_and_metadata() -> Result<()> {
        let mut executor = RoutingExecutor::new(persistent_config(json!({
            "native": snapshot("native", "inProgress", MARKER, "active")
        })))?;
        executor.native(THREAD)?.connect()?;
        executor.poll_execution(&receipt())?;
        assert_eq!(executor.native_clients.len(), 1);
        assert_eq!(executor.native_observers.len(), 1);
        assert!(executor.primary.response_history().is_empty());
        Ok(())
    }

    #[test]
    fn persistent_dispatch_returns_after_start_and_preserves_frozen_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("native.jsonl");
        let mut executor = RoutingExecutor::new(persistent_config(json!({
            "turn_mode": "active", "log": log
        })))?;
        executor.resume_thread(THREAD, "selected-model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = executor.prepare_dispatch(THREAD, &prompt)?;
        let result = executor.send_dispatch(&draft, &prompt, "selected-model")?;
        assert_eq!(result["status"], "in_progress");
        assert_eq!(result["dispatched_turn_id"], "turn-1");
        assert_eq!(result["start_response_received"], true);
        let calls = recorded(&log)?;
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["method"] == "turn/start")
                .count(),
            1
        );
        assert!(!calls.iter().any(|call| call["method"] == "turn/interrupt"));
        Ok(())
    }

    #[test]
    fn unacknowledged_notification_does_not_bind_an_unrelated_turn() -> Result<()> {
        let mut executor = RoutingExecutor::new(persistent_config(json!({"turn_mode": "no_ack"})))?;
        executor.resume_thread(THREAD, "model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = executor.prepare_dispatch(THREAD, &prompt)?;
        let result = executor.send_dispatch(&draft, &prompt, "model")?;
        assert_eq!(result["status"], "needs_attention");
        assert_eq!(result["turn"]["id"], "turn-1");
        assert!(result["dispatched_turn_id"].is_null());
        Ok(())
    }

    #[test]
    fn persisted_native_intent_resumes_only_with_saved_permissions() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("native.jsonl");
        let settings = persistent_config(json!({"turn_mode": "active", "log": log}));
        let mut first = RoutingExecutor::new(settings.clone())?;
        first.resume_thread(THREAD, "original-model", "workspace-write", "auto_review")?;
        let prompt = format!("Dispatch {MARKER}");
        let draft = first.prepare_dispatch(THREAD, &prompt)?;
        drop(first);
        let mut changed = settings;
        changed["execution_sandbox"] = json!("read-only");
        changed["execution_approvals_reviewer"] = json!("user");
        let mut restarted = RoutingExecutor::new(changed)?;
        restarted.send_dispatch(&draft, &prompt, "original-model")?;
        let calls = recorded(&log)?;
        let resumed = calls
            .iter()
            .rev()
            .find(|call| call["method"] == "thread/resume")
            .unwrap();
        assert_eq!(resumed["params"]["model"], "original-model");
        assert_eq!(resumed["params"]["sandbox"], "workspace-write");
        assert_eq!(resumed["params"]["approvalsReviewer"], "auto_review");
        Ok(())
    }

    #[test]
    fn exact_digest_handles_xml_escaping_and_rejects_changed_marker_mentions() -> Result<()> {
        let prompt = format!("Dispatch {MARKER} <actualSHA256> & preserve history");
        let mut current = snapshot("new", "completed", "", "idle");
        current["turns"][0]["items"][0] = json!({
            "type": "functionCallOutput", "name": "send_message_to_thread", "namespace": "codex_app",
            "output": {"text": format!("<codex_delegation><source_thread_id>{CALLER}</source_thread_id><input>{}</input></codex_delegation>",
                prompt.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")),
                "truncated": false}
        });
        let mut executor = RoutingExecutor::new(desktop_config(json!({"desktop": current})))?;
        let draft = json!({
            "transport": "desktop_tools", "thread_id": THREAD, "previous_turn_id": "old",
            "dispatch_marker": MARKER, "source_thread_id": CALLER,
            "dispatch_prompt_sha256": digest(prompt.as_bytes())
        });
        assert_eq!(executor.poll_execution(&draft)?["status"], "completed");
        let mut changed = snapshot("new", "completed", &format!("Changed {MARKER}"), "idle");
        changed["thread"]["status"]["activeFlags"] = json!([]);
        let mut executor = RoutingExecutor::new(desktop_config(json!({"desktop": changed})))?;
        assert_eq!(
            executor
                .poll_execution(&draft)
                .unwrap_err()
                .downcast_ref::<ExternalError>()
                .unwrap()
                .kind,
            "association"
        );
        Ok(())
    }

    #[test]
    fn continuation_reports_latest_turn_separately_from_original_dispatch() -> Result<()> {
        let latest = snapshot("human", "completed", "Please continue", "idle");
        let original = snapshot("native", "completed", MARKER, "idle");
        let mut current = latest;
        current["turns"]
            .as_array_mut()
            .unwrap()
            .push(original["turns"][0].clone());
        let mut executor = RoutingExecutor::new(persistent_config(json!({"native": current})))?;
        let result = executor.poll_execution(&receipt())?;
        assert_eq!(result["dispatched_turn_id"], "native");
        assert_eq!(result["turn"]["id"], "human");
        assert_eq!(result["dispatch_turn"]["id"], "native");
        assert_eq!(result["continuation"], true);
        Ok(())
    }

    #[test]
    fn native_continuation_reuses_transport_and_permissions_and_guards_active_owner() -> Result<()>
    {
        let prompt = format!("Continue {MARKER}");
        let draft = json!({
            "transport": "app_server", "thread_id": THREAD, "previous_turn_id": "native",
            "dispatch_marker": MARKER, "source_thread_id": CALLER,
            "dispatch_prompt_sha256": digest(prompt.as_bytes()), "retry_id": "retry",
            "model": "model", "sandbox": "workspace-write", "approvals_reviewer": "auto_review"
        });
        for owner in ["active", "idle"] {
            let mut executor = RoutingExecutor::new(persistent_config(json!({
                "turn_mode": "active", "native": snapshot("native", "interrupted", MARKER, owner)
            })))?;
            let result = executor.send_continuation(&draft, &prompt, "model");
            if owner == "active" {
                assert_eq!(
                    result
                        .unwrap_err()
                        .downcast_ref::<ExternalError>()
                        .unwrap()
                        .kind,
                    "continuation_changed"
                );
                assert!(executor.native_clients.is_empty());
            } else {
                let result = result?;
                assert_eq!(result["transport"], "app_server");
                assert_eq!(result["dispatched_turn_id"], "turn-1");
                assert!(executor.desktop.is_none());
                let error = executor
                    .send_continuation(&result, &prompt, "model")
                    .unwrap_err();
                assert_eq!(
                    error.downcast_ref::<ExternalError>().unwrap().kind,
                    "already_sent"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn transient_retry_signals_require_structured_error_and_settled_operations() {
        let mut current = snapshot("native", "failed", MARKER, "idle");
        current["turns"][0]["error"] = json!({
            "codexErrorInfo": {"responseStreamDisconnected": {"httpStatusCode": 503}}
        });
        let turn = &current["turns"][0];
        assert_eq!(retry_signal(&current, turn, &receipt())["eligible"], true);
        current["turns"][0]["items"].as_array_mut().unwrap().push(json!({
            "type": "commandExecution", "status": "completed", "exitCode": 0, "processId": "background"
        }));
        assert_eq!(
            retry_signal(&current, &current["turns"][0], &receipt())["reason"],
            "operation_outcome_unknown"
        );
        current["turns"][0]["items"].as_array_mut().unwrap().pop();
        current["turns"][0]["error"]["codexErrorInfo"]["responseStreamDisconnected"]["httpStatusCode"] =
            json!(401);
        assert_eq!(
            retry_signal(&current, &current["turns"][0], &receipt())["reason"],
            "nontransient_http_status"
        );
    }

    #[test]
    fn interrupted_turn_requires_owned_timeout_proof_before_automatic_retry() {
        let mut current = snapshot("native", "interrupted", MARKER, "idle");
        current["turns"][0]["startedAt"] = json!(100.0);
        current["turns"][0]["completedAt"] = json!(700.0);
        assert_eq!(
            retry_signal(&current, &current["turns"][0], &receipt())["eligible"],
            false
        );
        let mut receipt = receipt();
        receipt["local_timeout"] = json!({
            "source": "owned_app_server_wait", "seconds": 600, "observed_at": 701.0, "turn_id": "native"
        });
        assert_eq!(
            retry_signal(&current, &current["turns"][0], &receipt)["eligible"],
            true
        );
    }
}
