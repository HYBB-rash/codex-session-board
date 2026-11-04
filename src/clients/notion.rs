use super::rpc::RpcFailure;
use super::{ExternalError, Notion, RpcClient, external, required, truthy};
use crate::common::canonical_id;
use anyhow::{Context, Result, ensure};
use lazy_regex::regex;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use url::Url;

const COLUMNS: [&str; 6] = [
    "url",
    "任务名称",
    "状态",
    "执行人 / 代理",
    "AI 会话 / 线程",
    "模型",
];
const MARKDOWN_SPEC: &str = "notion://docs/notion-flavored-markdown-spec";
const PAGE_SIZE: usize = 50;
const MAX_QUERY_PAGES: usize = 100;

pub(super) fn unescape(text: &str) -> String {
    regex!(r"&(#x[0-9a-fA-F]+|#X[0-9a-fA-F]+|#[0-9]+|[A-Za-z][A-Za-z0-9]+);")
        .replace_all(text, |capture: &regex::Captures<'_>| {
            let entity = &capture[1];
            let number = if entity.starts_with("#x") || entity.starts_with("#X") {
                u32::from_str_radix(&entity[2..], 16).ok()
            } else if let Some(digits) = entity.strip_prefix('#') {
                digits.parse().ok()
            } else {
                None
            };
            if let Some(number) = number {
                return char::from_u32(number).unwrap_or('\u{fffd}').to_string();
            }
            match entity {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" => "'",
                "nbsp" => "\u{a0}",
                "ndash" => "–",
                "mdash" => "—",
                "hellip" => "…",
                "copy" => "©",
                "reg" => "®",
                "trade" => "™",
                "laquo" => "«",
                "raquo" => "»",
                "lsquo" => "‘",
                "rsquo" => "’",
                "ldquo" => "“",
                "rdquo" => "”",
                _ => &capture[0],
            }
            .to_owned()
        })
        .into_owned()
}

fn attributes(text: &str) -> HashMap<String, String> {
    regex!(r#"([\w-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
        .captures_iter(text)
        .filter_map(|capture| {
            let value = capture.get(2).or_else(|| capture.get(3))?.as_str();
            Some((capture[1].to_owned(), unescape(value)))
        })
        .collect()
}

fn mask(output: &mut String, start: usize, end: usize) {
    let replacement: String = output[start..end]
        .bytes()
        .map(|byte| if byte == b'\n' { '\n' } else { ' ' })
        .collect();
    output.replace_range(start..end, &replacement);
}

fn visible_markup(text: &str) -> String {
    let mut output = text.to_owned();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let marker = trimmed.as_bytes().first().copied();
        let marker = marker.filter(|byte| matches!(byte, b'`' | b'~'));
        let width = marker.map_or(0, |marker| {
            trimmed
                .as_bytes()
                .iter()
                .take_while(|byte| **byte == marker)
                .count()
        });
        if let Some((expected, minimum)) = fence {
            mask(&mut output, offset, offset + line.len());
            if marker == Some(expected) && width >= minimum {
                fence = None;
            }
        } else if let Some(marker) = marker.filter(|_| width >= 3) {
            fence = Some((marker, width));
            mask(&mut output, offset, offset + line.len());
        }
        offset += line.len();
    }
    let fenced = output.clone();
    for matched in regex!(r"<code\b[^>]*>[\s\S]*?</code>|`+[^`\n]*`+").find_iter(&fenced) {
        mask(&mut output, matched.start(), matched.end());
    }
    output
}

struct PageParts {
    properties: Value,
    content: String,
    ancestry: String,
}

fn page_parts(text: &str, operation: &str) -> Result<PageParts> {
    let fail = |kind| external(operation, kind, false, false);
    let opening = regex!(r"(?m)^<page\b[^>]*>")
        .find(text)
        .ok_or_else(|| fail("incomplete_page_markup"))?;
    let property_expression = regex!(r"<properties\b[^>]*>");
    let properties = property_expression
        .find(&text[opening.end()..])
        .ok_or_else(|| fail("incomplete_page_markup"))?;
    let start = opening.end() + properties.end();
    let payload = &text[start..];
    let mut decoder = serde_json::Deserializer::from_str(payload).into_iter::<Value>();
    let properties_value = decoder
        .next()
        .and_then(std::result::Result::ok)
        .filter(Value::is_object)
        .ok_or_else(|| fail("invalid_page_properties"))?;
    let end = start + decoder.byte_offset();
    let closing = regex!(r"^\s*</properties>")
        .find(&text[end..])
        .ok_or_else(|| fail("incomplete_page_markup"))?;
    let content = page_content(&text[end + closing.end()..], opening.as_str(), operation)?;
    Ok(PageParts {
        properties: properties_value,
        content,
        ancestry: text[opening.end()..opening.end() + properties.start()].to_owned(),
    })
}

fn page_content(body: &str, opening: &str, operation: &str) -> Result<String> {
    let fail = |kind| external(operation, kind, false, false);
    let visible = visible_markup(body);
    let boundary = regex!(r"(?m)^</page>[ \t]*(?:\n|$)")
        .find(&visible)
        .ok_or_else(|| fail("incomplete_page_markup"))?;
    let body = &body[..boundary.start()];
    let visible = &visible[..boundary.start()];
    let content_open = regex!(r"<content\b[^>]*>").find(visible);
    let content_close = regex!(r"</content>\s*$").find(visible);
    let content = match (content_open, content_close) {
        (Some(start), Some(end)) if end.start() >= start.end() => {
            body[start.end()..end.start()].to_owned()
        }
        _ if regex!(r"<content\s*/>\s*$").is_match(visible) => String::new(),
        _ => return Err(fail("incomplete_page_markup")),
    };
    if truncated_markup(&format!("{opening}\n{visible}")) {
        return Err(fail("truncated_response"));
    }
    Ok(content)
}

fn truncated_markup(text: &str) -> bool {
    regex!(r#"(?i)<(?:page|content)\b[^>]*\btruncated\s*=\s*["']true["']|<truncated\b"#)
        .is_match(text)
}

fn source_id(value: &str) -> Result<String> {
    let source = value.strip_prefix("collection://").unwrap_or(value);
    ensure!(
        regex!(r"^[A-Za-z0-9_-]{1,128}$").is_match(source),
        "Data source must be a simple Notion identifier"
    );
    Ok(source.to_owned())
}

fn same_id(left: &str, right: &str) -> bool {
    left.replace('-', "")
        .eq_ignore_ascii_case(&right.replace('-', ""))
}

fn page_argument(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty() && value.trim() == value,
        "Invalid page identifier"
    );
    if value.starts_with("https://") {
        let url = Url::parse(value)?;
        let host = url.host_str().context("Page URL requires a host")?;
        ensure!(
            matches!(host, "notion.so" | "www.notion.so" | "app.notion.com")
                || host.ends_with(".notion.site"),
            "Unexpected Notion page URL host"
        );
        let tail = url
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        let identifier = regex!(r"([0-9a-fA-F]{32}|[0-9a-fA-F-]{36})$")
            .find(tail)
            .context("Page URL requires a page ID")?
            .as_str()
            .to_owned();
        return Ok(identifier);
    }
    ensure!(
        regex!(r"^[A-Za-z0-9_-]{1,128}$").is_match(value),
        "Page ID must be a simple Notion identifier"
    );
    Ok(value.to_owned())
}

fn metadata_id(value: &str) -> Result<String> {
    if value.starts_with("https://") {
        let url = Url::parse(value)?;
        ensure!(
            matches!(
                url.host_str(),
                Some("notion.so" | "www.notion.so" | "app.notion.com")
            ),
            "Unexpected metadata URL host"
        );
        let tail = url
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        let identifier = regex!(
            r"(?:^|-)([0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})$"
        )
        .captures(tail)
        .context("Metadata URL requires a page UUID")?[1]
            .to_owned();
        return canonical_id(&identifier);
    }
    ensure!(
        regex!(r"^(?:[0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})$")
            .is_match(value),
        "Metadata page identity must be a complete UUID"
    );
    canonical_id(value)
}

fn template_allowance(raw: &Value, config: &Value) -> Option<Value> {
    let identifier = raw["id"].as_str()?;
    if let Some(allowance) = config["known_template_blocks"].get(identifier) {
        return allowance.is_object().then(|| allowance.clone());
    }
    let profile = config.get("template_block_profile")?;
    let parts = page_parts(raw["text"].as_str()?, "fetch").ok()?;
    let parent = regex!(r"<parent-data-source\b[^>]*>").find(&parts.ancestry)?;
    let parent_attributes = attributes(parent.as_str());
    if parent_attributes.get("url")?
        != &format!("collection://{}", profile["data_source_id"].as_str()?)
        || parts.properties["执行人 / 代理"] != profile["executor"]
    {
        return None;
    }
    let visible = visible_markup(&parts.content);
    let headings = template_headings(&visible, profile)?;
    let observed = template_observed(&visible, identifier, profile, &headings)?;
    let ids = raw["unknown_block_ids"].as_array()?;
    if ids.len() != observed.len() {
        return None;
    }
    let mut allowance = Map::new();
    for identifier in ids {
        let identifier = identifier.as_str()?;
        allowance.insert(
            identifier.to_owned(),
            observed.get(&identifier.replace('-', ""))?.clone(),
        );
    }
    Some(Value::Object(allowance))
}

fn template_headings(visible: &str, profile: &Value) -> Option<Vec<(usize, String)>> {
    let headings: Vec<_> = regex!(r"(?m)^## ([^\n]+?)\s*$")
        .captures_iter(visible)
        .filter_map(|capture| Some((capture.get(0)?.end(), capture[1].to_owned())))
        .collect();
    let required = profile["required_sections"].as_array()?;
    let mut previous = None;
    for name in required {
        let name = name.as_str()?;
        let positions: Vec<_> = headings
            .iter()
            .enumerate()
            .filter(|(_, (_, heading))| heading == name)
            .map(|(position, _)| position)
            .collect();
        if positions.len() != 1 || previous.is_some_and(|previous| previous >= positions[0]) {
            return None;
        }
        previous = Some(positions[0]);
    }
    Some(headings)
}

fn template_block_id(url: &Url, identifier: &str) -> Option<String> {
    let block = url.fragment()?.replace('-', "");
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("app.notion.com" | "www.notion.so" | "notion.so")
        )
        || url
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()?
            .replace('-', "")
            != identifier.replace('-', "")
        || !regex!(r"^[0-9a-f]{32}$").is_match(&block)
    {
        return None;
    }
    Some(block)
}

fn template_observed(
    visible: &str,
    identifier: &str,
    profile: &Value,
    headings: &[(usize, String)],
) -> Option<Map<String, Value>> {
    let mut counts: Map<String, Value> = profile["sections"]
        .as_object()?
        .keys()
        .map(|name| (name.clone(), json!({})))
        .collect();
    let mut observed = Map::new();
    for matched in regex!(r"<unknown\b[^>]*/>").find_iter(visible) {
        let attrs = attributes(matched.as_str());
        let url = Url::parse(attrs.get("url")?).ok()?;
        let block = template_block_id(&url, identifier)?;
        let section = &headings
            .iter()
            .rev()
            .find(|(end, _)| *end <= matched.start())?
            .1;
        if observed.contains_key(&block) {
            return None;
        }
        let alt = attrs.get("alt")?;
        let section_counts = counts.get_mut(section)?.as_object_mut()?;
        let count = section_counts.get(alt).and_then(Value::as_u64).unwrap_or(0);
        section_counts.insert(alt.clone(), json!(count + 1));
        observed.insert(block, json!(alt));
    }
    if Value::Object(counts) != profile["sections"] {
        return None;
    }
    Some(observed)
}

fn known_template_omissions(raw: &Value, config: &Value) -> bool {
    let Some(identifier) = raw["id"].as_str() else {
        return false;
    };
    let Some(allowed) = template_allowance(raw, config) else {
        return false;
    };
    let Some(ids) = raw["unknown_block_ids"].as_array() else {
        return false;
    };
    let Some(allowed) = allowed.as_object() else {
        return false;
    };
    if ids.is_empty()
        || raw["unknown_block_count"].as_u64() != Some(ids.len() as u64)
        || ids.len() != allowed.len()
    {
        return false;
    }
    let Some(expected) = expected_omissions(ids, allowed) else {
        return false;
    };
    let Some(text) = raw["text"].as_str() else {
        return false;
    };
    let Ok(parts) = page_parts(text, "fetch") else {
        return false;
    };
    observed_omissions(&parts.content, identifier).is_some_and(|observed| observed == expected)
}

fn expected_omissions(
    ids: &[Value],
    allowed: &Map<String, Value>,
) -> Option<HashMap<String, String>> {
    let mut expected = HashMap::new();
    let mut exact = HashSet::new();
    for identifier in ids {
        let identifier = identifier.as_str()?;
        let alt = allowed.get(identifier).and_then(Value::as_str)?;
        if !exact.insert(identifier) {
            return None;
        }
        expected.insert(identifier.replace('-', ""), alt.to_owned());
    }
    Some(expected)
}

fn observed_omissions(content: &str, identifier: &str) -> Option<HashMap<String, String>> {
    let mut observed = HashMap::new();
    for matched in regex!(r"<unknown\b[^>]*/>").find_iter(&visible_markup(content)) {
        let attrs = attributes(matched.as_str());
        let url = Url::parse(attrs.get("url")?).ok()?;
        let block = url.fragment().unwrap_or("").replace('-', "");
        let alt = attrs.get("alt")?;
        if !url
            .path()
            .replace('-', "")
            .contains(&identifier.replace('-', ""))
            || observed.insert(block, alt.clone()).is_some()
        {
            return None;
        }
    }
    Some(observed)
}

fn complete(value: &Value, operation: &str, config: Option<&Value>, depth: usize) -> Result<()> {
    if depth > 16 {
        return Err(external(operation, "response_nesting_limit", false, false));
    }
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    if truthy(&value["isError"]) {
        return Err(external(operation, "tool_error", false, false));
    }
    let known = config.is_some_and(|config| known_template_omissions(value, config));
    for key in ["truncated", "is_truncated"] {
        let value = &value[key];
        let harmless = value.is_null() || value == false || value == 0 || value == "false";
        if !harmless && !(key == "truncated" && known) {
            return Err(external(operation, "truncated_response", false, false));
        }
    }
    if !(value["unknown_block_count"].is_null()
        || value["unknown_block_count"] == 0
        || value["unknown_block_count"] == "0")
        && !known
        || truthy(&value["unknown_block_ids"]) && !known
    {
        return Err(external(operation, "omitted_blocks", false, false));
    }
    for key in [
        "result",
        "structuredContent",
        "metadata",
        "truncation",
        "pagination",
    ] {
        if let Some(child) = object.get(key) {
            complete(child, operation, config, depth + 1)?;
        }
    }
    Ok(())
}

fn rpc_reason(payload: &Value) -> Option<&'static str> {
    let text = payload["message"].as_str()?.to_lowercase();
    if regex!(r"\b(?:401|403|451)\b").is_match(&text)
        || ["unauthorized", "forbidden", "authentication required"]
            .iter()
            .any(|word| text.contains(word))
    {
        return None;
    }
    if text.contains("mcp startup failed") && text.contains("timed out awaiting tools/list") {
        return Some("mcp_startup_timeout");
    }
    if payload["code"] == -32603
        && text.contains("transport")
        && (text.contains("http request failed") || text.contains("error sending request"))
    {
        return Some("mcp_transport_error");
    }
    None
}

fn unwrap(mut value: Value, operation: &str, config: Option<&Value>) -> Result<Value> {
    for _ in 0..16 {
        complete(&value, operation, config, 0)?;
        if let Some(text) = value.as_str() {
            match serde_json::from_str(text) {
                Ok(parsed) => value = parsed,
                Err(_) if text.trim_start().starts_with(['{', '[']) => {
                    return Err(external(operation, "invalid_json", false, false));
                }
                Err(_) => return Ok(json!({"text": text})),
            }
            continue;
        }
        if value.is_array() {
            return Ok(value);
        }
        if !value.is_object() {
            return Err(external(operation, "invalid_response", false, false));
        }
        if !value["error"].is_null() {
            let reason = rpc_reason(&value["error"]);
            return Err(external(
                operation,
                reason.unwrap_or("rpc_error"),
                false,
                reason.is_some(),
            ));
        }
        if value.get("result").is_some() {
            value = value["result"].take();
        } else if !value["structuredContent"].is_null() {
            value = value["structuredContent"].take();
        } else if let Some(content) = value["content"].as_array() {
            value = envelope_text(content, operation)?;
        } else {
            return Ok(value);
        }
    }
    Err(external(operation, "response_nesting_limit", false, false))
}

fn envelope_text(content: &[Value], operation: &str) -> Result<Value> {
    let fail = || external(operation, "unsupported_content_envelope", false, false);
    if content.is_empty()
        || content
            .iter()
            .any(|part| part["type"] != "text" || !part["text"].is_string())
    {
        return Err(fail());
    }
    let mut text = String::new();
    for part in content {
        complete(part, operation, None, 0)?;
        text.push_str(part["text"].as_str().ok_or_else(fail)?);
    }
    Ok(json!(text))
}

fn text<'a>(value: &'a Value, operation: &str) -> Result<&'a str> {
    let text = value
        .get("text")
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| external(operation, "missing_text", false, false))?;
    if operation != "fetch" && truncated_markup(&visible_markup(text)) {
        return Err(external(operation, "truncated_response", false, false));
    }
    Ok(text)
}

fn rows(mut value: Value, source: &str, columns: &[&str]) -> Result<Vec<Value>> {
    let (batch, names) = query_row_envelope(&mut value, source)?;
    let column_names = query_column_names(&names)?;
    batch
        .iter()
        .map(|row| metadata_row(row, &column_names, columns))
        .collect()
}

fn validate_query_source(value: &Value, source: &str) -> Result<()> {
    complete(value, "query", None, 0)?;
    if truthy(&value["has_more"]) {
        return Err(external(
            "query",
            "incomplete_metadata_result",
            false,
            false,
        ));
    }
    if let Some(sources) = value.get("data_source_ids") {
        let valid = sources.as_array().is_some_and(|sources| {
            sources.len() == 1
                && sources[0]
                    .as_str()
                    .is_some_and(|item| same_id(item, source))
        });
        if !valid {
            return Err(external(
                "query",
                "unexpected_query_data_source",
                false,
                false,
            ));
        }
    }
    Ok(())
}

fn query_row_envelope<'a>(value: &'a mut Value, source: &str) -> Result<(&'a Vec<Value>, Value)> {
    let mut names = Value::Null;
    for depth in 0..8 {
        validate_query_source(value, source)?;
        if let Some(list) = value.as_array() {
            if list.len() == 1
                && list[0].is_object()
                && list[0].get("url").is_none()
                && ["rows", "results", "data"]
                    .iter()
                    .any(|key| list[0].get(key).is_some())
            {
                *value = list[0].clone();
                continue;
            }
            break;
        }
        if !value.is_object() {
            return Err(external("query", "invalid_rows", false, false));
        }
        if value.get("columns").is_some() {
            names = value["columns"].clone();
        }
        let key = ["rows", "results", "data"]
            .into_iter()
            .find(|key| value.get(key).is_some())
            .ok_or_else(|| external("query", "missing_rows", false, false))?;
        *value = value[key].take();
        if depth == 7 {
            return Err(external("query", "response_nesting_limit", false, false));
        }
    }
    let batch = value
        .as_array()
        .filter(|batch| batch.len() <= PAGE_SIZE)
        .ok_or_else(|| external("query", "invalid_rows", false, false))?;
    Ok((batch, names))
}

fn query_column_names(names: &Value) -> Result<Vec<String>> {
    let mut column_names = Vec::new();
    if !names.is_null() {
        let names = names
            .as_array()
            .ok_or_else(|| external("query", "invalid_columns", false, false))?;
        let mut seen = HashSet::new();
        for name in names {
            let name = if name.is_object() {
                &name["name"]
            } else {
                name
            };
            let name = name
                .as_str()
                .ok_or_else(|| external("query", "invalid_columns", false, false))?;
            if !seen.insert(name) {
                return Err(external("query", "invalid_columns", false, false));
            }
            column_names.push(name.to_owned());
        }
    }
    Ok(column_names)
}

fn metadata_row(row: &Value, column_names: &[String], columns: &[&str]) -> Result<Value> {
    let row = if let Some(values) = row.as_array() {
        if column_names.is_empty() || values.len() != column_names.len() {
            return Err(external("query", "invalid_rows", false, false));
        }
        Value::Object(
            column_names
                .iter()
                .cloned()
                .zip(values.iter().cloned())
                .collect(),
        )
    } else {
        row.clone()
    };
    let object = row
        .as_object()
        .ok_or_else(|| external("query", "invalid_rows", false, false))?;
    let mut selected = Map::new();
    for column in columns {
        let value = object
            .get(*column)
            .ok_or_else(|| external("query", "missing_metadata_column", false, false))?;
        if !value.is_null() && !value.is_string() {
            return Err(external("query", "invalid_metadata_column", false, false));
        }
        selected.insert((*column).to_owned(), value.clone());
    }
    let url = selected["url"]
        .as_str()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| external("query", "invalid_row_url", false, false))?;
    metadata_id(url).map_err(|_| external("query", "invalid_row_url", false, false))?;
    Ok(Value::Object(selected))
}

fn tool_request_error(error: &anyhow::Error, role: &str, write: bool) -> anyhow::Error {
    let reason = error
        .downcast_ref::<RpcFailure>()
        .and_then(|failure| rpc_reason(&failure.response["error"]));
    let upstream = error.downcast_ref::<ExternalError>();
    let kind = reason
        .or_else(|| upstream.map(|error| error.kind.as_str()))
        .unwrap_or("request_failed");
    let read_reconnectable = reason.is_some()
        || upstream.is_some_and(|error| {
            error.kind.starts_with("connection") || error.kind.starts_with("timeout")
        });
    external(role, kind, write, read_reconnectable)
}

fn metadata_query(identifier: &str, source: &str, columns: &[&str]) -> Value {
    let mut urls = Vec::new();
    for prefix in [
        "https://app.notion.com/",
        "https://app.notion.com/p/",
        "https://notion.so/",
        "https://www.notion.so/",
    ] {
        for value in [identifier.to_owned(), identifier.replace('-', "")] {
            urls.push(format!("{prefix}{value}"));
        }
    }
    let query = format!(
        "SELECT {} FROM \"collection://{}\" WHERE url IN ({}) LIMIT 2",
        columns
            .iter()
            .map(|column| format!("\"{column}\""))
            .collect::<Vec<_>>()
            .join(", "),
        source,
        vec!["?"; urls.len()].join(", ")
    );
    json!({"data": {
        "mode": "sql", "data_source_urls": [format!("collection://{source}")],
        "query": query, "params": urls
    }})
}

fn task_source(parts: &PageParts, config: &Value) -> Result<String> {
    let parent = regex!(r"<parent-data-source\b[^>]*>")
        .find(&parts.ancestry)
        .ok_or_else(|| external("fetch", "incomplete_page_markup", false, false))?;
    let attrs = attributes(parent.as_str());
    let source = ["url", "id", "data-source-id"]
        .iter()
        .find_map(|key| attrs.get(*key))
        .context("Missing parent data source")?;
    let source = source_id(source)?;
    if !same_id(&source, &source_id(required(config, "data_source_id")?)?) {
        return Err(external(
            "fetch",
            "unexpected_parent_data_source",
            false,
            false,
        ));
    }
    Ok(source)
}

pub struct NotionMcp {
    client: RpcClient,
    thread: String,
    config: Value,
    schema: Option<Value>,
    access: Option<Value>,
    access_loaded: bool,
    markdown_spec: Option<String>,
    loaded_pages: HashSet<String>,
}

impl NotionMcp {
    pub fn new(client: RpcClient, thread: String, config: Value) -> Self {
        Self {
            client,
            thread,
            config,
            schema: None,
            access: None,
            access_loaded: false,
            markdown_spec: None,
            loaded_pages: HashSet::new(),
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(!self.thread.is_empty(), "Notion context thread is required");
        required(&self.config, "mcp_server")?;
        required(&self.config, "executor")?;
        source_id(required(&self.config, "data_source_id")?)?;
        let tools = self.config["tools"]
            .as_object()
            .context("Explicit tool mapping is required")?;
        required(&self.config["tools"], "fetch")?;
        for name in tools.values() {
            ensure!(
                name.is_null() || name.as_str().is_some_and(|name| !name.is_empty()),
                "Configured tool names must be nonempty strings"
            );
        }
        Ok(())
    }

    fn load_access(&mut self) -> Result<()> {
        if !self.access_loaded {
            if self.config["tools"]["access"]
                .as_str()
                .is_some_and(|tool| !tool.is_empty())
            {
                self.access = Some(self.call_raw("access", &json!({}))?);
            }
            self.access_loaded = true;
        }
        Ok(())
    }

    pub fn prepare(&mut self) -> Result<Value> {
        self.validate()?;
        self.load_access()?;
        if let Some(schema) = &self.schema {
            return Ok(schema.clone());
        }
        let source = source_id(required(&self.config, "data_source_id")?)?;
        let schema = self.call_raw("fetch", &json!({"id": format!("collection://{source}")}))?;
        let schema_text = text(&schema, "schema")?;
        let table = regex!(r"<sqlite-table\b[^>]*>([\s\S]*?)</sqlite-table>")
            .captures(schema_text)
            .ok_or_else(|| external("schema", "missing_sqlite_schema", false, false))?;
        let sql = unescape(&table[1]);
        if COLUMNS[1..]
            .iter()
            .any(|column| !sql.contains(&format!("\"{column}\"")))
        {
            return Err(external(
                "schema",
                "task_columns_missing_from_schema",
                false,
                false,
            ));
        }
        self.schema = Some(schema.clone());
        Ok(schema)
    }

    fn check_access(&self, role: &str) -> Result<()> {
        let Some(access) = &self.access else {
            return Ok(());
        };
        let entries = access.get("tools").unwrap_or(access);
        let name = self.config["tools"][role].as_str().unwrap_or("");
        let entry = if let Some(entries) = entries.as_array() {
            entries.iter().find(|entry| entry["name"] == name)
        } else {
            entries.get(name)
        };
        let Some(entry) = entry.filter(|entry| entry.is_object()) else {
            return Ok(());
        };
        if (!entry["status"].is_null()
            && !matches!(
                entry["status"].as_str(),
                Some("available" | "enabled" | "allowed")
            ))
            || entry["available"] == false
        {
            return Err(external(role, "tool_access_denied", false, false));
        }
        if truthy(&entry["restricted_parameters"]) {
            return Err(external(role, "restricted_tool_parameters", false, false));
        }
        Ok(())
    }

    fn call_raw(&mut self, role: &str, args: &Value) -> Result<Value> {
        self.validate()?;
        ensure!(args.is_object(), "Notion tool arguments must be an object");
        let tool = self.config["tools"][role]
            .as_str()
            .filter(|tool| !tool.is_empty())
            .ok_or_else(|| external(role, "tool_not_configured", false, false))?
            .to_owned();
        self.check_access(role)?;
        let write = !matches!(role, "fetch" | "schema" | "query" | "access");
        let result = self.client.request(
            "mcpServer/tool/call",
            &json!({
                "threadId": self.thread, "server": self.config["mcp_server"],
                "tool": tool, "arguments": args
            }),
        );
        let result = result.map_err(|error| tool_request_error(&error, role, write))?;
        let config = (role == "fetch").then_some(&self.config);
        unwrap(result, role, config).map_err(|error| {
            let detail = error.downcast_ref::<ExternalError>();
            external(
                role,
                detail.map_or("invalid_response", |error| error.kind.as_str()),
                write,
                detail.is_some_and(|error| error.reconnectable),
            )
        })
    }

    fn metadata(&mut self, pid: &str, columns: &[&str]) -> Result<Value> {
        let identifier = metadata_id(pid)?;
        self.prepare()?;
        let source = source_id(required(&self.config, "data_source_id")?)?;
        let response = self.call_raw("query", &metadata_query(&identifier, &source, columns))?;
        let rows = rows(response, &source, columns)?;
        if rows.len() != 1
            || metadata_id(
                rows[0]["url"]
                    .as_str()
                    .context("Metadata row requires a URL")?,
            )? != identifier
        {
            return Err(external("query", "task_not_in_tracker", false, false));
        }
        Ok(json!({"id": identifier, "source_id": source, "properties": rows[0]}))
    }

    pub fn get_task(&mut self, pid: &str) -> Result<Value> {
        let identifier = page_argument(pid)?;
        self.prepare()?;
        let raw = self.call_raw("fetch", &json!({"id": identifier}))?;
        let notion_text = text(&raw, "fetch")?;
        let parts = page_parts(notion_text, "fetch")?;
        let source = task_source(&parts, &self.config)?;
        if let Some(actual) = raw["id"].as_str()
            && !same_id(&page_argument(actual)?, &identifier)
        {
            return Err(external("fetch", "unexpected_page_id", false, false));
        }
        let mut result = json!({
            "id": identifier, "source_id": source, "properties": parts.properties,
            "content": parts.content, "notion_text": notion_text, "raw": raw
        });
        if known_template_omissions(&result["raw"], &self.config) {
            result["content_limitations"] = json!({
                "reason": "Known template blocks are preserved as placeholders; their subtrees were not read.",
                "unknown_block_ids": result["raw"]["unknown_block_ids"],
                "unknown_block_count": result["raw"]["unknown_block_count"],
                "truncated": result["raw"]["truncated"]
            });
        }
        self.loaded_pages.insert(identifier);
        Ok(result)
    }

    fn ensure_spec(&mut self) -> Result<()> {
        if self.markdown_spec.is_none() {
            let spec = self.call_raw("fetch", &json!({"id": MARKDOWN_SPEC}))?;
            let spec = text(&spec, "markdown_spec")?;
            if spec.trim().is_empty() {
                return Err(external(
                    "markdown_spec",
                    "empty_markdown_spec",
                    false,
                    false,
                ));
            }
            self.markdown_spec = Some(spec.to_owned());
        }
        Ok(())
    }

    pub fn append_report(&mut self, pid: &str, content: &str) -> Result<Value> {
        ensure!(!content.trim().is_empty(), "Report content cannot be empty");
        let identifier = page_argument(pid)?;
        self.prepare()?;
        if !self.loaded_pages.contains(&identifier) {
            self.get_task(&identifier)?;
        }
        self.ensure_spec()?;
        let result = self.call_raw(
            "update",
            &json!({
                "page_id": identifier, "command": "insert_content",
                "position": {"type": "end"}, "content": content
            }),
        );
        self.loaded_pages.remove(&identifier);
        result
    }

    pub fn create_page(&mut self, properties: &Value, content: Option<&str>) -> Result<Value> {
        flat_properties(properties)?;
        self.prepare()?;
        let mut page = json!({"properties": properties});
        if let Some(content) = content {
            self.ensure_spec()?;
            page["content"] = json!(content);
        }
        let source = source_id(required(&self.config, "data_source_id")?)?;
        self.call_raw(
            "create",
            &json!({
                "parent": {"data_source_id": source}, "allow_async": false, "pages": [page]
            }),
        )
    }
}

fn flat_properties(properties: &Value) -> Result<()> {
    let object = properties
        .as_object()
        .context("Flat Notion properties must be an object")?;
    for (name, value) in object {
        ensure!(
            !name.is_empty()
                && (value.is_null()
                    || value.is_string()
                    || value.is_number()
                    || value
                        .as_array()
                        .is_some_and(|values| values.iter().all(Value::is_string))),
            "Notion tool properties require strings, numbers, string lists or null"
        );
    }
    Ok(())
}

impl Notion for NotionMcp {
    fn query_tasks(&mut self) -> Result<Vec<Value>> {
        self.prepare()?;
        let source = source_id(required(&self.config, "data_source_id")?)?;
        let executor = required(&self.config, "executor")?.to_owned();
        let query = format!(
            "SELECT url, \"任务名称\", \"状态\", \"执行人 / 代理\", \"AI 会话 / 线程\", \"模型\" \
             FROM \"collection://{source}\" WHERE \"执行人 / 代理\" = ? AND url > ? \
             AND \"状态\" IN (?, ?, ?) ORDER BY url LIMIT 50"
        );
        let mut after = String::new();
        let mut result = Vec::new();
        for _ in 0..MAX_QUERY_PAGES {
            let response = self.call_raw(
                "query",
                &json!({"data": {
                    "mode": "sql", "data_source_urls": [format!("collection://{source}")],
                    "query": query,
                    "params": [executor, after, "待办事项", "进行中", "请求续试"]
                }}),
            )?;
            let batch = rows(response, &source, &COLUMNS)?;
            for row in &batch {
                let url = row["url"].as_str().context("Query row requires a URL")?;
                if url <= after.as_str() {
                    return Err(external("query", "pagination_not_advancing", false, false));
                }
                after = url.to_owned();
            }
            let finished = batch.len() < PAGE_SIZE;
            result.extend(batch);
            if finished {
                return Ok(result);
            }
        }
        Err(external("query", "query_page_limit", false, false))
    }

    fn get_task_metadata(&mut self, pid: &str) -> Result<Value> {
        self.metadata(pid, &COLUMNS)
    }

    fn get_feedback_metadata(&mut self, pid: &str) -> Result<Value> {
        self.metadata(pid, &[COLUMNS.as_slice(), &["调度诊断"]].concat())
    }

    fn update_properties(&mut self, pid: &str, properties: &Value) -> Result<Value> {
        let identifier = page_argument(pid)?;
        flat_properties(properties)?;
        self.prepare()?;
        let result = self.call_raw(
            "update",
            &json!({
                "page_id": identifier, "command": "update_properties", "properties": properties
            }),
        );
        self.loaded_pages.remove(&identifier);
        result
    }

    fn fetch(&mut self, pid: &str) -> Result<Value> {
        self.call("fetch", &json!({"id": pid}))
    }

    fn call(&mut self, role: &str, args: &Value) -> Result<Value> {
        self.validate()?;
        if role != "access" {
            self.load_access()?;
        }
        self.call_raw(role, args)
    }
}

pub fn connect_notion(config: &Value) -> Result<NotionMcp> {
    let mut client = RpcClient::from_config(config)?;
    let cwd = config["worker_cwd"]
        .as_str()
        .or_else(|| config["workspace"].as_str())
        .context("Notion control context requires workspace or worker_cwd")?;
    ensure!(
        std::path::Path::new(cwd).is_absolute(),
        "Control cwd must be absolute"
    );
    let response = client.request("thread/start", &json!({"ephemeral": true, "cwd": cwd}))?;
    let thread = required(&response["thread"], "id")?.to_owned();
    let notion = NotionMcp::new(client, thread, config.clone());
    notion.validate()?;
    Ok(notion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::test_support::{SOURCE, config, page, row};

    fn adapter(options: Value) -> Result<NotionMcp> {
        let config = config(options);
        Ok(NotionMcp::new(
            RpcClient::from_config(&config)?,
            "control".into(),
            config,
        ))
    }

    #[test]
    fn query_uses_explicit_names_bound_parameters_and_cached_schema_access() -> Result<()> {
        let mut notion = adapter(json!({"query_pages": [{"rows": [row(1)]}, {"rows": []}]}))?;
        assert_eq!(notion.query_tasks()?, vec![row(1)]);
        assert!(notion.query_tasks()?.is_empty());
        let history = notion.client.request("test/history", &json!({}))?;
        let calls: Vec<_> = history
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call["method"] == "mcpServer/tool/call")
            .collect();
        assert_eq!(calls[0]["params"]["tool"], "access-tool");
        assert_eq!(calls[1]["params"]["tool"], "fetch-tool");
        assert_eq!(calls[2]["params"]["tool"], "query-tool");
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["params"]["tool"] == "fetch-tool")
                .count(),
            1
        );
        let data = &calls[2]["params"]["arguments"]["data"];
        assert_eq!(
            data["params"],
            json!(["Fedora Codex", "", "待办事项", "进行中", "请求续试"])
        );
        assert!(data["query"].as_str().unwrap().contains("url > ?"));
        assert!(!data["query"].as_str().unwrap().contains("Fedora Codex"));
        assert_eq!(calls[2]["params"]["server"], "explicit-server");
        Ok(())
    }

    #[test]
    fn keyset_pagination_requires_empty_terminal_page_and_monotonic_urls() -> Result<()> {
        let batch: Vec<_> = (1..=50).map(row).collect();
        let mut notion = adapter(json!({"query_pages": [{"rows": batch}, {"rows": []}]}))?;
        assert_eq!(notion.query_tasks()?.len(), 50);
        let history = notion.client.request("test/history", &json!({}))?;
        let queries: Vec<_> = history
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call["params"]["tool"] == "query-tool")
            .collect();
        assert_eq!(
            queries[1]["params"]["arguments"]["data"]["params"][1],
            row(50)["url"]
        );
        let mut notion = adapter(json!({"query_pages": [{"rows": [row(2), row(1)]}]}))?;
        assert_eq!(
            notion
                .query_tasks()
                .unwrap_err()
                .downcast_ref::<ExternalError>()
                .unwrap()
                .kind,
            "pagination_not_advancing"
        );
        Ok(())
    }

    #[test]
    fn supported_nested_envelopes_and_matrix_rows() -> Result<()> {
        let expected = row(1);
        let matrix = json!({
            "columns": COLUMNS.iter().map(|name| json!({"name": name})).collect::<Vec<_>>(),
            "rows": [COLUMNS.iter().map(|name| expected[*name].clone()).collect::<Vec<_>>()]
        });
        for value in [
            json!({"result": {"structuredContent": matrix}}),
            json!({"results": [matrix]}),
            json!({"data": {"results": [expected]}}),
            json!({"content": [
                {"type": "text", "text": "{\"rows\":"},
                {"type": "text", "text": format!("[{expected}]}}")}
            ]}),
        ] {
            let decoded = unwrap(value, "query", None)?;
            assert_eq!(
                rows(decoded, "source-id", &COLUMNS)?,
                vec![expected.clone()]
            );
        }
        Ok(())
    }

    #[test]
    fn metadata_checks_require_all_columns_uuid_and_expected_source() {
        let expected = row(1);
        for value in [
            json!({"rows": [{"url": expected["url"]}]}),
            json!({"rows": [expected], "data_source_ids": ["another"]}),
            json!({"rows": [expected], "has_more": true}),
            json!({"rows": [expected], "columns": ["url", "url"]}),
            json!({"rows": [{"url": "https://evil.example/00000000000000000000000000000001"}]}),
        ] {
            assert!(rows(value, "source-id", &COLUMNS).is_err());
        }
        let mut malformed = expected;
        malformed["模型"] = json!(["model"]);
        assert!(rows(json!({"rows": [malformed]}), "source-id", &COLUMNS).is_err());
    }

    #[test]
    fn metadata_reads_never_fetch_page_bodies_and_include_terminal_tasks() -> Result<()> {
        let mut metadata = row(1);
        metadata["状态"] = json!("已完成待归档");
        metadata["调度诊断"] = json!("仅状态");
        let mut notion = adapter(json!({"query_pages": [{"rows": [metadata]}]}))?;
        let record = notion.get_feedback_metadata("00000000-0000-0000-0000-000000000001")?;
        assert_eq!(record["properties"]["调度诊断"], "仅状态");
        let history = notion.client.request("test/history", &json!({}))?;
        for call in history.as_array().unwrap() {
            if call["params"]["tool"] == "fetch-tool" {
                assert_eq!(call["params"]["arguments"]["id"], "collection://source-id");
            }
            if call["params"]["tool"] == "query-tool" {
                assert!(
                    !call["params"]["arguments"]["data"]["query"]
                        .as_str()
                        .unwrap()
                        .contains("状态\" IN")
                );
                assert_eq!(
                    call["params"]["arguments"]["data"]["params"]
                        .as_array()
                        .unwrap()
                        .len(),
                    8
                );
            }
        }
        Ok(())
    }

    #[test]
    fn duplicate_or_foreign_task_metadata_is_rejected() -> Result<()> {
        for response in [
            json!({"rows": []}),
            json!({"rows": [row(1), row(1)]}),
            json!({"rows": [row(2)]}),
        ] {
            let mut notion = adapter(json!({"query_pages": [response]}))?;
            assert!(
                notion
                    .get_task_metadata("00000000-0000-0000-0000-000000000001")
                    .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn access_denial_and_unknown_parameter_restrictions_stop_before_query() -> Result<()> {
        for entry in [
            json!({"status": "unavailable"}),
            json!({"available": false}),
            json!({"status": "available", "restricted_parameters": ["data.mode"]}),
        ] {
            let mut notion = adapter(json!({"access": {"tools": {"query-tool": entry}}}))?;
            let error = notion.query_tasks().unwrap_err();
            assert!(
                !error
                    .downcast_ref::<ExternalError>()
                    .unwrap()
                    .outcome_unknown
            );
            let history = notion.client.request("test/history", &json!({}))?;
            assert!(
                !history
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|call| call["params"]["tool"] == "query-tool")
            );
        }
        Ok(())
    }

    #[test]
    fn schema_failure_stops_before_query() -> Result<()> {
        let mut notion =
            adapter(json!({"schema": "<sqlite-table>missing columns</sqlite-table>"}))?;
        assert_eq!(
            notion
                .query_tasks()
                .unwrap_err()
                .downcast_ref::<ExternalError>()
                .unwrap()
                .kind,
            "task_columns_missing_from_schema"
        );
        Ok(())
    }

    #[test]
    fn content_parser_preserves_exact_chinese_body_and_ignores_literal_examples() -> Result<()> {
        let body = "\n## 要求\n保留原文。\n```xml\n</page>\n<truncated />\n```\n`<truncated />`\n";
        let raw = page(body);
        let mut notion = adapter(json!({"page": raw}))?;
        let result = notion.get_task(SOURCE)?;
        assert_eq!(result["content"], body);
        assert_eq!(result["properties"]["任务名称"], "测试");
        assert_eq!(result["source_id"], "source-id");
        assert_eq!(result["notion_text"], raw["text"]);
        let mut raw = raw;
        raw["text"] = json!(
            raw["text"]
                .as_str()
                .unwrap()
                .replace("\"测试\"", "\"</properties><content>literal</content>\"")
        );
        let parts = page_parts(raw["text"].as_str().unwrap(), "fetch")?;
        assert_eq!(parts.content, body);
        assert_eq!(
            parts.properties["任务名称"],
            "</properties><content>literal</content>"
        );
        Ok(())
    }

    #[test]
    fn missing_closures_explicit_truncation_and_nested_omissions_fail() -> Result<()> {
        let raw = page("报告");
        for changed in [
            raw["text"].as_str().unwrap().replace("</page>", ""),
            raw["text"]
                .as_str()
                .unwrap()
                .replace("<content>", "<content truncated='true'>"),
            raw["text"]
                .as_str()
                .unwrap()
                .replace("</content>", "<truncated />\n</content>"),
        ] {
            assert!(page_parts(&changed, "fetch").is_err());
        }
        for value in [
            json!({"result": {"structuredContent": {"rows": []}}, "truncated": true}),
            json!({"metadata": {"unknown_block_count": 1}}),
            json!({"result": {"isError": true}}),
            json!({"content": [{"type": "image", "text": "unsupported"}]}),
        ] {
            assert!(unwrap(value, "fetch", None).is_err());
        }
        Ok(())
    }

    #[test]
    fn empty_content_and_parent_schema_after_page_are_supported() -> Result<()> {
        let mut raw = page("");
        raw["text"] = json!(format!(
            "{}\n<parent-data-source-schema><content>outside</content></parent-data-source-schema>",
            raw["text"]
                .as_str()
                .unwrap()
                .replace("<content></content>", "<content />")
        ));
        assert_eq!(
            page_parts(raw["text"].as_str().unwrap(), "fetch")?.content,
            ""
        );
        Ok(())
    }

    #[test]
    fn exact_template_allowance_preserves_limitations_but_never_allows_new_omissions() -> Result<()>
    {
        let block = "00000000-0000-4000-8000-000000000001";
        let body = format!(
            "\n## 支持文件\n<unknown url=\"https://app.notion.com/p/{}#{}\" alt=\"drive\" />\n",
            SOURCE.replace('-', ""),
            block.replace('-', "")
        );
        let mut raw = page(&body);
        raw["truncated"] = json!(true);
        raw["unknown_block_count"] = json!(1);
        raw["unknown_block_ids"] = json!([block]);
        let mut config = config(json!({"page": raw}));
        config["known_template_blocks"] = json!({(SOURCE): {(block): "drive"}});
        let mut notion = NotionMcp::new(
            RpcClient::from_config(&config)?,
            "control".into(),
            config.clone(),
        );
        let result = notion.get_task(SOURCE)?;
        assert_eq!(result["content_limitations"]["unknown_block_count"], 1);
        assert_eq!(result["raw"], raw);
        for (key, changed) in [
            ("metadata", json!({"truncated": true})),
            ("is_truncated", json!(true)),
            ("unknown_block_count", json!(2)),
            ("unknown_block_ids", json!([block, block])),
        ] {
            let mut changed_raw = raw.clone();
            changed_raw[key] = changed;
            assert!(unwrap(changed_raw, "fetch", Some(&config)).is_err());
        }
        Ok(())
    }

    #[test]
    fn new_page_template_profile_checks_sections_scope_hosts_and_counts() -> Result<()> {
        let block = "00000000-0000-4000-8000-000000000001";
        let body = format!(
            "\n## 任务要求\nRead\n## 支持文件\n<unknown url=\"https://app.notion.com/p/{}#{}\" alt=\"drive\" />\n## 轮次记录\n",
            SOURCE.replace('-', ""),
            block.replace('-', "")
        );
        let mut raw = page(&body);
        raw["truncated"] = json!(true);
        raw["unknown_block_count"] = json!(1);
        raw["unknown_block_ids"] = json!([block]);
        let config = json!({"template_block_profile": {
            "data_source_id": "source-id", "executor": "Fedora Codex",
            "required_sections": ["任务要求", "支持文件", "轮次记录"],
            "sections": {"支持文件": {"drive": 1}}
        }});
        assert!(known_template_omissions(&raw, &config));
        for (before, after) in [
            ("app.notion.com", "untrusted.example"),
            ("collection://source-id", "collection://other"),
            ("## 支持文件", "## 未知"),
            ("alt=\"drive\"", "alt=\"button\""),
        ] {
            let mut changed = raw.clone();
            changed["text"] = json!(changed["text"].as_str().unwrap().replace(before, after));
            assert!(unwrap(changed, "fetch", Some(&config)).is_err());
        }
        Ok(())
    }

    #[test]
    fn property_writes_use_flat_values_and_are_attempted_once() -> Result<()> {
        let mut notion = adapter(json!({}))?;
        assert_eq!(
            notion.update_properties(
                SOURCE,
                &json!({"状态": "进行中", "AI 会话 / 线程": "codex://threads/test"})
            )?["success"],
            true
        );
        let history = notion.client.request("test/history", &json!({}))?;
        let write = history
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["params"]["tool"] == "update-tool")
            .unwrap();
        assert_eq!(write["params"]["arguments"]["command"], "update_properties");
        assert_eq!(write["params"]["arguments"]["properties"]["状态"], "进行中");
        assert!(
            notion
                .update_properties(SOURCE, &json!({"状态": {"status": {"name": "进行中"}}}))
                .is_err()
        );
        assert!(
            notion
                .update_properties(SOURCE, &json!({"状态": true}))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn append_reads_spec_and_page_then_invalidates_cached_read() -> Result<()> {
        let mut notion = adapter(json!({"page": page("\n旧历史\n")}))?;
        notion.append_report(SOURCE, "完整报告")?;
        notion.append_report(SOURCE, "下一轮")?;
        let history = notion.client.request("test/history", &json!({}))?;
        let calls = history.as_array().unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["params"]["arguments"]["id"] == SOURCE)
                .count(),
            2
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["params"]["arguments"]["id"] == MARKDOWN_SPEC)
                .count(),
            1
        );
        for call in calls
            .iter()
            .filter(|call| call["params"]["tool"] == "update-tool")
        {
            assert_eq!(
                call["params"]["arguments"]["position"],
                json!({"type": "end"})
            );
            assert_eq!(call["params"]["arguments"]["command"], "insert_content");
            assert!(call["params"]["arguments"].get("new_str").is_none());
        }
        Ok(())
    }

    #[test]
    fn uncertain_write_is_never_retried_and_cannot_be_classified_as_reconnectable() -> Result<()> {
        let mut notion = adapter(json!({"disconnect_role": "update"}))?;
        let error = notion
            .update_properties(SOURCE, &json!({"状态": "进行中"}))
            .unwrap_err();
        let error = error.downcast_ref::<ExternalError>().unwrap();
        assert!(error.outcome_unknown);
        assert!(!error.reconnectable);
        Ok(())
    }

    #[test]
    fn only_recognized_mcp_read_failures_are_reconnectable_without_leaking_message() -> Result<()> {
        for (payload, expected) in [
            (
                json!({"code": -32603, "message": "Transport send error: HTTP request failed: private-secret"}),
                true,
            ),
            (
                json!({"code": -32603, "message": "MCP startup failed: timed out awaiting tools/list private-secret"}),
                true,
            ),
            (
                json!({"code": -32603, "message": "Transport HTTP request failed: 401 Unauthorized private-secret"}),
                false,
            ),
            (
                json!({"code": -32603, "message": "unknown internal private-secret"}),
                false,
            ),
        ] {
            let mut notion = adapter(json!({"rpc_error": payload}))?;
            let error = notion.query_tasks().unwrap_err();
            assert_eq!(
                error.downcast_ref::<ExternalError>().unwrap().reconnectable,
                expected
            );
            assert!(!format!("{error:#}").contains("private-secret"));
        }
        Ok(())
    }

    #[test]
    fn content_creation_uses_data_source_parent_without_async_or_rest_objects() -> Result<()> {
        let mut notion = adapter(json!({}))?;
        notion.create_page(&json!({"任务名称": "新任务"}), Some("内容"))?;
        let history = notion.client.request("test/history", &json!({}))?;
        let call = history
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["params"]["tool"] == "create-tool")
            .unwrap();
        assert_eq!(
            call["params"]["arguments"]["parent"],
            json!({"data_source_id": "source-id"})
        );
        assert_eq!(call["params"]["arguments"]["allow_async"], false);
        assert_eq!(
            call["params"]["arguments"]["pages"][0]["properties"]["任务名称"],
            "新任务"
        );
        Ok(())
    }

    #[test]
    fn control_context_is_ephemeral_and_does_not_start_an_agent_turn() -> Result<()> {
        let settings = config(json!({}));
        let mut notion = connect_notion(&settings)?;
        let history = notion.client.request("test/history", &json!({}))?;
        let created = history
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["method"] == "thread/start")
            .unwrap();
        assert_eq!(created["params"], json!({"ephemeral": true, "cwd": "/tmp"}));
        assert!(
            !history
                .as_array()
                .unwrap()
                .iter()
                .any(|call| call["method"] == "turn/start")
        );
        Ok(())
    }
}
