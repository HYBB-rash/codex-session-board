use serde_json::{Value, json};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufWriter, Write};
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const THREAD: &str = "33333333-3333-4333-8333-333333333333";
const SCHEMA: &str = "<sqlite-table>CREATE TABLE tasks (url TEXT, \"任务名称\" TEXT, \"状态\" TEXT, \"执行人 / 代理\" TEXT, \"AI 会话 / 线程\" TEXT, \"模型\" TEXT)</sqlite-table>";

struct Server<Output> {
    options: Value,
    output: Output,
    seen: Vec<Value>,
    initialized: bool,
    query_index: usize,
    desktop_index: usize,
    turn_index: usize,
}

impl<Output: Write> Server<Output> {
    fn emit(&mut self, value: &Value) -> Result<()> {
        serde_json::to_writer(&mut self.output, value)?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        Ok(())
    }

    fn reply(&mut self, request: &Value, result: Value) -> Result<()> {
        self.emit(&json!({"id": request["id"], "result": result}))
    }

    fn error(&mut self, request: &Value, error: Value) -> Result<()> {
        self.emit(&json!({"id": request["id"], "error": error}))
    }

    fn note(&mut self, method: &str, params: Value) -> Result<()> {
        self.emit(&json!({"method": method, "params": params}))
    }

    fn handle(&mut self, request: Value) -> Result<Option<i32>> {
        self.record_request(&request)?;
        let method = string(&request["method"])?;
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        match method {
            "initialize" => self.initialize(&request)?,
            "initialized" | "notifications/initialized" => self.initialized = true,
            _ if !self.initialized => {
                self.error(
                    &request,
                    json!({"code": -32000, "message": "Not initialized"}),
                )?;
            }
            _ if method.starts_with("test/") => return self.test_message(method, &request, params),
            _ if method.starts_with("thread/") => self.thread_message(method, &request, &params)?,
            "turn/start" => self.start_turn(&request, &params)?,
            "mcpServer/tool/call" => return self.notion_tool(&request, &params),
            "tools/call" => return self.desktop_tool(&request, &params),
            _ => self.unknown_method(&request)?,
        }
        Ok(None)
    }

    fn record_request(&mut self, request: &Value) -> Result<()> {
        self.seen.push(request.clone());
        if truthy(&self.options["log"]) {
            let path = string(&self.options["log"])?;
            let mut log = OpenOptions::new().create(true).append(true).open(path)?;
            let mut bytes = serde_json::to_vec(request)?;
            bytes.push(b'\n');
            log.write_all(&bytes)?;
        }
        Ok(())
    }

    fn unknown_method(&mut self, request: &Value) -> Result<()> {
        self.error(
            request,
            json!({"code": -32601, "message": "unknown method"}),
        )
    }

    fn test_result(&self, method: &str, params: Value) -> Option<Value> {
        match method {
            "test/history" => Some(json!(self.seen)),
            "test/echo" => Some(params),
            "test/null" => Some(Value::Null),
            "test/scalar" => Some(json!(42)),
            _ => None,
        }
    }

    fn test_message(
        &mut self,
        method: &str,
        request: &Value,
        params: Value,
    ) -> Result<Option<i32>> {
        if let Some(result) = self.test_result(method, params) {
            self.reply(request, result)?;
            return Ok(None);
        }
        match method {
            "test/interleaved" => {
                self.emit(&json!({"id": "unrelated", "result": {"keep": true}}))?;
                self.note("unrelated/notice", json!({"keep": true}))?;
                self.reply(request, json!({"ok": true}))?;
            }
            "test/chunks" => self.test_chunks(request)?,
            "test/attention" => {
                self.emit(&json!({
                    "id": request["id"], "method": "item/tool/requestUserInput", "params": {}
                }))?;
                self.reply(request, json!({}))?;
            }
            "test/error" => self.error(
                request,
                json!({"code": -32603, "message": "private-secret", "data": {"keep": true}}),
            )?,
            _ => return self.test_transport(method, request),
        }
        Ok(None)
    }

    fn test_chunks(&mut self, request: &Value) -> Result<()> {
        let mut packet = serde_json::to_vec(&json!({
            "id": request["id"], "result": {"text": "你好🙂"}
        }))?;
        packet.extend_from_slice(b"\r\n");
        self.output.write_all(b"\n\r\n")?;
        self.output.flush()?;
        for byte in packet {
            self.output.write_all(&[byte])?;
            self.output.flush()?;
        }
        Ok(())
    }

    fn test_transport(&mut self, method: &str, request: &Value) -> Result<Option<i32>> {
        match method {
            "test/partial" => {
                self.output.write_all(b"{\"id\":")?;
                self.output.flush()?;
            }
            "test/bad" => {
                let bad = self
                    .options
                    .get("bad")
                    .and_then(Value::as_str)
                    .unwrap_or("[]");
                self.output.write_all(bad.as_bytes())?;
                self.output.write_all(b"\n")?;
                self.output.flush()?;
            }
            "test/exit" => return Ok(Some(7)),
            "test/block" => {
                self.reply(request, json!({}))?;
                std::thread::sleep(Duration::from_secs(30));
            }
            _ => self.unknown_method(request)?,
        }
        Ok(None)
    }

    fn thread_message(&mut self, method: &str, request: &Value, params: &Value) -> Result<()> {
        match method {
            "thread/start" => self.thread_start(request, params),
            "thread/name/set" => self.thread_name(request),
            "thread/resume" => self.thread_resume(request, params),
            "thread/read" => self.thread_read(request, params),
            "thread/turns/list" => self.reply(request, json!({
                "data": self.options["native"].get("turns").cloned().unwrap_or_else(|| json!([])),
                "nextCursor": null
            })),
            _ => self.unknown_method(request),
        }
    }

    fn thread_start(&mut self, request: &Value, params: &Value) -> Result<()> {
        self.reply(
            request,
            json!({"thread": {
                "id": self.options.get("thread_id").cloned().unwrap_or_else(|| json!(THREAD)),
                "ephemeral": params["ephemeral"], "cwd": params["cwd"], "custom": 1
            }}),
        )
    }

    fn thread_name(&mut self, request: &Value) -> Result<()> {
        if truthy(&self.options["title_error"]) {
            self.error(request, json!({"code": -9, "message": "name rejected"}))
        } else {
            self.reply(request, json!({}))
        }
    }

    fn thread_resume(&mut self, request: &Value, params: &Value) -> Result<()> {
        if truthy(&self.options["active_writer"]) {
            self.error(
                request,
                json!({
                    "code": -32600, "message": "thread already has an active writer"
                }),
            )
        } else {
            self.reply(
                request,
                json!({"thread": {
                    "id": params["threadId"], "custom": 2
                }}),
            )
        }
    }

    fn thread_read(&mut self, request: &Value, params: &Value) -> Result<()> {
        let thread = self.options["native"]
            .get("thread")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "id": params["threadId"], "status": {"type": "idle", "activeFlags": []}
                })
            });
        self.reply(request, json!({"thread": thread}))
    }

    fn initialize(&mut self, request: &Value) -> Result<()> {
        if truthy(&self.options["init_attention"]) {
            self.emit(&json!({
                "id": request["id"], "method": "future/request", "params": {}
            }))?;
        } else if truthy(&self.options["mcp"]) {
            require_jsonrpc(request)?;
            self.reply(request, json!({"protocolVersion": "2024-11-05"}))?;
        } else {
            if truthy(&self.options["stderr"]) {
                let mut stderr = io::stderr().lock();
                stderr.write_all(&vec![b'x'; 100_000])?;
                stderr.write_all(b"\xffTAIL")?;
                stderr.flush()?;
            }
            self.reply(
                request,
                json!({"userAgent": "offline-fake", "platformOs": "linux"}),
            )?;
        }
        Ok(())
    }

    fn start_turn(&mut self, request: &Value, params: &Value) -> Result<()> {
        self.turn_index += 1;
        let turn = json!({
            "id": format!("turn-{}", self.turn_index), "status": "inProgress", "items": []
        });
        let mode = self.options["turn_mode"]
            .as_str()
            .unwrap_or("completed")
            .to_owned();
        self.note(
            "turn/started",
            json!({"threadId": params["threadId"], "turn": turn}),
        )?;
        if mode == "no_ack" {
            self.emit(&json!({
                "id": request["id"], "method": "item/commandExecution/requestApproval", "params": {}
            }))?;
            return Ok(());
        }
        if mode != "early" {
            self.reply(request, json!({"turn": turn}))?;
        }
        if mode == "active" {
            return Ok(());
        }
        self.turn_output(request, params, &turn, &mode)
    }

    fn turn_output(
        &mut self,
        request: &Value,
        params: &Value,
        turn: &Value,
        mode: &str,
    ) -> Result<()> {
        self.note(
            "item/agentMessage/delta",
            json!({
                "threadId": params["threadId"], "turnId": turn["id"],
                "itemId": "answer", "delta": "draft"
            }),
        )?;
        if mode == "attention" {
            self.emit(&json!({
                "id": request["id"], "method": "future/request", "params": {}
            }))?;
            return Ok(());
        }
        self.finish_turn(request, params, turn, mode)
    }

    fn finish_turn(
        &mut self,
        request: &Value,
        params: &Value,
        turn: &Value,
        mode: &str,
    ) -> Result<()> {
        self.note(
            "item/completed",
            json!({
                "threadId": params["threadId"], "turnId": turn["id"], "item": {
                    "id": "answer", "type": "agentMessage",
                    "phase": "final_answer", "text": "完整结果"
                }
            }),
        )?;
        let mut done = turn.clone();
        done["status"] = json!(if matches!(mode, "failed" | "interrupted") {
            mode
        } else {
            "completed"
        });
        done["custom"] = json!("terminal");
        self.note(
            "turn/completed",
            json!({"threadId": params["threadId"], "turn": done}),
        )?;
        if mode == "early" {
            self.reply(request, json!({"turn": turn}))?;
        }
        Ok(())
    }

    fn notion_tool(&mut self, request: &Value, params: &Value) -> Result<Option<i32>> {
        let tool = string(&params["tool"])?;
        let role = tool.strip_suffix("-tool").unwrap_or(tool);
        let args = &params["arguments"];
        if truthy(&self.options["rpc_error"])
            && role == self.options["error_role"].as_str().unwrap_or("query")
        {
            self.error(request, self.options["rpc_error"].clone())?;
            return Ok(None);
        }
        let result = self.notion_result(role, args)?;
        if self.options["disconnect_role"] == role {
            return Ok(Some(0));
        }
        self.reply(
            request,
            json!({
                "content": [{"type": "text", "text": serde_json::to_string(&result)?}]
            }),
        )?;
        Ok(None)
    }

    fn notion_fetch(&self, args: &Value) -> Result<Value> {
        let identifier = string(&args["id"])?;
        Ok(if identifier.starts_with("collection://") {
            json!({"text": self.options.get("schema").cloned().unwrap_or_else(|| json!(SCHEMA))})
        } else if identifier.starts_with("notion://docs/") {
            json!({"text": "Notion Markdown specification"})
        } else {
            self.options
                .get("page")
                .cloned()
                .unwrap_or_else(|| json!({"text": ""}))
        })
    }

    fn notion_result(&mut self, role: &str, args: &Value) -> Result<Value> {
        Ok(match role {
            "access" => self
                .options
                .get("access")
                .cloned()
                .unwrap_or_else(|| json!({"tools": {}})),
            "fetch" => self.notion_fetch(args)?,
            "query" => {
                let pages = self
                    .options
                    .get("query_pages")
                    .cloned()
                    .unwrap_or_else(|| json!([{"rows": []}]));
                let result = page_at(&pages, self.query_index)?;
                self.query_index += 1;
                result
            }
            _ => self
                .options
                .get("write_result")
                .cloned()
                .unwrap_or_else(|| json!({"success": true})),
        })
    }

    fn desktop_tool(&mut self, request: &Value, params: &Value) -> Result<Option<i32>> {
        require_jsonrpc(request)?;
        let result = if params["name"] == "read_thread" {
            self.desktop_read_result()?
        } else {
            if truthy(&self.options["send_exit"]) {
                return Ok(Some(0));
            }
            self.options
                .get("send_receipt")
                .cloned()
                .unwrap_or_else(|| json!({"queued": true}))
        };
        self.reply(
            request,
            json!({
                "content": [{"type": "text", "text": serde_json::to_string(&result)?}]
            }),
        )?;
        Ok(None)
    }

    fn desktop_read_result(&mut self) -> Result<Value> {
        let pages = self
            .options
            .get("desktop_pages")
            .cloned()
            .unwrap_or_else(|| {
                json!([self
                    .options
                    .get("desktop")
                    .cloned()
                    .unwrap_or_else(|| json!({}))])
            });
        let result = page_at(&pages, self.desktop_index)?;
        self.desktop_index += 1;
        Ok(result)
    }
}

fn string(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Expected a string").into())
}

fn require_jsonrpc(request: &Value) -> Result<()> {
    if request["jsonrpc"] != "2.0" {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Expected JSON-RPC 2.0").into());
    }
    Ok(())
}

fn page_at(pages: &Value, index: usize) -> Result<Value> {
    let pages = pages
        .as_array()
        .filter(|pages| !pages.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected nonempty fixture pages",
            )
        })?;
    Ok(pages[index.min(pages.len() - 1)].clone())
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn run() -> Result<i32> {
    let options = std::env::args()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing fixture options"))?;
    let options = serde_json::from_str(&options)?;
    let stdout = io::stdout();
    let mut server = Server {
        options,
        output: BufWriter::new(stdout.lock()),
        seen: Vec::new(),
        initialized: false,
        query_index: 0,
        desktop_index: 0,
        turn_index: 0,
    };
    for line in io::stdin().lock().lines() {
        let request = serde_json::from_str(&line?)?;
        if let Some(code) = server.handle(request)? {
            return Ok(code);
        }
    }
    Ok(0)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("protocol test server: {error}");
            std::process::exit(1);
        }
    }
}
