use anyhow::{Context, Result, ensure};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::blocking::Client;
use reqwest::{Method, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fmt;
use std::thread;
use std::time::Duration;

pub const API_VERSION: &str = "2025-09-03";
pub const MAX_PAYLOAD_BYTES: usize = 500_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotionError {
    pub category: String,
    pub method: String,
    pub status: Option<u16>,
    pub code: Option<String>,
    pub request_id: Option<String>,
    pub retry_after: Option<f64>,
    pub attempts: usize,
    pub retryable: bool,
    pub outcome_unknown: bool,
    pub rate_limit_reason: Option<String>,
    pub committed_resource_id: Option<String>,
    pub committed_child_ids: Vec<String>,
}

impl fmt::Display for NotionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Notion {}: {}", self.method, self.category)?;
        if let Some(status) = self.status {
            write!(formatter, " (HTTP {status})")?;
        }
        if self.outcome_unknown {
            write!(
                formatter,
                "; mutation outcome unknown; reconcile before resubmitting"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for NotionError {}

fn error(category: &str, method: &str, attempt: usize, read_only: bool) -> NotionError {
    NotionError {
        category: category.into(),
        method: method.into(),
        attempts: attempt,
        status: None,
        code: None,
        request_id: None,
        retry_after: None,
        retryable: read_only,
        outcome_unknown: !read_only,
        rate_limit_reason: None,
        committed_resource_id: None,
        committed_child_ids: Vec::new(),
    }
}

pub fn text_chunks(text: &str, chunk_size: usize) -> Result<Vec<String>> {
    ensure!(
        (1..=2000).contains(&chunk_size),
        "chunk_size must be between 1 and 2000"
    );
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut units = 0;
    for character in text.chars() {
        let width = character.len_utf16();
        ensure!(
            width <= chunk_size,
            "chunk_size is too small for this character"
        );
        if units + width > chunk_size {
            chunks.push(std::mem::take(&mut chunk));
            units = 0;
        }
        chunk.push(character);
        units += width;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    Ok(chunks)
}

pub fn rich_text(text: &str, chunk_size: usize) -> Result<Vec<Value>> {
    let chunks = text_chunks(text, chunk_size)?;
    ensure!(
        chunks.len() <= 100,
        "rich_text exceeds 100 fragments; use paragraph_blocks"
    );
    Ok(chunks
        .into_iter()
        .map(|content| json!({"type": "text", "text": {"content": content}}))
        .collect())
}

pub fn paragraph_blocks(text: &str, chunk_size: usize) -> Result<Vec<Value>> {
    Ok(text_chunks(text, chunk_size)?
        .into_iter()
        .map(|content| {
            json!({
                "object": "block", "type": "paragraph",
                "paragraph": {"rich_text": [{"type": "text", "text": {"content": content}}]}
            })
        })
        .collect())
}

pub fn plain_text(items: &Value) -> String {
    items
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item["plain_text"]
                        .as_str()
                        .or(item["text"]["content"].as_str())
                        .or(item["equation"]["expression"].as_str())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn property_text(value: &Value, name: Option<&str>) -> String {
    let property = name.map(|name| &value["properties"][name]).unwrap_or(value);
    let kind = property["type"]
        .as_str()
        .or_else(|| {
            ["title", "rich_text", "status", "select", "multi_select"]
                .into_iter()
                .find(|key| property.get(key).is_some())
        })
        .unwrap_or_default();
    match kind {
        "title" | "rich_text" => plain_text(&property[kind]),
        "status" | "select" => property[kind]["name"].as_str().unwrap_or_default().into(),
        "multi_select" => property[kind]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        "url" | "email" | "phone_number" => property[kind].as_str().unwrap_or_default().into(),
        "checkbox" => match property[kind].as_bool() {
            Some(true) => "True".into(),
            Some(false) => "False".into(),
            None => String::new(),
        },
        "number" => {
            if property[kind].is_null() {
                String::new()
            } else {
                property[kind].to_string()
            }
        }
        _ => String::new(),
    }
}

pub fn property_status(value: &Value, name: Option<&str>) -> String {
    let property = name.map(|name| &value["properties"][name]).unwrap_or(value);
    property["status"]["name"]
        .as_str()
        .unwrap_or_default()
        .into()
}

pub fn property_select(value: &Value, name: Option<&str>) -> String {
    let property = name.map(|name| &value["properties"][name]).unwrap_or(value);
    property["select"]["name"]
        .as_str()
        .unwrap_or_default()
        .into()
}

fn identifier(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
        "identifier must be a nonempty Notion ID"
    );
    Ok(value)
}

pub struct NotionClient {
    token: String,
    root: String,
    http: Client,
    pub data_source_id: String,
    pub api_version: String,
    pub executor_property: String,
    pub executor_property_type: String,
    pub status_property: String,
    pub status_property_type: String,
    pub max_retries: usize,
}

impl NotionClient {
    pub fn new(token: String, data_source_id: String) -> Result<Self> {
        ensure!(
            !token.is_empty()
                && token.is_ascii()
                && !token
                    .bytes()
                    .any(|byte| byte.is_ascii_whitespace() || byte < 32 || byte == 127),
            "Invalid Notion credential"
        );
        identifier(&data_source_id)?;
        Ok(Self {
            token,
            data_source_id,
            root: "https://api.notion.com/v1".into(),
            http: Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(30))
                .build()?,
            api_version: API_VERSION.into(),
            executor_property: "执行者".into(),
            executor_property_type: "select".into(),
            status_property: "状态".into(),
            status_property_type: "status".into(),
            max_retries: 3,
        })
    }

    pub fn configure(&mut self, config: &Value) -> Result<()> {
        for (key, destination) in [
            ("api_version", &mut self.api_version),
            ("executor_property", &mut self.executor_property),
            ("executor_property_type", &mut self.executor_property_type),
            ("status_property", &mut self.status_property),
            ("status_property_type", &mut self.status_property_type),
        ] {
            if let Some(value) = config[key].as_str() {
                ensure!(!value.is_empty(), "{key} cannot be empty");
                *destination = value.into();
            }
        }
        ensure!(
            NaiveDate::parse_from_str(&self.api_version, "%Y-%m-%d")?
                >= NaiveDate::from_ymd_opt(2025, 9, 3).unwrap(),
            "API version must be >= 2025-09-03"
        );
        ensure!(
            ["select", "multi_select", "status", "rich_text", "title"]
                .contains(&self.executor_property_type.as_str()),
            "Unsupported executor property type"
        );
        ensure!(
            ["select", "status"].contains(&self.status_property_type.as_str()),
            "Unsupported status property type"
        );
        if let Some(retries) = config["max_retries"].as_u64() {
            self.max_retries = retries.try_into()?;
        }
        if let Some(timeout) = config["timeout_seconds"].as_f64() {
            ensure!(
                timeout.is_finite() && timeout > 0.0,
                "Timeout must be finite and positive"
            );
            self.http = Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs_f64(timeout))
                .build()?;
        }
        Ok(())
    }

    pub fn request(
        &self,
        method: Method,
        path: &str,
        payload: Option<&Value>,
        read_only: bool,
    ) -> Result<Value> {
        let read_only = read_only || method == Method::GET;
        let bytes = payload.map(serde_json::to_vec).transpose()?;
        ensure!(
            bytes
                .as_ref()
                .is_none_or(|value| value.len() <= MAX_PAYLOAD_BYTES),
            "Request exceeds Notion's 500 KB payload limit"
        );
        for attempt in 1..=self.max_retries + 1 {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{path}", self.root))
                .bearer_auth(&self.token)
                .header("Notion-Version", &self.api_version)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json");
            if let Some(bytes) = &bytes {
                request = request.body(bytes.clone());
            }
            let failure = match request.send() {
                Err(transport) => error(
                    if transport.is_timeout() {
                        "timeout"
                    } else {
                        "network"
                    },
                    method.as_str(),
                    attempt,
                    read_only,
                ),
                Ok(response) => {
                    let status = response.status().as_u16();
                    let retry_after = response
                        .headers()
                        .get("Retry-After")
                        .and_then(|header| header.to_str().ok())
                        .and_then(|header| {
                            header
                                .parse::<f64>()
                                .ok()
                                .filter(|value| value.is_finite())
                                .map(|value| value.max(0.0))
                                .or_else(|| {
                                    DateTime::parse_from_rfc2822(header).ok().map(|date| {
                                        ((date.timestamp() - Utc::now().timestamp()) as f64)
                                            .max(0.0)
                                    })
                                })
                        });
                    let body: Option<Value> = response.json().ok();
                    if (200..300).contains(&status) {
                        let valid = body.filter(Value::is_object).ok_or_else(|| {
                            let mut failure =
                                error("invalid_response", method.as_str(), attempt, read_only);
                            failure.retryable = false;
                            failure
                        })?;
                        return Ok(valid);
                    }
                    let body = body.unwrap_or(Value::Null);
                    let mut failure = error(
                        match status {
                            400 => "validation",
                            401 => "authentication",
                            403 => "permission",
                            404 => "not_found",
                            408 => "timeout",
                            409 => "conflict",
                            429 => "rate_limited",
                            529 => "overloaded",
                            300..400 => "redirect",
                            500.. => "server",
                            _ => "http",
                        },
                        method.as_str(),
                        attempt,
                        read_only,
                    );
                    let codes = [
                        "invalid_json",
                        "invalid_request_url",
                        "invalid_request",
                        "invalid_grant",
                        "validation_error",
                        "missing_version",
                        "invalid_beta",
                        "unauthorized",
                        "restricted_resource",
                        "object_not_found",
                        "conflict_error",
                        "rate_limited",
                        "internal_server_error",
                        "bad_gateway",
                        "service_unavailable",
                        "database_connection_unavailable",
                        "gateway_timeout",
                        "service_overload",
                    ];
                    failure.code = body["code"]
                        .as_str()
                        .filter(|code| codes.contains(code))
                        .map(String::from);
                    failure.status = Some(status);
                    failure.retry_after = retry_after;
                    failure.request_id = body["request_id"]
                        .as_str()
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                        .map(|value| value.to_string());
                    let extra = &body["additional_data"];
                    let reasons = [
                        "public_api_request_rate_limit",
                        "public_api_space_request_rate_limit",
                        "public_api_endpoint_rate_limit",
                        "mcp_tool_rate_limit",
                        "collection_router_too_many_requests",
                        "collection_router_upstream_429",
                        "public_api_request_blocked",
                    ];
                    failure.rate_limit_reason = extra["rate_limit_reason"]
                        .as_str()
                        .filter(|value| reasons.contains(value))
                        .map(String::from);
                    failure.committed_resource_id = extra["committed_resource_id"]
                        .as_str()
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                        .map(|value| value.to_string());
                    failure.committed_child_ids = extra["committed_child_ids"]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .filter_map(|value| uuid::Uuid::parse_str(value).ok())
                                .map(|value| value.to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    let rejected = status == 429
                        || (status == 529 && failure.code.as_deref() == Some("service_overload"));
                    failure.retryable = (rejected
                        && failure.rate_limit_reason.as_deref()
                            != Some("public_api_request_blocked"))
                        || (read_only && (status == 408 || status == 409 || status >= 500));
                    failure.outcome_unknown = !read_only
                        && !rejected
                        && ((300..400).contains(&status) || status == 408 || status >= 500);
                    failure
                }
            };
            if failure.retryable && attempt <= self.max_retries {
                let delay = failure
                    .retry_after
                    .unwrap_or(0.0)
                    .max(2.0_f64.powi((attempt - 1).min(5) as i32).min(30.0));
                thread::sleep(Duration::from_secs_f64(delay));
            } else {
                return Err(failure.into());
            }
        }
        unreachable!()
    }

    pub fn paginate(&self, method: Method, path: &str, payload: &Value) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen = HashSet::new();
        loop {
            let mut params = json!({"page_size": 100});
            if let Some(cursor) = &cursor {
                params["start_cursor"] = json!(cursor);
            }
            let response = if method == Method::GET {
                let mut address = url::Url::parse(&format!("{}{path}", self.root))?;
                address.query_pairs_mut().append_pair("page_size", "100");
                if let Some(cursor) = &cursor {
                    address
                        .query_pairs_mut()
                        .append_pair("start_cursor", cursor);
                }
                self.request(
                    Method::GET,
                    &format!("{path}?{}", address.query().unwrap_or_default()),
                    None,
                    true,
                )?
            } else {
                let object = params.as_object_mut().unwrap();
                if let Some(fields) = payload.as_object() {
                    for (key, value) in fields {
                        object.insert(key.clone(), value.clone());
                    }
                }
                self.request(method.clone(), path, Some(&params), true)?
            };
            ensure!(
                response["request_status"].is_null() || response["request_status"].is_object(),
                "Invalid query status"
            );
            ensure!(
                response["request_status"]["type"] != "incomplete",
                "Notion query is incomplete; narrow the filter"
            );
            let batch = response["results"]
                .as_array()
                .context("Invalid pagination results")?;
            ensure!(
                batch.iter().all(Value::is_object),
                "Invalid pagination item"
            );
            let more = response["has_more"]
                .as_bool()
                .context("Invalid pagination flag")?;
            result.extend(batch.iter().cloned());
            if !more {
                return Ok(result);
            }
            let next = response["next_cursor"]
                .as_str()
                .filter(|value| !value.is_empty())
                .context("Missing pagination cursor")?;
            ensure!(seen.insert(next.to_string()), "Pagination cursor loop");
            cursor = Some(next.to_string());
        }
    }

    pub fn query_tasks(
        &self,
        executor: Option<&str>,
        statuses: Option<&[String]>,
    ) -> Result<Vec<Value>> {
        let mut filters = Vec::new();
        if let Some(executor) = executor {
            ensure!(!executor.is_empty(), "Empty executor");
            let operator = if self.executor_property_type == "multi_select" {
                "contains"
            } else {
                "equals"
            };
            filters.push(json!({"property": self.executor_property, &self.executor_property_type: {operator: executor}}));
        }
        if let Some(statuses) = statuses {
            if statuses.is_empty() {
                return Ok(Vec::new());
            }
            let mut seen = HashSet::new();
            let choices: Vec<_> = statuses.iter().filter(|value| seen.insert((*value).clone())).map(|status|
                json!({"property": self.status_property, &self.status_property_type: {"equals": status}})).collect();
            ensure!(
                choices.len() <= 100 && !statuses.iter().any(String::is_empty),
                "Invalid status filter"
            );
            filters.push(if choices.len() == 1 {
                choices[0].clone()
            } else {
                json!({"or": choices})
            });
        }
        let payload = match filters.len() {
            0 => json!({}),
            1 => json!({"filter": filters[0]}),
            _ => json!({"filter": {"and": filters}}),
        };
        self.paginate(
            Method::POST,
            &format!("/data_sources/{}/query", self.data_source_id),
            &payload,
        )
    }

    pub fn get_page(&self, pid: &str) -> Result<Value> {
        self.request(
            Method::GET,
            &format!("/pages/{}", identifier(pid)?),
            None,
            true,
        )
    }

    pub fn get_blocks(&self, pid: &str) -> Result<Vec<Value>> {
        self.paginate(
            Method::GET,
            &format!("/blocks/{}/children", identifier(pid)?),
            &json!({}),
        )
    }

    pub fn get_task_text(&self, pid: &str) -> Result<String> {
        let mut stack: Vec<_> = self
            .get_blocks(pid)?
            .into_iter()
            .rev()
            .map(|block| (block, 0usize, HashSet::from([pid.to_string()])))
            .collect();
        let mut lines = Vec::new();
        while let Some((block, depth, ancestors)) = stack.pop() {
            ensure!(depth < 256, "Block tree nesting limit");
            let kind = block["type"].as_str().unwrap_or("unsupported");
            let body = &block[kind];
            ensure!(body.is_null() || body.is_object(), "Invalid block");
            let mut text = plain_text(&body["rich_text"]);
            match kind {
                "heading_1" => text = format!("# {text}"),
                "heading_2" => text = format!("## {text}"),
                "heading_3" => text = format!("### {text}"),
                "heading_4" => text = format!("#### {text}"),
                "bulleted_list_item" | "toggle" => text = format!("- {text}"),
                "numbered_list_item" => text = format!("1. {text}"),
                "to_do" => {
                    text = format!(
                        "- [{}] {text}",
                        if body["checked"] == true { "x" } else { " " }
                    )
                }
                "quote" => text = format!("> {text}"),
                "code" => {
                    let longest = text
                        .split(|character| character != '`')
                        .map(str::len)
                        .max()
                        .unwrap_or(0);
                    let fence = "`".repeat((longest + 1).max(3));
                    text = format!(
                        "{fence}{}\n{text}\n{fence}",
                        body["language"].as_str().unwrap_or_default()
                    );
                }
                "equation" => text = body["expression"].as_str().unwrap_or_default().into(),
                "table_row" => {
                    text = format!(
                        "| {} |",
                        body["cells"]
                            .as_array()
                            .map(|cells| cells
                                .iter()
                                .map(plain_text)
                                .collect::<Vec<_>>()
                                .join(" | "))
                            .unwrap_or_default()
                    )
                }
                "divider" => text = "---".into(),
                "child_page" | "child_database" => {
                    text = body["title"].as_str().unwrap_or_default().into()
                }
                _ if text.is_empty() => {
                    text = plain_text(&body["caption"]);
                    if text.is_empty() {
                        text = body["url"]
                            .as_str()
                            .or(body["name"].as_str())
                            .unwrap_or_default()
                            .into();
                    }
                    if kind == "unsupported" {
                        text = "[unsupported block]".into();
                    }
                }
                _ => {}
            }
            if !text.is_empty() {
                lines.extend(
                    text.split('\n')
                        .map(|line| format!("{}{line}", "  ".repeat(depth))),
                );
            }
            if block["has_children"] == true && !["child_page", "child_database"].contains(&kind) {
                let id = block["id"].as_str().context("Child block has no ID")?;
                ensure!(!ancestors.contains(id), "Block tree cycle");
                let mut next_ancestors = ancestors.clone();
                next_ancestors.insert(id.to_string());
                stack.extend(
                    self.get_blocks(id)?
                        .into_iter()
                        .rev()
                        .map(|child| (child, depth + 1, next_ancestors.clone())),
                );
            }
        }
        Ok(lines.join("\n"))
    }

    pub fn update_properties(&self, pid: &str, properties: &Value) -> Result<Value> {
        ensure!(properties.is_object(), "Properties must be an object");
        self.request(
            Method::PATCH,
            &format!("/pages/{}", identifier(pid)?),
            Some(&json!({"properties": properties})),
            false,
        )
    }

    pub fn create_page(&self, properties: &Value, children: Option<&[Value]>) -> Result<Value> {
        ensure!(properties.is_object(), "Properties must be an object");
        let mut payload = json!({"parent": {"type": "data_source_id", "data_source_id": self.data_source_id}, "properties": properties});
        if let Some(children) = children {
            ensure!(
                children.len() <= 100 && children.iter().all(Value::is_object),
                "Creation supports 0..100 block objects"
            );
            payload["children"] = json!(children);
        }
        self.request(Method::POST, "/pages", Some(&payload), false)
    }

    pub fn append_blocks(&self, pid: &str, children: &[Value]) -> Result<Value> {
        ensure!(
            !children.is_empty() && children.len() <= 100 && children.iter().all(Value::is_object),
            "Append supports 1..100 block objects"
        );
        self.request(
            Method::PATCH,
            &format!("/blocks/{}/children", identifier(pid)?),
            Some(&json!({"children": children})),
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn unicode_chunks_preserve_utf16_limits() -> Result<()> {
        assert_eq!(text_chunks("a😀中文", 3)?, ["a😀", "中文"]);
        assert!(text_chunks("😀", 1).is_err());
        assert!(rich_text(&"a".repeat(201), 2).is_err());
        assert_eq!(paragraph_blocks("a\nb", 2)?.len(), 2);
        Ok(())
    }

    fn server(response: &'static str) -> Result<(String, thread::JoinHandle<()>)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let handle = thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = connection.read(&mut buffer);
            connection.write_all(response.as_bytes()).unwrap();
        });
        Ok((format!("http://{address}"), handle))
    }

    #[test]
    fn never_retries_unknown_mutation_or_follows_redirect() -> Result<()> {
        for response in [
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/steal\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        ] {
            let (address, handle) = server(response)?;
            let mut client = NotionClient::new("test-secret".into(), "source".into())?;
            client.root = address;
            let failure = client.update_properties("page", &json!({})).unwrap_err();
            let failure = failure.downcast_ref::<NotionError>().unwrap();
            assert!(failure.outcome_unknown);
            assert!(!failure.retryable);
            assert_eq!(failure.attempts, 1);
            assert!(!failure.to_string().contains("test-secret"));
            handle.join().unwrap();
        }
        Ok(())
    }

    #[test]
    fn rejects_partial_query_results() -> Result<()> {
        let body = r#"{"results":[],"has_more":false,"request_status":{"type":"incomplete"}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let response: &'static str = Box::leak(response.into_boxed_str());
        let (address, handle) = server(response)?;
        let mut client = NotionClient::new("token".into(), "source".into())?;
        client.root = address;
        assert!(client.query_tasks(None, None).is_err());
        handle.join().unwrap();
        Ok(())
    }
}
