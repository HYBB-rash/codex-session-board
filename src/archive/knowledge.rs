use super::{page_url, text, truthy};
use crate::common::canonical_id;
use anyhow::{Context, Result, bail, ensure};
use lazy_regex::regex;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

pub const SUMMARY_SECTIONS: [&str; 5] = [
    "目标与边界",
    "当前有效方案",
    "关键决定及原因",
    "可复用经验与避坑",
    "已知限制与待确认事项",
];
pub const SUMMARY_TARGET: usize = 1500;
pub const MAX_SUMMARY_LENGTH: usize = 2500;
pub const MAX_TOPICS: usize = 12;

pub fn normalize_id(value: &str) -> Result<String> {
    let value = value.trim();
    ensure!(!value.is_empty(), "ID must be a UUID or Notion URL");
    let candidate = if let Some(value) = value.strip_prefix("collection://") {
        value.to_string()
    } else if value.contains("://") {
        notion_url_id(value)?
    } else {
        value.to_string()
    };
    ensure!(
        regex!(r"^(?:[0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})$")
            .is_match(&candidate),
        "ID must contain a complete UUID"
    );
    let identifier = canonical_id(&candidate)?;
    ensure!(
        identifier != "00000000-0000-0000-0000-000000000000"
            && identifier != "ffffffff-ffff-ffff-ffff-ffffffffffff",
        "placeholder UUID is not a source ID"
    );
    Ok(identifier)
}

fn notion_url_id(value: &str) -> Result<String> {
    let parsed = trusted_notion_url(value)?;
    let segment = parsed
        .path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    let matched =
        regex!(r"(?:^|-)([0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})$")
            .captures(segment)
            .context("Notion URL has no page UUID")?;
    Ok(matched[1].to_string())
}

fn trusted_notion_url(value: &str) -> Result<Url> {
    let parsed = Url::parse(value)?;
    let authority = value
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    ensure!(
        parsed.scheme() == "https"
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.port().is_none()
            && [
                "notion.so",
                "www.notion.so",
                "app.notion.com",
                "notion.com",
                "www.notion.com"
            ]
            .iter()
            .any(|host| authority.eq_ignore_ascii_case(host))
            && !value.chars().any(char::is_whitespace)
            && matches!(
                parsed.host_str(),
                Some(
                    "notion.so"
                        | "www.notion.so"
                        | "app.notion.com"
                        | "notion.com"
                        | "www.notion.com"
                )
            ),
        "ID URL must be a trusted Notion HTTPS URL"
    );
    Ok(parsed)
}

fn entity_id(value: &Value) -> Result<String> {
    if let Some(value) = value.as_str() {
        return normalize_id(value);
    }
    let object = value
        .as_object()
        .context("relation item must identify exactly one page")?;
    let ids = ["id", "url"]
        .into_iter()
        .filter_map(|key| object.get(key))
        .map(|item| normalize_id(item.as_str().context("relation identifier must be text")?))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !ids.is_empty() && ids.iter().all(|item| item == &ids[0]),
        "relation item must identify exactly one page"
    );
    Ok(ids[0].clone())
}

pub fn project_id(properties: &Value) -> Result<String> {
    let mut value = properties
        .get("所属项目")
        .context("所属项目 is missing")?
        .clone();
    if let Some(raw) = value.as_str()
        && raw.trim_start().starts_with(['"', '[', '{'])
    {
        value = serde_json::from_str(raw).context("所属项目 contains invalid JSON")?;
    }
    if value.get("relation").is_some() {
        ensure!(
            value.get("type").is_none_or(|value| value == "relation")
                && !truthy(&value["has_more"]),
            "所属项目 relation is incomplete or has the wrong type"
        );
        value = value["relation"].clone();
        ensure!(value.is_array(), "relation must be a list");
    }
    if let Some(items) = value.as_array() {
        ensure!(
            items.len() == 1,
            "所属项目 must contain exactly one project"
        );
        entity_id(&items[0])
    } else {
        entity_id(&value)
    }
}

pub(crate) fn html_unescape(input: &str) -> String {
    regex!(r"&(#x[0-9a-fA-F]+|#X[0-9a-fA-F]+|#[0-9]+|[A-Za-z][A-Za-z0-9]+);")
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let entity = &captures[1];
            let character = if entity.starts_with("#x") || entity.starts_with("#X") {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            } else if let Some(number) = entity.strip_prefix('#') {
                number.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{a0}'),
                    "ndash" => Some('–'),
                    "mdash" => Some('—'),
                    "hellip" => Some('…'),
                    "copy" => Some('©'),
                    "reg" => Some('®'),
                    "trade" => Some('™'),
                    "laquo" => Some('«'),
                    "raquo" => Some('»'),
                    "lsquo" => Some('‘'),
                    "rsquo" => Some('’'),
                    "ldquo" => Some('“'),
                    "rdquo" => Some('”'),
                    _ => None,
                }
            };
            character.map_or_else(
                || captures[0].to_string(),
                |character| character.to_string(),
            )
        })
        .into_owned()
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn attributes(input: &str) -> Result<BTreeMap<String, String>> {
    let pattern = regex!(r#"([\w-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#);
    let mut result = BTreeMap::new();
    for captures in pattern.captures_iter(input) {
        ensure!(
            !result.contains_key(&captures[1]),
            "duplicate markup attribute"
        );
        let value = captures
            .get(2)
            .or_else(|| captures.get(3))
            .context("Invalid attribute")?;
        result.insert(captures[1].to_string(), html_unescape(value.as_str()));
    }
    ensure!(
        pattern
            .replace_all(input, "")
            .trim()
            .trim_matches('/')
            .trim()
            .is_empty(),
        "malformed markup attributes"
    );
    Ok(result)
}

fn mask(output: &mut String, start: usize, end: usize) {
    let replacement: String = output[start..end]
        .bytes()
        .map(|byte| match byte {
            b'\n' => '\n',
            b'\r' => '\r',
            _ => ' ',
        })
        .collect();
    output.replace_range(start..end, &replacement);
}

pub(crate) fn mask_literals(input: &str, html_code: bool) -> String {
    let mut output = input.to_string();
    mask_fences(input, &mut output);
    let visible = output.clone();
    if html_code {
        for matched in regex!(r"(?s)<code\b[^>]*>.*?</code>").find_iter(&visible) {
            mask(&mut output, matched.start(), matched.end());
        }
    }
    mask_inline_code(&mut output);
    output
}

fn mask_fences(input: &str, output: &mut String) {
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let marker = trimmed.as_bytes().first().copied().unwrap_or(0);
        let length = trimmed.bytes().take_while(|value| *value == marker).count();
        let is_fence = matches!(marker, b'`' | b'~') && length >= 3;
        if let Some((character, opening_length)) = fence {
            mask(output, offset, offset + line.len());
            if is_fence
                && marker == character
                && length >= opening_length
                && trimmed[length..].trim().is_empty()
            {
                fence = None;
            }
        } else if is_fence && (marker != b'`' || !trimmed[length..].contains('`')) {
            fence = Some((marker, length));
            mask(output, offset, offset + line.len());
        }
        offset += line.len();
    }
}

fn mask_inline_code(output: &mut String) {
    let mut cursor = 0;
    while cursor < output.len() {
        if output.as_bytes()[cursor] != b'`' {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < output.len() && output.as_bytes()[cursor] == b'`' {
            cursor += 1;
        }
        let length = cursor - start;
        let mut end = cursor;
        while end < output.len() && output.as_bytes()[end] != b'\n' {
            if output.as_bytes()[end] == b'`' {
                let closing = end;
                while end < output.len() && output.as_bytes()[end] == b'`' {
                    end += 1;
                }
                if end - closing == length {
                    mask(output, start, end);
                    cursor = end;
                    break;
                }
            } else {
                end += 1;
            }
        }
    }
}

fn metadata(value: &Value, explained: bool, depth: usize) -> Result<()> {
    ensure!(depth <= 16, "fetch metadata nesting limit");
    ensure!(
        !truthy(&value["isError"]) && !truthy(&value["error"]),
        "fetch returned an error"
    );
    for key in ["truncated", "is_truncated", "has_more"] {
        let flag = &value[key];
        let absent = flag.is_null() || flag == false || flag == 0 || flag == "false";
        ensure!(
            absent || key == "truncated" && flag == true && explained,
            "unexplained fetch truncation"
        );
    }
    ensure!(
        value["next_cursor"].is_null() || value["next_cursor"] == "",
        "fetch has unread pages"
    );
    ensure!(
        !truthy(&value["truncation"]),
        "fetch reports a separate truncation reason"
    );
    if !explained {
        ensure!(
            !truthy(&value["unknown_block_ids"])
                && (value["unknown_block_count"].is_null()
                    || value["unknown_block_count"] == 0
                    || value["unknown_block_count"] == "0"),
            "unexplained unknown blocks"
        );
    }
    for key in ["metadata", "truncation", "pagination", "_meta"] {
        if value[key].is_object() {
            metadata(&value[key], false, depth + 1)?;
        }
    }
    Ok(())
}

fn fetch_object(raw: &Value) -> Result<Value> {
    let value = super::decode_tool(raw.clone(), |value| {
        if value["type"] == "text" || !value["text"].is_string() {
            metadata(value, false, 0)
        } else {
            Ok(())
        }
    })?;
    ensure!(value.is_object(), "fetch envelope must contain an object");
    ensure!(value["text"].is_string(), "fetch has no page text");
    Ok(value)
}

fn truncated(input: &str) -> bool {
    regex!(r"(?im)<(?:truncated|omitted|truncation)\b|[\[【（(][^\n\]】）)]*(?:truncated|content omitted|已截断|内容截断|内容已省略)[^\n\]】）)]*[\]】）)]|^\s*(?:\.\.\.\s*)?(?:output|response|content|text) truncated\b|^\s*(?:正文|内容|输出)(?:已被|已)?截断[。.!！]?\s*$")
        .is_match(input)
}

fn headings(input: &str) -> Vec<String> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    for captures in regex!(r"(?m)^[ \t]*(#{1,6})[ \t]+([^\n]+)").captures_iter(input) {
        let depth = captures[1].len();
        let title = captures[2].trim().trim_matches('#').trim();
        let title = title
            .replace("<b>", "")
            .replace("</b>", "")
            .trim_matches(['*', ' '])
            .to_string();
        while stack.last().is_some_and(|item| item.0 >= depth) {
            stack.pop();
        }
        stack.push((depth, title));
    }
    stack.into_iter().map(|item| item.1).collect()
}

fn template_section(visible: &str, start: usize, end: usize, kind: &str) -> Result<String> {
    let before = &visible[..start];
    let headings = headings(before);
    let protected =
        regex!(r"任务要求|报告|审阅填写|审阅意见|审阅结论|我的决定|下一轮要求|完成标准|执行指令");
    ensure!(
        !headings.iter().any(|heading| protected.is_match(heading)),
        "unknown block appears in requirements, report or review input"
    );
    ensure!(
        before.rsplit('\n').next().unwrap_or("").trim().is_empty()
            && visible[end..]
                .split('\n')
                .next()
                .unwrap_or("")
                .trim()
                .is_empty(),
        "unknown block appears inside text"
    );
    let containers = template_containers(before)?;
    ensure!(
        !containers.iter().any(|item| item == "callout")
            || !headings.iter().any(|item| item.contains("审阅")),
        "unknown block appears in review input"
    );
    let last = headings.last().map(String::as_str).unwrap_or("");
    if matches!(last, "支持文件" | "文档") {
        return Ok(last.to_string());
    }
    if kind == "button"
        && (last == "审阅按钮"
            || matches!(last, "审阅决定 / 下一轮指令" | "审阅决定/下一轮指令")
                && containers.len() >= 2
                && containers[containers.len() - 2..] == ["columns", "column"])
    {
        return Ok("审阅按钮".to_string());
    }
    bail!("unknown block is outside a recognized template section")
}

fn template_containers(before: &str) -> Result<Vec<String>> {
    let mut containers = Vec::new();
    for captures in regex!(r"<(/?)(callout|columns|column)\b[^>]*>").captures_iter(before) {
        if &captures[1] == "/" {
            ensure!(
                containers.pop().as_deref() == Some(&captures[2]),
                "unbalanced template containers"
            );
        } else {
            containers.push(captures[2].to_string());
        }
    }
    Ok(containers)
}

fn immediate_parent(ancestry: &str, tag: &str) -> Result<Option<String>> {
    let pattern = Regex::new(&format!(r"<{tag}\b([^>]*)/?>"))?;
    let matches: Vec<_> = pattern.captures_iter(ancestry).collect();
    ensure!(matches.len() <= 1, "multiple immediate parents");
    if let Some(captures) = matches.first() {
        let attrs = attributes(&captures[1])?;
        let mut object = serde_json::Map::new();
        for key in ["id", "url"] {
            if let Some(value) = attrs.get(key) {
                object.insert(key.to_string(), json!(value));
            }
        }
        Ok(Some(entity_id(&Value::Object(object))?))
    } else {
        Ok(None)
    }
}

struct PageEnvelope<'a> {
    opening: regex::Captures<'a>,
    properties: Value,
    ancestry: &'a str,
    body: &'a str,
}

fn page_envelope(page_text: &str) -> Result<PageEnvelope<'_>> {
    let opening = regex!(r"(?m)^<page\b([^>]*)>")
        .captures(page_text)
        .context("missing page markup")?;
    let opening_span = opening.get(0).context("missing page markup")?;
    let property_opening = regex!(r"<properties\b[^>]*>")
        .find(&page_text[opening_span.end()..])
        .context("missing properties markup")?;
    let property_start = opening_span.end() + property_opening.end();
    let property_source = &page_text[property_start..];
    let mut stream = serde_json::Deserializer::from_str(property_source).into_iter::<Value>();
    let properties = stream.next().context("missing properties")??;
    ensure!(properties.is_object(), "invalid page properties");
    let property_end = property_start + stream.byte_offset();
    let closing = regex!(r"^\s*</properties>")
        .find(&page_text[property_end..])
        .context("invalid properties boundary")?;
    let offset = property_end + closing.end();
    let body = &page_text[offset..];
    let ancestry = &page_text[opening_span.end()..opening_span.end() + property_opening.start()];
    Ok(PageEnvelope {
        opening,
        properties,
        ancestry,
        body,
    })
}

#[derive(Default)]
struct ContentBounds {
    start: Option<usize>,
    end: Option<usize>,
    outer_end: Option<usize>,
    stack: Vec<String>,
}

impl ContentBounds {
    fn observe(&mut self, captures: &regex::Captures<'_>) -> Result<bool> {
        let token = captures.get(0).context("missing token")?;
        let tag = &captures[2];
        let attrs = &captures[3];
        if &captures[1] == "/" {
            ensure!(
                attrs.trim().is_empty() && self.stack.last().map(String::as_str) == Some(tag),
                "unbalanced page/content markup"
            );
            if tag == "content" {
                self.end = Some(token.start());
            } else if self.stack.len() == 1 {
                ensure!(
                    self.end.is_some(),
                    "outer page closed before complete content"
                );
                self.outer_end = Some(token.end());
                return Ok(true);
            }
            self.stack.pop();
        } else {
            self.open(tag, attrs, token.end())?;
        }
        Ok(false)
    }

    fn open(&mut self, tag: &str, attrs: &str, end: usize) -> Result<()> {
        if tag == "content" {
            ensure!(
                self.stack == ["page"] && self.start.is_none(),
                "unexpected content boundary"
            );
            self.start = Some(end);
            if attrs.trim_end().ends_with('/') {
                self.end = Some(end);
            } else {
                self.stack.push(tag.to_string());
            }
        } else {
            ensure!(
                self.stack == ["page", "content"] && !attrs.trim_end().ends_with('/'),
                "child page must be a complete page inside content"
            );
            self.stack.push(tag.to_string());
        }
        Ok(())
    }
}

fn content_bounds(body: &str, visible: &str) -> Result<(usize, usize, usize)> {
    if let Some(blank) = regex!(r"(?s)^\s*(?:<iconMetadata>.*?</iconMetadata>\s*)?<blank-page>This page is blank and has no content\.</blank-page>\s*</page>").find(body) {
        return Ok((0, 0, blank.end()));
    }
    let mut bounds = ContentBounds {
        stack: vec!["page".to_string()],
        ..ContentBounds::default()
    };
    for captures in
        regex!(r#"<(/?)(page|content)\b((?:[^>"']|"[^"]*"|'[^']*')*)>"#).captures_iter(visible)
    {
        let token = captures.get(0).context("missing token")?;
        if token.start() > 0 && visible.as_bytes()[token.start() - 1] == b'\\' {
            continue;
        }
        if bounds.observe(&captures)? {
            break;
        }
    }
    Ok((
        bounds.start.context("incomplete page/content markup")?,
        bounds.end.context("incomplete page/content markup")?,
        bounds.outer_end.context("incomplete page/content markup")?,
    ))
}

fn page_identifier(value: &Value, properties: &Value, opening: &str) -> Result<String> {
    let opening_attrs = regex!(r"(?:^|\s)deleted(?:\s|$)").replace_all(opening, " ");
    let attrs = attributes(&opening_attrs)?;
    let mut identifiers = Vec::new();
    for candidate in [value.get("id"), value.get("url"), properties.get("url")]
        .into_iter()
        .flatten()
    {
        ensure!(
            candidate.is_null() || candidate.is_string(),
            "page identifier must be text"
        );
    }
    for identifier in [
        value.get("id").and_then(Value::as_str),
        value.get("url").and_then(Value::as_str),
        attrs.get("url").map(String::as_str),
        properties.get("url").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    {
        identifiers.push(normalize_id(identifier)?);
    }
    ensure!(
        !identifiers.is_empty() && identifiers.iter().all(|item| item == &identifiers[0]),
        "page ID is missing or inconsistent"
    );
    Ok(identifiers[0].clone())
}

fn validate_truncation(
    envelope: &PageEnvelope<'_>,
    prefix: &str,
    outer_end: usize,
    visible_content: &str,
) -> Result<()> {
    ensure!(
        !truncated(&format!(
            "{visible_content}\n{}\n{}",
            envelope.ancestry,
            mask_literals(&envelope.body[outer_end..], true)
        )) && !regex!(r#"(?i)<(?:page|content)\b[^>]*\btruncated\s*=\s*["']true["']"#)
            .is_match(&format!("{}\n{prefix}", &envelope.opening[0])),
        "actual text truncation marker"
    );
    Ok(())
}

fn unknown_blocks(
    visible_content: &str,
    identifier: &str,
) -> Result<(BTreeSet<String>, Vec<String>)> {
    let unknown_pattern = regex!(r"<unknown\b([^>]*)/>");
    let unknowns: Vec<_> = unknown_pattern
        .captures_iter(visible_content)
        .filter(|capture| {
            capture.get(0).is_some_and(|matched| {
                matched.start() == 0 || visible_content.as_bytes()[matched.start() - 1] != b'\\'
            })
        })
        .collect();
    let unknown_count = regex!(r"</?unknown\b")
        .find_iter(visible_content)
        .filter(|matched| {
            matched.start() == 0 || visible_content.as_bytes()[matched.start() - 1] != b'\\'
        })
        .count();
    ensure!(unknowns.len() == unknown_count, "malformed unknown block");
    let mut observed = BTreeSet::new();
    let mut limitations = Vec::new();
    for captures in unknowns {
        let (block, limitation) = unknown_block(visible_content, identifier, &captures)?;
        ensure!(observed.insert(block), "duplicate unknown block");
        limitations.push(limitation);
    }
    Ok((observed, limitations))
}

fn unknown_block(
    visible_content: &str,
    identifier: &str,
    captures: &regex::Captures<'_>,
) -> Result<(String, String)> {
    let attrs = attributes(&captures[1])?;
    let kind = attrs
        .get("alt")
        .map(String::as_str)
        .context("unsupported unknown block type")?;
    ensure!(
        [
            "unknown",
            "drive",
            "figma",
            "button",
            "external_object_instance"
        ]
        .contains(&kind),
        "unsupported unknown block type"
    );
    let source_url = attrs.get("url").context("unknown block has no URL")?;
    ensure!(
        normalize_id(source_url)? == identifier,
        "unknown block belongs to another page"
    );
    let parsed_url = Url::parse(source_url)?;
    let block = normalize_id(
        parsed_url
            .fragment()
            .context("unknown block has no block ID")?,
    )?;
    let matched = captures.get(0).context("missing unknown block")?;
    let section = template_section(visible_content, matched.start(), matched.end(), kind)?;
    let limitation =
        format!("已忽略「{section}」模板块 {kind}（{block}）；其子内容未读取，不作为知识来源。");
    Ok((block, limitation))
}

fn validate_unknown_metadata(value: &Value, observed: &BTreeSet<String>) -> Result<()> {
    let ids = value
        .get("unknown_block_ids")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let ids = ids.as_array().context("unknown_block_ids must be a list")?;
    let normalized = ids
        .iter()
        .map(|item| normalize_id(item.as_str().context("unknown block ID must be text")?))
        .collect::<Result<Vec<_>>>()?;
    let unique: BTreeSet<_> = normalized.iter().cloned().collect();
    ensure!(
        unique.len() == normalized.len() && unique == *observed,
        "unknown IDs do not exactly explain omissions"
    );
    let count = value
        .get("unknown_block_count")
        .cloned()
        .unwrap_or_else(|| json!(0));
    ensure!(
        count.as_u64() == Some(observed.len() as u64),
        "unknown block count does not explain omissions"
    );
    metadata(value, !observed.is_empty(), 0)?;
    Ok(())
}

fn child_pages(content: &str, visible_content: &str, identifier: &str) -> Result<Vec<Value>> {
    let mut children = Vec::new();
    let mut child_ids = BTreeSet::new();
    let child_pattern = regex!(r"(?s)<page\b([^>]*)>(.*?)</page>");
    for captures in child_pattern.captures_iter(visible_content) {
        let matched = captures.get(0).context("missing child page")?;
        if matched.start() > 0 && visible_content.as_bytes()[matched.start() - 1] == b'\\' {
            continue;
        }
        let attrs = attributes(&captures[1])?;
        let child_url = attrs.get("url").context("child page has no URL")?;
        let child = normalize_id(child_url)?;
        ensure!(
            child != identifier && child_ids.insert(child.clone()),
            "duplicate or self-referencing child page"
        );
        let title = captures.get(2).context("child page has no title")?;
        children.push(json!({"id": child, "title": html_unescape(content[title.start()..title.end()].trim()), "url": child_url}));
    }
    let count = regex!(r"<page\b")
        .find_iter(visible_content)
        .filter(|matched| {
            matched.start() == 0 || visible_content.as_bytes()[matched.start() - 1] != b'\\'
        })
        .count();
    ensure!(count == children.len(), "incomplete child page markup");
    Ok(children)
}

fn page_edited(value: &Value, properties: &Value) -> Result<Value> {
    let edited = ["page_last_edited_at", "last_edited_time", "edited_at"]
        .into_iter()
        .find_map(|key| value.get(key).filter(|value| !value.is_null()))
        .or_else(|| {
            properties
                .get("更新时间：")
                .filter(|value| !value.is_null())
        })
        .cloned()
        .unwrap_or(Value::Null);
    ensure!(
        edited.is_null() || edited.is_string(),
        "edited timestamp must be a string"
    );
    Ok(edited)
}

pub fn parse_page(raw: &Value) -> Result<Value> {
    let value = fetch_object(raw)?;
    let envelope = page_envelope(text(&value, "text")?)?;
    let visible = mask_literals(envelope.body, true);
    let (start, end, outer_end) = content_bounds(envelope.body, &visible)?;
    let content = &envelope.body[start..end];
    let identifier = page_identifier(&value, &envelope.properties, &envelope.opening[1])?;
    let visible_content = mask_literals(content, true);
    validate_truncation(&envelope, &visible[..start], outer_end, &visible_content)?;
    let (observed, limitations) = unknown_blocks(&visible_content, &identifier)?;
    validate_unknown_metadata(&value, &observed)?;
    let parent_page = immediate_parent(envelope.ancestry, "parent-page")?;
    let parent_source = immediate_parent(envelope.ancestry, "parent-data-source")?;
    ensure!(
        parent_page.is_none() || parent_source.is_none(),
        "page has two immediate parent types"
    );
    let children = child_pages(content, &visible_content, &identifier)?;
    let edited = page_edited(&value, &envelope.properties)?;
    let deleted = value["is_archived"] == true
        || value["in_trash"] == true
        || regex!(r"\bdeleted(?:\s|/|$)").is_match(&envelope.opening[1]);
    Ok(
        json!({"id": identifier, "properties": envelope.properties, "content": content, "deleted": deleted,
        "parent_page_id": parent_page, "parent_data_source_id": parent_source,
        "edited_at": edited, "limitations": limitations, "children": children}),
    )
}

fn string(value: &Value, field: &str, empty: bool) -> Result<String> {
    let value = value
        .as_str()
        .with_context(|| format!("{field} must be a string"))?;
    ensure!(
        empty || !value.trim().is_empty(),
        "{field} must be a nonempty string"
    );
    ensure!(
        !value
            .chars()
            .any(|character| character < '\u{20}' && !matches!(character, '\t' | '\n' | '\r')),
        "{field} contains control characters"
    );
    Ok(value.trim().to_string())
}

fn key(value: &Value) -> Result<String> {
    let value = value
        .as_str()
        .context("topic key must be a lowercase ASCII slug")?;
    ensure!(
        regex!(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").is_match(value),
        "topic key must be a lowercase ASCII slug"
    );
    Ok(value.to_string())
}

fn source_url(input: &str, available: Option<&BTreeSet<String>>) -> Result<String> {
    let input = html_unescape(input);
    ensure!(
        input.starts_with("https://"),
        "links must identify read Notion pages"
    );
    let identifier = normalize_id(&input)?;
    ensure!(
        available.is_none_or(|available| available.contains(&identifier)),
        "link references a source that was not read"
    );
    Ok(identifier)
}

fn safe_text(input: &str, available: Option<&BTreeSet<String>>) -> Result<()> {
    let visible = mask_literals(input, false);
    safe_markup(&visible, available)?;
    safe_links(&visible, available)?;
    ensure!(
        !truncated(&visible),
        "knowledge must not contain truncation placeholders"
    );
    Ok(())
}

fn safe_markup(visible: &str, available: Option<&BTreeSet<String>>) -> Result<()> {
    for captures in regex!(r"<(/?)([A-Za-z][\w-]*)\b([^>]*)>").captures_iter(visible) {
        let closing = &captures[1] == "/";
        match &captures[2] {
            "b" | "strong" | "i" | "em" | "s" | "del" | "u" | "code" | "br" | "p" => {
                ensure!(
                    captures[3].trim().trim_matches('/').trim().is_empty(),
                    "HTML attributes are not allowed"
                );
            }
            "mention" => {
                if closing {
                    ensure!(captures[3].trim().is_empty(), "invalid mention closing tag");
                } else {
                    let attrs = attributes(&captures[3])?;
                    ensure!(
                        attrs.len() == 1 && attrs.contains_key("url"),
                        "mention must contain only a source URL"
                    );
                    source_url(&attrs["url"], available)?;
                }
            }
            _ => bail!("unsupported or unsafe HTML"),
        }
    }
    ensure!(
        !regex!(r"(?i)<!--|<!DOCTYPE|<\?|<\s*/?\s*(?:script|iframe|img)\b").is_match(visible),
        "unsafe HTML"
    );
    Ok(())
}

fn safe_links(visible: &str, available: Option<&BTreeSet<String>>) -> Result<()> {
    for captures in regex!(r"!?\[[^\]\n]*\]\(([^)\n]*)\)").captures_iter(visible) {
        source_url(
            captures[1]
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>'),
            available,
        )?;
    }
    for captures in regex!(r"(?m)^\s{0,3}\[[^\]\n]+\]:\s*(\S+)").captures_iter(visible) {
        source_url(captures[1].trim_matches(['<', '>']), available)?;
    }
    for matched in
        regex!(r#"(?i)(?:[A-Za-z][A-Za-z0-9+.-]*://|javascript:|data:|vbscript:)[^\s<>()\[\]"']+"#)
            .find_iter(visible)
    {
        source_url(
            matched
                .as_str()
                .trim_end_matches(['.', ',', ';', '，', '。', '；']),
            available,
        )?;
    }
    Ok(())
}

fn summary(value: &Value, empty: bool) -> Result<String> {
    let summary = string(value, "summary", empty)?;
    let visible =
        regex!(r"(?s)<mention\b[^>]*?(?:/>|>.*?</mention>)").replace_all(&summary, "[来源]");
    let visible = regex!(r"\[([^\]\n]*)\]\([^)\n]+\)")
        .replace_all(&visible, "$1")
        .into_owned();
    ensure!(
        visible.chars().count() <= MAX_SUMMARY_LENGTH,
        "summary exceeds 2500 characters; revise it without truncation"
    );
    if summary.is_empty() {
        return Ok(summary);
    }
    let pattern = Regex::new(&format!(
        r"(?m)^[ \t]*(?:#{{1,6}}[ \t]+)?(?:\*\*)?({})(?:\*\*)?[ \t]*[：:]?[ \t]*(?:\n|$)",
        SUMMARY_SECTIONS.join("|")
    ))?;
    let visible = mask_literals(&summary, false);
    let matches: Vec<_> = pattern.captures_iter(&visible).collect();
    ensure!(
        matches
            .iter()
            .map(|capture| &capture[1])
            .collect::<Vec<_>>()
            == SUMMARY_SECTIONS,
        "summary must contain all five named sections in order"
    );
    validate_summary_sections(&summary, &matches)?;
    Ok(summary)
}

fn validate_summary_sections(summary: &str, matches: &[regex::Captures<'_>]) -> Result<()> {
    for (index, capture) in matches.iter().enumerate() {
        let start = capture.get(0).context("missing summary section")?.end();
        let end = matches
            .get(index + 1)
            .and_then(|capture| capture.get(0))
            .map_or(summary.len(), |matched| matched.start());
        ensure!(
            summary[start..end]
                .chars()
                .any(|character| !character.is_whitespace()
                    && !matches!(character, '#' | '*' | '_' | '-')),
            "each summary section must contain meaningful text"
        );
    }
    Ok(())
}

fn exact_fields(value: &Value, names: &[&str], message: &str) -> Result<()> {
    let object = value.as_object().context(message.to_string())?;
    ensure!(
        object.len() == names.len() && names.iter().all(|name| object.contains_key(*name)),
        "{message}"
    );
    Ok(())
}

pub fn validate_plan(
    plan: &Value,
    available_source_ids: &[String],
    existing_topic_keys: &[String],
) -> Result<Value> {
    exact_fields(
        plan,
        &["summary", "topics", "no_update_reason", "conflicts"],
        "plan must have summary, topics, no_update_reason and conflicts",
    )?;
    let available = plan_sources(available_source_ids, existing_topic_keys)?;
    let reason = string(&plan["no_update_reason"], "no_update_reason", true)?;
    let summary = summary(&plan["summary"], !reason.is_empty())?;
    safe_text(&summary, Some(&available))?;
    safe_text(&reason, Some(&available))?;
    let conflicts = plan_conflicts(&plan["conflicts"], &available)?;
    let topics = plan["topics"].as_array().context("topics must be a list")?;
    ensure!(
        topics.len() <= MAX_TOPICS,
        "topics must be a list of at most 12 topics"
    );
    ensure!(
        reason.is_empty() || topics.is_empty(),
        "no-update reason cannot also request topic updates"
    );
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for topic in topics {
        let topic = validate_topic(topic, &available)?;
        ensure!(
            seen.insert(text(&topic, "key")?.to_string()),
            "duplicate topic key"
        );
        normalized.push(topic);
    }
    Ok(
        json!({"summary": summary, "topics": normalized, "no_update_reason": reason, "conflicts": conflicts}),
    )
}

fn plan_sources(source_ids: &[String], topic_keys: &[String]) -> Result<BTreeSet<String>> {
    let sources = source_ids
        .iter()
        .map(|value| normalize_id(value))
        .collect::<Result<_>>()?;
    for existing in topic_keys {
        key(&json!(existing))?;
    }
    Ok(sources)
}

fn plan_conflicts(value: &Value, available: &BTreeSet<String>) -> Result<Vec<String>> {
    value
        .as_array()
        .context("conflicts must be a list")?
        .iter()
        .map(|value| {
            let value = string(value, "conflict", false)?;
            safe_text(&value, Some(available))?;
            Ok(value)
        })
        .collect()
}

fn topic_sources(topic: &Value, available: &BTreeSet<String>) -> Result<Vec<String>> {
    let source_ids = topic["source_ids"]
        .as_array()
        .context("source_ids must be a list")?;
    ensure!(!source_ids.is_empty(), "each topic needs source_ids");
    let mut sources = Vec::new();
    for source in source_ids {
        let source = normalize_id(source.as_str().context("source ID must be text")?)?;
        ensure!(
            available.contains(&source),
            "topic uses a source that was not read completely"
        );
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    Ok(sources)
}

fn validate_topic(topic: &Value, available: &BTreeSet<String>) -> Result<Value> {
    exact_fields(
        topic,
        &["key", "title", "when_to_read", "content", "source_ids"],
        "topic has an invalid JSON shape",
    )?;
    let topic_key = key(&topic["key"])?;
    let mut sources = topic_sources(topic, available)?;
    let title = string(&topic["title"], "title", false)?;
    let condition = string(&topic["when_to_read"], "when_to_read", false)?;
    let content = string(&topic["content"], "content", false)?;
    for value in [&title, &condition, &content] {
        safe_text(value, Some(available))?;
        for matched in regex!(r#"https://[^\s<>()\[\]"']+"#).find_iter(&mask_literals(value, false))
        {
            let cited = normalize_id(
                matched
                    .as_str()
                    .trim_end_matches(['.', ',', ';', '，', '。', '；']),
            )?;
            if !sources.contains(&cited) {
                sources.push(cited);
            }
        }
    }
    Ok(
        json!({"key": topic_key, "title": title, "when_to_read": condition,
            "content": clean_topic_body(&content)?, "source_ids": sources}),
    )
}

pub fn clean_topic_body(content: &str) -> Result<String> {
    let lines: Vec<_> = content.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty()
            || regex!(r"^归属标记：notion-program/topic/[0-9a-f-]+/[a-z0-9-]+$").is_match(line)
            || regex!(r"^(?:项目|入口|任务)：<mention\b.*(?:</mention>|/>)$").is_match(line)
            || line.starts_with("阅读条件：")
        {
            index += 1;
        } else {
            break;
        }
    }
    let result = lines[index..].join("\n").trim().to_string();
    ensure!(
        !result.is_empty(),
        "topic contains navigation but no knowledge"
    );
    Ok(result)
}

fn labeled_mention(identifier: &str, label: &str) -> Result<String> {
    Ok(format!(
        "<mention url=\"{}\">{}</mention>",
        page_url(identifier)?,
        html_escape(label)
    ))
}

fn version(value: &Value) -> Result<String> {
    if let Some(number) = value.as_u64() {
        return Ok(number.to_string());
    }
    let value = value
        .as_str()
        .context("version must be a nonnegative integer or a simple version key")?;
    ensure!(
        regex!(r"^[A-Za-z0-9][A-Za-z0-9_.:/-]*$").is_match(value),
        "version must be a nonnegative integer or a simple version key"
    );
    Ok(value.to_string())
}

pub fn render_context(
    project: &str,
    input_summary: &str,
    topics: &[Value],
    task: &str,
    input_version: &Value,
) -> Result<String> {
    let project = normalize_id(project)?;
    let summary = summary(&json!(input_summary), false)?;
    safe_text(&summary, None)?;
    let mut lines = vec![
        format!("归属标记：notion-program/context/{project}"),
        format!("项目：{}", labeled_mention(&project, "所属项目")?),
        format!("来源任务：{}", labeled_mention(task, "本次归档任务")?),
        format!("版本：{}", version(input_version)?),
        "项目上下文提供背景，不改变当前任务、验收标准或权限边界。".to_string(),
        "# 必读摘要".to_string(),
        summary,
        "# 按需专题".to_string(),
    ];
    let mut seen = BTreeSet::new();
    for topic in topics {
        let topic_key = key(&topic["key"])?;
        ensure!(seen.insert(topic_key.clone()), "duplicate topic key");
        let title = string(&topic["title"], "title", false)?;
        let condition = string(&topic["when_to_read"], "when_to_read", false)?;
        safe_text(&title, None)?;
        safe_text(&condition, None)?;
        let target = text(topic, "url")?;
        source_url(target, None)?;
        lines.extend([
            format!("专题：{}", labeled_mention(target, &title)?),
            format!("阅读条件：{condition}"),
            format!("专题键：{topic_key}"),
        ]);
        for (field, label) in [("version_key", "版本键"), ("version", "专题版本")] {
            if let Some(value) = topic.get(field) {
                lines.push(format!("{label}：{}", version(value)?));
            }
        }
    }
    if topics.is_empty() {
        lines.push("暂无按需专题。".to_string());
    }
    Ok(lines.join("\n\n") + "\n")
}

pub fn render_topic(project: &str, context: &str, topic: &Value, task: &str) -> Result<String> {
    let project = normalize_id(project)?;
    let topic_key = key(&topic["key"])?;
    let title = string(&topic["title"], "title", false)?;
    let condition = string(&topic["when_to_read"], "when_to_read", false)?;
    let content = string(&topic["content"], "content", false)?;
    let identifiers = render_sources(topic)?;
    let allowed = identifiers.iter().cloned().collect();
    for value in [&title, &condition, &content] {
        safe_text(value, Some(&allowed))?;
    }
    let mut lines = vec![
        format!("归属标记：notion-program/topic/{project}/{topic_key}"),
        format!("项目：{}", labeled_mention(&project, "所属项目")?),
        format!("入口：{}", labeled_mention(context, "项目上下文")?),
        format!("任务：{}", labeled_mention(task, "本次归档任务")?),
        format!("阅读条件：{condition}"),
        content,
        "## 原始来源".to_string(),
    ];
    lines.push(
        identifiers
            .iter()
            .enumerate()
            .map(|(index, identifier)| {
                Ok(format!(
                    "- {}",
                    labeled_mention(identifier, &format!("来源 {}", index + 1))?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n"),
    );
    Ok(lines.join("\n\n") + "\n")
}

fn render_sources(topic: &Value) -> Result<Vec<String>> {
    let sources = topic["source_ids"]
        .as_array()
        .context("topic needs source_ids")?;
    ensure!(!sources.is_empty(), "topic needs source_ids");
    let mut identifiers = Vec::new();
    for source in sources {
        let source = normalize_id(source.as_str().context("source ID must be text")?)?;
        if !identifiers.contains(&source) {
            identifiers.push(source);
        }
    }
    Ok(identifiers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "6a2722bc-14f2-4a7e-a67c-948ee26cfd66";
    const CHILD: &str = "4afccc90-9072-437a-8d75-bead4629547c";

    fn summary_text() -> String {
        SUMMARY_SECTIONS
            .iter()
            .map(|section| format!("## {section}\n完整说明。"))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn literal_mask_preserves_unicode_offsets_and_crlf() {
        let input =
            "正文🙂\r\n```text\r\n中文🦀<br>\r\n```\r\n尾部 `中文` <code>🎉\r\n<br></code>\r\n结束";
        let expected = format!(
            "正文🙂\r\n{}\r\n{}\r\n{}\r\n尾部 {} {}\r\n{}\r\n结束",
            " ".repeat(7),
            " ".repeat(14),
            " ".repeat(3),
            " ".repeat(8),
            " ".repeat(10),
            " ".repeat(11),
        );
        let masked = mask_literals(input, true);
        assert_eq!(masked, expected);
        assert_eq!(masked.len(), input.len());
        assert_eq!(
            mask_literals("正文 `中文` <code>保留</code>", false),
            "正文          <code>保留</code>"
        );
    }

    #[test]
    fn strict_identity_and_single_project() -> Result<()> {
        assert_eq!(
            normalize_id(&format!(
                "https://www.notion.so/Title-{}",
                PAGE.replace('-', "")
            ))?,
            PAGE
        );
        assert!(normalize_id(&format!("https://evil.example/{PAGE}")).is_err());
        assert!(normalize_id("00000000000000000000000000000000").is_err());
        assert_eq!(
            project_id(&json!({"所属项目":{"relation":[{"id":PAGE}],"has_more":false}}))?,
            PAGE
        );
        assert!(project_id(&json!({"所属项目":[PAGE, CHILD]})).is_err());
        Ok(())
    }

    #[test]
    fn nested_page_content_and_code_remain_exact() -> Result<()> {
        let body = format!(
            "\n正文。\n<page url=\"{}\">\n子页\n</page>\n````xml\n```\n</content>\n</page>\n````\n末尾。\n",
            page_url(CHILD)?
        );
        let raw = json!({"id":PAGE,"text":format!("<page url=\"{}\">\n<properties>{{\"url\":\"{}\"}}</properties>\n<content>{body}</content>\n</page>\n", page_url(PAGE)?, page_url(PAGE)?)});
        let parsed = parse_page(&raw)?;
        assert_eq!(parsed["content"], body);
        assert_eq!(parsed["children"][0]["id"], CHILD);
        let mut truncated = raw;
        truncated["truncated"] = json!(true);
        assert!(parse_page(&truncated).is_err());
        Ok(())
    }

    #[test]
    fn plan_provenance_and_unicode_limit() -> Result<()> {
        let plan = json!({"summary":summary_text(),"topics":[{"key":"recovery","title":"恢复","when_to_read":"中断时","content":"核对现有结果。","source_ids":[PAGE]}],"no_update_reason":"","conflicts":[]});
        let available = vec![PAGE.to_string()];
        assert_eq!(
            validate_plan(&plan, &available, &[])?["topics"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let mut invalid = plan;
        invalid["summary"] = json!(summary_text() + &"中".repeat(2501));
        assert!(validate_plan(&invalid, &available, &[]).is_err());
        invalid["summary"] = json!(summary_text());
        invalid["topics"][0]["content"] = json!("[未知](https://example.com/source)");
        assert!(validate_plan(&invalid, &available, &[]).is_err());
        Ok(())
    }

    #[test]
    fn no_update_and_native_rendering() -> Result<()> {
        let plan = json!({"summary":"","topics":[],"no_update_reason":"已检查，无新增知识。","conflicts":[]});
        assert_eq!(validate_plan(&plan, &[PAGE.to_string()], &[])?, plan);
        let rendered = render_context(PAGE, &summary_text(), &[], CHILD, &json!("baseline"))?;
        assert!(rendered.contains(&format!("notion-program/context/{PAGE}")));
        assert!(!rendered.contains("<page "));
        Ok(())
    }
}
