use crate::app::App;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

fn header<'request>(request: &'request Request, name: &str) -> Option<&'request str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

pub fn authorized(
    host: Option<&str>,
    origin: Option<&str>,
    csrf: Option<&str>,
    token: &str,
    port: u16,
    writing: bool,
) -> bool {
    let authority = format!("127.0.0.1:{port}");
    host == Some(authority.as_str())
        && (!writing
            || csrf == Some(token)
                && origin.is_none_or(|origin| origin == format!("http://{authority}")))
}

fn response(request: Request, status: u16, body: String, content_type: &str) {
    let mut result = Response::from_string(body).with_status_code(StatusCode(status));
    for (name, value) in [
        ("Content-Type", content_type),
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        ("X-Frame-Options", "DENY"),
    ] {
        if let Ok(header) = Header::from_bytes(name, value) {
            result.add_header(header);
        }
    }
    let _ = request.respond(result);
}

fn json_response(request: Request, status: u16, body: Value) {
    response(
        request,
        status,
        body.to_string(),
        "application/json; charset=utf-8",
    );
}

fn body(request: &mut Request) -> Result<Value> {
    let length = request.body_length().context("请求大小无效")?;
    anyhow::ensure!(length > 0 && length <= 12000, "请求大小无效");
    let mut buffer = String::new();
    request
        .as_reader()
        .take(12001)
        .read_to_string(&mut buffer)?;
    anyhow::ensure!(buffer.len() <= 12000, "请求大小无效");
    let body: Value = serde_json::from_str(&buffer)?;
    anyhow::ensure!(body.is_object(), "请求格式无效");
    Ok(body)
}

pub fn info(app: &App) -> Result<Value> {
    crate::service::info(&app.config)
}

pub fn handle(mut request: Request, app: &App, token: &str, port: u16) {
    let writing = request.method() != &Method::Get;
    if !authorized(
        header(&request, "Host"),
        header(&request, "Origin"),
        header(&request, "X-CSRF-Token"),
        token,
        port,
        writing,
    ) {
        return json_response(request, 403, json!({"error":"请从本地面板操作"}));
    }
    let path = request.url().split('?').next().unwrap_or("").to_string();
    if !writing && path == "/" {
        return response(
            request,
            200,
            include_str!("../index.html").replace("__CSRF_TOKEN__", token),
            "text/html; charset=utf-8",
        );
    }
    let result = if !writing && path == "/api/info" {
        info(app)
    } else if request.method() == &Method::Post {
        body(&mut request).and_then(|body| app.request(&path, Some(&body)))
    } else if !writing {
        app.request(&path, None)
    } else {
        bail_result()
    };
    match result {
        Ok(value) => json_response(
            request,
            if path == "/api/refresh" { 202 } else { 200 },
            value,
        ),
        Err(error) => json_response(request, 400, json!({"error":error.to_string()})),
    }
}

fn bail_result() -> Result<Value> {
    bail!("不支持的请求方法")
}

pub fn bind(port: u16) -> Result<(Server, u16)> {
    let server = Server::http(("127.0.0.1", port)).map_err(|error| anyhow::anyhow!("{error}"))?;
    let actual_port = server.server_addr().to_ip().context("服务地址无效")?.port();
    Ok((server, actual_port))
}

pub fn run(server: Server, port: u16, app: Arc<App>) -> Result<()> {
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let token = Arc::new(token);
    for request in server.incoming_requests() {
        let app = Arc::clone(&app);
        let token = Arc::clone(&token);
        std::thread::spawn(move || handle(request, &app, &token, port));
    }
    Ok(())
}

use std::io::Read;
