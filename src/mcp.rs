use crate::{config, operations::{Action, OPERATIONS, Operation}, runtime::RuntimeHandle};
use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;

pub const RESOURCE: &str = "ui://session-board/index.html";
pub const MIME: &str = "text/html;profile=mcp-app";
const VIEW: &str = "board_view";

pub fn tools() -> Value {
    tools_for(false)
}

pub fn tools_for(legacy: bool) -> Value {
    tools_for_mode(legacy, false)
}

fn tools_for_mode(legacy: bool, four_columns: bool) -> Value {
    let icon =
        base64::engine::general_purpose::STANDARD.encode(include_bytes!("../assets/board.svg"));
    let describe = |name: &str, title: &str, read_only, action, schema: (Value, Value), meta| {
        let (properties, required) = schema;
        json!({"name":name,"title":title,"description":title,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read_only,
                "destructiveHint":matches!(action, Some(Action::Archive | Action::PluginUpdateStart)),
                "openWorldHint":action == Some(Action::PluginUpdateStart)},
            "icons":[{"src":format!("data:image/svg+xml;base64,{icon}"),"mimeType":"image/svg+xml"}],
            "_meta":meta})
    };
    let mut tools = vec![describe(VIEW, "会话看板", true, None, (json!({}), json!([])),
        json!({"ui":{"resourceUri":RESOURCE,"visibility":["app","model"]},
            "openai/ui":{"entrypoints":[{"type":"global"}]}}))];
    for op in OPERATIONS.iter() {
        if legacy && matches!(op.action, Action::Archive | Action::ArchiveBatches | Action::ArchiveBatchStart
            | Action::ArchiveBatchFinish | Action::ArchiveBatchCheck | Action::NewReset | Action::Connect
            | Action::ProjectAssign | Action::SshSetup) {
            continue;
        }
        tools.push(describe(op.tool, op.title, op.read_only, Some(op.action),
            schema(op.action, legacy, four_columns), json!({"ui":{"visibility":["app"]}})));
    }
    json!({"tools":tools})
}

fn schema(action: Action, legacy: bool, four_columns: bool) -> (Value, Value) {
    match action {
        Action::Sort => (
            json!({"state":{"type":"string","enum":crate::model::visible_states(legacy, four_columns).map(|item| item.0).collect::<Vec<_>>()},
                "field":{"type":"string","enum":["updated_at"]},
                "direction":{"type":"string","enum":["asc","desc"]},
                "version":{"type":"integer","minimum":0}}),
            json!(["state", "field", "direction", "version"]),
        ),
        Action::Update => (
            json!({
                "id":{"type":"string"}, "state":{"type":["string","null"],
                    "enum":crate::model::visible_states(legacy, four_columns).map(|item| json!(item.0)).chain([Value::Null]).collect::<Vec<_>>()},
                "note":{"type":"string","maxLength":2000},
                "sectionVersion":{"type":["string","null"]},
                "version":{"type":"integer","minimum":0},"inputVersion":{"type":"integer","minimum":0}
            }),
            json!(["id", "state", "version", "inputVersion"]),
        ),
        Action::ProjectAssign => (
            json!({"id":{"type":"string"},"projectId":{"type":"string"},"confirmed":{"type":"boolean","const":true}}),
            json!(["id","projectId","confirmed"]),
        ),
        Action::Archive => (
            json!({"id":{"type":"string"},"version":{"type":"integer","minimum":0},
                "inputVersion":{"type":"integer","minimum":0},"sectionVersion":{"type":["string","null"]},
                "batchId":{"type":"string","format":"uuid"}}),
            json!(["id", "version", "inputVersion"]),
        ),
        Action::NewReset => (
            json!({"projectId":{"type":"string"},
            "requestId":{"type":"string"},"confirmed":{"type":"boolean","const":true}}),
            json!(["projectId", "requestId", "confirmed"]),
        ),
        _ => basic_schema(action),
    }
}

fn basic_schema(action: Action) -> (Value, Value) {
    match action {
        Action::ArchiveBatchStart => (
            json!({"id":{"type":"string","format":"uuid"},
                "items":{"type":"array","minItems":1,"items":{"type":"object",
                    "properties":{"id":{"type":"string"},"title":{"type":"string","maxLength":500},
                        "version":{"type":"integer","minimum":0},"inputVersion":{"type":"integer","minimum":0},
                        "sectionVersion":{"type":["string","null"]}},
                    "required":["id","title","version","inputVersion"],"additionalProperties":false}}}),
            json!(["id", "items"]),
        ),
        Action::ArchiveBatchFinish | Action::ArchiveBatchCheck =>
            (json!({"id":{"type":"string","format":"uuid"}}), json!(["id"])),
        Action::Open | Action::PreviewReply => (json!({"id":{"type":"string"}}), json!(["id"])),
        Action::Connect => (json!({"hostId":{"type":"string"}}), json!(["hostId"])),
        _ => setup_schema(action),
    }
}

fn setup_schema(action: Action) -> (Value, Value) {
    if action == Action::New {
        return (json!({"projectId":{"type":"string"},"requestId":{"type":"string"},
            "hostId":{"type":"string"},"operationId":{"type":"string","format":"uuid"},
            "consent":{"type":"string"}}), json!([]));
    }
    if action != Action::SshSetup {
        return (json!({}), json!([]));
    }
    (json!({
        "action":{"type":"string","enum":["status","preview","apply","decline","restore","recheck"]},
        "target":{"type":"object","properties":{"hostname":{"type":"string"},"user":{"type":"string"},
            "port":{"type":"string"},"directory":{"type":"string"}},
            "required":["hostname","user","port","directory"],"additionalProperties":false},
        "previewId":{"type":"string","format":"uuid"},"targetId":{"type":"string"},"alias":{"type":"string"}
    }), json!(["action"]))
}

pub fn resource() -> Value {
    let bridge = include_str!("../assets/app-bridge.js").replace("</script", "<\\/script");
    let html = include_str!("../index.html")
        .replace("__CSRF_TOKEN__", "")
        .replacen(
            "<script>",
            &format!("<script>{bridge}</script>\n<script>"),
            1,
        );
    json!({"uri":RESOURCE,"mimeType":MIME,"text":html,
        "_meta":{"ui":{"prefersBorder":false,"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[]}},
        "openai/ui":{"preferredDisplayMode":"fullscreen","availableDisplayModes":["inline","fullscreen"]}}})
}

fn backend(runtime: &mut RuntimeHandle, name: &str, arguments: &Value) -> Result<Value> {
    if name == VIEW {
        return Ok(ordinary_result(json!({"ready":true})));
    }
    let op = Operation::for_tool(name).context("未知看板工具")?;
    let value = runtime.request(op, arguments)?;
    Ok(if op.action == Action::PreviewReply {
        json!({"content":[],"_meta":{"previewReply":value}})
    } else {
        ordinary_result(value)
    })
}

fn ordinary_result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value})
}

fn dispatch(method: &str, params: &Value, runtime: &mut RuntimeHandle) -> Result<Value> {
    match method {
        "initialize" => Ok(
            json!({"protocolVersion":params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities":{"tools":{},"resources":{}},
            "serverInfo":{"name":"codex-session-board","version":env!("CARGO_PKG_VERSION")},
            "instructions":"会话看板。不发送消息或执行任务。功能取决于配置模式和主机连接状态；已连接的本地与远程会话共用状态、标题及归档逻辑。"}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => {
            Ok(tools_for_mode(runtime.legacy(), runtime.four_columns()))
        }
        "resources/list" => {
            Ok(json!({"resources":[{"uri":RESOURCE,"name":"会话看板","mimeType":MIME}]}))
        }
        "resources/templates/list" => Ok(json!({"resourceTemplates":[]})),
        "resources/read" => {
            ensure!(params["uri"] == RESOURCE, "未知界面资源");
            Ok(json!({"contents":[resource()]}))
        }
        "tools/call" => Ok(
            match backend(
                runtime,
                params["name"].as_str().context("缺少工具名称")?,
                params.get("arguments").unwrap_or(&json!({})),
            ) {
                Ok(result) => result,
                Err(error) => {
                    json!({"isError":true,"content":[{"type":"text","text":error.to_string()}]})
                }
            },
        ),
        _ => bail!("Unknown method: {method}"),
    }
}

pub fn serve(path: Option<&Path>) -> Result<()> {
    let entry = config::entry_path()?;
    let path = path.unwrap_or(&entry);
    let mut runtime = RuntimeHandle::load(path)?;
    for line in std::io::stdin().lock().lines() {
        let result = answer(&line?, &mut runtime);
        if let Some(response) = result {
            writeln!(std::io::stdout().lock(), "{response}")?;
            std::io::stdout().flush()?;
        }
    }
    runtime.shutdown()
}

fn answer(line: &str, runtime: &mut RuntimeHandle) -> Option<Value> {
    let parsed = serde_json::from_str::<Value>(line);
    let request = match parsed {
        Ok(request) => request,
        Err(error) => {
            return Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":error.to_string()}}),
            );
        }
    };
    let identifier = request.get("id")?;
    let params = request.get("params").cloned().unwrap_or(json!({}));
    Some(
        match dispatch(request["method"].as_str().unwrap_or(""), &params, runtime) {
            Ok(result) => json!({"jsonrpc":"2.0","id":identifier,"result":result}),
            Err(error) => {
                json!({"jsonrpc":"2.0","id":identifier,"error":{"code":-32602,"message":error.to_string()}})
            }
        },
    )
}
