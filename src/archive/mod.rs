mod executor;
pub mod knowledge;
mod notifications;
mod writer;

#[cfg(test)]
mod tests;

pub use executor::{Executor, context_for_task, publish_notifications, run};
pub use knowledge::{
    MAX_SUMMARY_LENGTH, MAX_TOPICS, SUMMARY_SECTIONS, SUMMARY_TARGET, clean_topic_body,
    normalize_id, parse_page, project_id, render_context, render_topic, validate_plan,
};
pub use writer::SafeWriter;

use crate::common::digest;
use anyhow::{Context, Result, ensure};
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct ArchiveError {
    pub category: String,
    pub transient: bool,
    message: String,
}

impl ArchiveError {
    pub fn new(message: impl Into<String>, category: &str, transient: bool) -> Self {
        Self {
            message: message.into(),
            category: category.to_string(),
            transient,
        }
    }
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ArchiveError {}

pub(crate) fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

pub(crate) fn epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Missing text field {key}"))
}

pub(crate) fn phase(value: &Value) -> &str {
    value["phase"].as_str().unwrap_or("queued")
}

pub(crate) fn strings(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .context("Expected an array")?
        .iter()
        .map(|item| Ok(item.as_str().context("Expected a string")?.to_string()))
        .collect()
}

pub(crate) fn merge(target: &mut Value, updates: Value) {
    if !target.is_object() {
        *target = serde_json::json!({});
    }
    if let Some(values) = updates.as_object() {
        for (key, value) in values {
            target[key] = value.clone();
        }
    }
}

pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

pub(crate) fn python_json(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            let entries = keys
                .into_iter()
                .map(|key| {
                    Ok(format!(
                        "{}: {}",
                        serde_json::to_string(key)?,
                        python_json(&values[key])?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            format!("{{{}}}", entries.join(", "))
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_json)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        _ => serde_json::to_string(value)?,
    })
}

pub fn value_digest(value: &Value) -> Result<String> {
    if let Some(value) = value.as_str() {
        Ok(digest(value.as_bytes()))
    } else {
        Ok(digest(python_json(value)?.as_bytes()))
    }
}

pub fn page_url(identifier: &str) -> Result<String> {
    Ok(format!(
        "https://app.notion.com/p/{}",
        normalize_id(identifier)?.replace('-', "")
    ))
}

pub fn mention(identifier: &str) -> Result<String> {
    Ok(format!("<mention url=\"{}\" />", page_url(identifier)?))
}

pub fn canonical(input: &str) -> String {
    let mut output = String::new();
    let lines: Vec<_> = input.split_inclusive('\n').collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start_matches([' ', '\t']);
        let marker = trimmed.chars().next().unwrap_or('\0');
        let length = trimmed.chars().take_while(|value| *value == marker).count();
        if matches!(marker, '`' | '~') && length >= 3 {
            let language = trimmed[length..].trim();
            let mut end = index + 1;
            while end < lines.len() {
                let closing = lines[end].trim();
                if closing.chars().all(|value| value == marker) && closing.len() >= length {
                    break;
                }
                end += 1;
            }
            if end < lines.len() {
                let code = lines[index + 1..end].concat();
                let code = code.strip_suffix('\n').unwrap_or(&code);
                let language = if language.is_empty() || language == "txt" {
                    "text"
                } else {
                    language
                };
                let hash = value_digest(&serde_json::json!({"language": language, "code": code}))
                    .unwrap_or_else(|_| digest(code.as_bytes()));
                output.push_str(&format!("FENCED-CONTENT-SHA256:{hash} "));
                index = end + 1;
                continue;
            }
        }
        output.push_str(line);
        index += 1;
    }
    output = Regex::new(r"(?s)<page\b[^>]*>.*?</page>")
        .expect("static regex")
        .replace_all(&output, "")
        .into_owned();
    output = Regex::new(r"<p\s*/>")
        .expect("static regex")
        .replace_all(&output, "")
        .into_owned();
    let visible = knowledge::mask_literals(&output, true);
    let breaks = Regex::new(r"<br\s*/?>").expect("static regex");
    let mut replaced = String::new();
    let mut cursor = 0;
    for found in breaks.find_iter(&visible) {
        replaced.push_str(&output[cursor..found.start()]);
        replaced.push('\n');
        cursor = found.end();
    }
    replaced.push_str(&output[cursor..]);
    output = replaced
        .replace("<b>", "**")
        .replace("</b>", "**")
        .replace("<code>", "`")
        .replace("</code>", "`");
    output = Regex::new(r#"(?s)<mention\s+url="([^"]+)"\s*>.*?</mention>"#)
        .expect("static regex")
        .replace_all(&output, "<mention url=\"$1\" />")
        .into_owned();
    output = Regex::new(r"\[[^\]\n]*\]\((https://(?:app\.)?notion\.(?:com|so)/[^)]+)\)")
        .expect("static regex")
        .replace_all(&output, "<mention url=\"$1\" />")
        .into_owned();
    output = Regex::new(r"\[(https?://[^\]\s]+)\]\((https?://[^)]+)\)")
        .expect("static regex")
        .replace_all(&output, |captures: &regex::Captures<'_>| {
            if captures[1] == captures[2] {
                captures[1].to_string()
            } else {
                captures[0].to_string()
            }
        })
        .into_owned();
    output =
        Regex::new(r"https://(?:www\.)?(?:app\.)?notion\.(?:so|com)/(?:p/)?([0-9a-f-]{32,36})")
            .expect("static regex")
            .replace_all(&output, |captures: &regex::Captures<'_>| {
                page_url(&captures[1]).unwrap_or_else(|_| captures[0].to_string())
            })
            .into_owned();
    output = Regex::new(r"\\([\\`*_{}\[\]()#+\-.!<>:|])")
        .expect("static regex")
        .replace_all(&output, "$1")
        .into_owned();
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn decode_tool(
    mut value: Value,
    mut inspect: impl FnMut(&Value) -> Result<()>,
) -> Result<Value> {
    for _ in 0..16 {
        if let Some(content) = value.as_str() {
            value = match serde_json::from_str(content) {
                Ok(parsed) => parsed,
                Err(error) if content.trim_start().starts_with(['{', '[']) => {
                    return Err(error.into());
                }
                Err(_) => return Ok(serde_json::json!({"text":content})),
            };
            continue;
        }
        if value.is_array() {
            return Ok(value);
        }
        ensure!(value.is_object(), "Invalid Notion response");
        ensure!(
            !truthy(&value["isError"]) && !truthy(&value["error"]),
            "Notion tool rejected operation"
        );
        ensure!(
            value["type"] != "async_task" && value.get("async_task").is_none(),
            "Asynchronous write outcome requires reconciliation"
        );
        inspect(&value)?;
        if let Some(result) = value.get("result").filter(|value| !value.is_null()) {
            value = result.clone();
        } else if let Some(structured) = value
            .get("structuredContent")
            .filter(|value| !value.is_null())
        {
            value = structured.clone();
        } else if value.get("text").is_none() && value["content"].is_array() {
            let parts = value["content"]
                .as_array()
                .context("Invalid tool content")?;
            ensure!(
                !parts.is_empty()
                    && parts
                        .iter()
                        .all(|part| part["type"] == "text" && part["text"].is_string()),
                "Ambiguous Notion response"
            );
            let mut content = String::new();
            for part in parts {
                inspect(part)?;
                content.push_str(text(part, "text")?);
            }
            value = Value::String(content);
        } else {
            return Ok(value);
        }
    }
    anyhow::bail!("Notion response nesting limit")
}

pub(crate) fn unwrap_tool(value: Value) -> Result<Value> {
    decode_tool(value, |_| Ok(()))
}
