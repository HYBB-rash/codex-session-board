use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum Action {
    Snapshot,
    Update,
    Sort,
    Archive,
    ArchiveBatches,
    ArchiveBatchStart,
    ArchiveBatchFinish,
    ArchiveBatchCheck,
    Open,
    PreviewReply,
    New,
    NewReset,
    ProjectAssign,
    Connect,
    SshSetup,
    Refresh,
    PluginUpdateStatus,
    PluginUpdateStart,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum Method {
    Get,
    Post,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Operation {
    pub action: Action,
    pub method: Method,
    pub path: &'static str,
    pub tool: &'static str,
    pub title: &'static str,
    pub read_only: bool,
}

pub(crate) static OPERATIONS: LazyLock<Vec<Operation>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/board-operations.json"))
        .unwrap_or_else(|error| panic!("看板操作登记无效：{error}"))
});

impl Operation {
    pub fn for_request(path: &str, method: Method) -> Option<&'static Self> {
        OPERATIONS.iter().find(|op| op.method == method && op.path == path)
    }

    pub fn for_tool(tool: &str) -> Option<&'static Self> {
        OPERATIONS.iter().find(|op| op.tool == tool)
    }
}
