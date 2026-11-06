use crate::{config, service};
use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

pub const RESOURCE: &str = "ui://session-board/index.html";
pub const MIME: &str = "text/html;profile=mcp-app";

pub fn tools() -> Value {
    let mut tools = Vec::new();
    let icon =
        base64::engine::general_purpose::STANDARD.encode(include_bytes!("../assets/board.svg"));
    for (name, title, read_only) in [
        ("board_view", "会话看板", true),
        ("board_snapshot", "读取会话状态", true),
        ("board_update", "确认工作状态", false),
        ("board_archive", "归档待归档会话", false),
        ("board_open", "打开已有会话", true),
        ("board_new", "打开新建会话页", false),
        ("board_refresh", "重新发现", false),
    ] {
        let (properties, required) = schema(name);
        let meta = if name == "board_view" {
            json!({"ui":{"resourceUri":RESOURCE,"visibility":["app","model"]},
                "openai/ui":{"entrypoints":[{"type":"global"}]}})
        } else {
            json!({"ui":{"visibility":["app"]}})
        };
        tools.push(json!({"name":name,"title":title,"description":title,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read_only,"destructiveHint":name == "board_archive","openWorldHint":false},
            "icons":[{"src":format!("data:image/svg+xml;base64,{icon}"),"mimeType":"image/svg+xml"}],
            "_meta":meta}));
    }
    json!({"tools":tools})
}

fn schema(name: &str) -> (Value, Value) {
    match name {
        "board_update" => (
            json!({
                "id":{"type":"string"}, "state":{"type":["string","null"]},
                "note":{"type":"string","maxLength":2000},
                "version":{"type":"integer","minimum":0},"inputVersion":{"type":"integer","minimum":0}
            }),
            json!(["id", "state", "version", "inputVersion"]),
        ),
        "board_open" => (json!({"id":{"type":"string"}}), json!(["id"])),
        "board_archive" => (
            json!({"id":{"type":"string"},"version":{"type":"integer","minimum":0},
                "inputVersion":{"type":"integer","minimum":0}}),
            json!(["id", "version", "inputVersion"]),
        ),
        _ => (json!({}), json!([])),
    }
}

pub fn resource() -> Value {
    let bridge = include_str!("../assets/app-bridge.js")
        .replace(
            "\"/api/refresh\":\"board_refresh\"",
            "\"/api/refresh\":\"board_refresh\",\"/api/new\":\"board_new\",\"/api/archive\":\"board_archive\"",
        )
        .replace("</script", "<\\/script");
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

pub fn backend(config_path: &Path, name: &str, arguments: &Value) -> Result<Value> {
    if name == "board_view" {
        return Ok(json!({"ready":true}));
    }
    let path = match name {
        "board_snapshot" => "/api/sessions",
        "board_update" => "/api/state",
        "board_archive" => "/api/archive",
        "board_open" => "/api/open",
        "board_new" => "/api/new",
        "board_refresh" => "/api/refresh",
        _ => bail!("未知看板工具"),
    };
    let running = service::launch(config_path)?;
    let url = running["url"].as_str().context("服务未返回地址")?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .build()?;
    if name == "board_snapshot" {
        return Ok(client
            .get(format!("{url}{path}"))
            .send()?
            .error_for_status()?
            .json()?);
    }
    post(&client, url, path, arguments)
}

fn post(
    client: &reqwest::blocking::Client,
    url: &str,
    path: &str,
    arguments: &Value,
) -> Result<Value> {
    let html = client.get(url).send()?.error_for_status()?.text()?;
    let token = html
        .split("const csrf = \"")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .filter(|value| !value.is_empty())
        .context("无法读取本地服务令牌")?;
    let response = client
        .post(format!("{url}{path}"))
        .header("X-CSRF-Token", token)
        .json(arguments)
        .send()?;
    let status = response.status();
    let result: Value = response.json()?;
    ensure!(
        status.is_success(),
        "{}",
        result["error"].as_str().unwrap_or("看板操作失败")
    );
    Ok(result)
}

pub fn dispatch(method: &str, params: &Value, path: &Path) -> Result<Value> {
    match method {
        "initialize" => Ok(
            json!({"protocolVersion":params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities":{"tools":{},"resources":{}},
            "serverInfo":{"name":"codex-session-board","version":env!("CARGO_PKG_VERSION")},
            "instructions":"本地会话看板。不发送消息或执行任务。仅显式归档按钮归档待归档会话。"}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
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
                path,
                params["name"].as_str().context("缺少工具名称")?,
                params.get("arguments").unwrap_or(&json!({})),
            ) {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value})
                }
                Err(error) => {
                    json!({"isError":true,"content":[{"type":"text","text":error.to_string()}]})
                }
            },
        ),
        _ => bail!("Unknown method: {method}"),
    }
}

pub fn serve(path: Option<&Path>) -> Result<()> {
    let defaults = config::default_paths()?;
    let path = path.unwrap_or(&defaults.0);
    for line in std::io::stdin().lock().lines() {
        let result = answer(&line?, path);
        if let Some(response) = result {
            writeln!(std::io::stdout().lock(), "{response}")?;
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}

fn answer(line: &str, path: &Path) -> Option<Value> {
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
        match dispatch(request["method"].as_str().unwrap_or(""), &params, path) {
            Ok(result) => json!({"jsonrpc":"2.0","id":identifier,"result":result}),
            Err(error) => {
                json!({"jsonrpc":"2.0","id":identifier,"error":{"code":-32602,"message":error.to_string()}})
            }
        },
    )
}
