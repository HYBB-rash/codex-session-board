#[path = "clients/notion.rs"]
mod notion;
#[path = "clients/routing.rs"]
mod routing;
#[path = "clients/rpc.rs"]
mod rpc;

pub use notion::{NotionMcp, connect_notion};
pub use routing::RoutingExecutor;
pub use rpc::{RpcClient, bridge};

use anyhow::Result;
use serde_json::Value;
use std::fmt;

pub trait Notion {
    fn query_tasks(&mut self) -> Result<Vec<Value>>;
    fn get_task_metadata(&mut self, pid: &str) -> Result<Value>;
    fn update_properties(&mut self, pid: &str, properties: &Value) -> Result<Value>;
    fn get_feedback_metadata(&mut self, pid: &str) -> Result<Value> {
        self.get_task_metadata(pid)
    }
    fn fetch(&mut self, pid: &str) -> Result<Value>;
    fn call(&mut self, role: &str, args: &Value) -> Result<Value>;
}

pub trait Executor {
    fn start_thread(
        &mut self,
        model: &str,
        cwd: &str,
        title: &str,
        sandbox: &str,
        reviewer: &str,
    ) -> Result<Value>;
    fn resume_thread(
        &mut self,
        thread: &str,
        model: &str,
        sandbox: &str,
        reviewer: &str,
    ) -> Result<Value>;
    fn prepare_dispatch(&mut self, thread: &str, prompt: &str) -> Result<Value>;
    fn send_dispatch(&mut self, draft: &Value, prompt: &str, model: &str) -> Result<Value>;
    fn poll_execution(&mut self, receipt: &Value) -> Result<Value>;
    fn is_thread_idle(&mut self, _thread: &str) -> Result<bool> {
        Err(anyhow::anyhow!("Execution idle check unavailable"))
    }
    fn send_continuation(&mut self, draft: &Value, prompt: &str, model: &str) -> Result<Value> {
        self.send_dispatch(draft, prompt, model)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalError {
    pub operation: String,
    pub outcome_unknown: bool,
    pub reconnectable: bool,
    pub kind: String,
}

impl fmt::Display for ExternalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.kind)?;
        if self.outcome_unknown {
            write!(formatter, "; reconcile before resubmitting")?;
        }
        Ok(())
    }
}

impl std::error::Error for ExternalError {}

pub(super) fn external(
    operation: &str,
    kind: &str,
    outcome_unknown: bool,
    reconnectable: bool,
) -> anyhow::Error {
    let kind = if kind.starts_with("pre_send_") {
        "pre_send_read"
    } else if matches!(
        kind,
        "execution_association_conflict"
            | "execution_pagination_incomplete"
            | "execution_history_bound"
            | "execution_dispatch_not_found"
    ) {
        "association"
    } else {
        kind
    };
    ExternalError {
        operation: operation.to_owned(),
        outcome_unknown,
        reconnectable: reconnectable && !outcome_unknown,
        kind: kind.to_owned(),
    }
    .into()
}

pub(super) fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| external("configuration", &format!("missing_{key}"), false, false))
}

pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

#[cfg(test)]
#[path = "clients/test_support.rs"]
mod test_support;
