use super::{
    ArchiveError, canonical, merge, normalize_id, parse_page, text, timestamp, unwrap_tool,
};
use crate::clients::Notion;
use crate::common::{digest, read_json, write_json};
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) fn fetch_page<N: Notion + ?Sized>(
    notion: &mut N,
    identifier: &str,
    evidence: Option<&Path>,
) -> Result<Value> {
    let raw = notion.fetch(identifier)?;
    if let Some(evidence) = evidence {
        write_json(
            &evidence.join(format!("{}.json", normalize_id(identifier)?)),
            &raw,
        )?;
    }
    parse_page(&raw)
}

pub struct SafeWriter<'a, N: Notion + ?Sized> {
    pub notion: &'a mut N,
    pub root: PathBuf,
    evidence: Option<PathBuf>,
    spec_loaded: bool,
}

impl<'a, N: Notion + ?Sized> SafeWriter<'a, N> {
    pub fn new(notion: &'a mut N, root: impl Into<PathBuf>) -> Self {
        Self {
            notion,
            root: root.into(),
            evidence: None,
            spec_loaded: false,
        }
    }

    pub fn with_evidence(mut self, evidence: impl Into<PathBuf>) -> Self {
        self.evidence = Some(evidence.into());
        self
    }

    pub(crate) fn page(&mut self, identifier: &str) -> Result<Value> {
        fetch_page(self.notion, identifier, self.evidence.as_deref())
    }

    fn call(&mut self, role: &str, arguments: &Value) -> Result<Value> {
        let raw = self.notion.call(role, arguments)?;
        match unwrap_tool(raw.clone()) {
            Ok(value) => Ok(value),
            Err(error) => {
                if let Some(evidence) = &self.evidence {
                    write_json(
                        &evidence
                            .join("errors")
                            .join(format!("{}.json", Uuid::new_v4().simple())),
                        &json!({"role":role,"at":timestamp(),"response":raw}),
                    )?;
                }
                Err(ArchiveError::new(error.to_string(), "tool_connection", false).into())
            }
        }
    }

    fn spec(&mut self) -> Result<()> {
        if !self.spec_loaded {
            let raw = unwrap_tool(
                self.notion
                    .fetch("notion://docs/notion-flavored-markdown-spec")?,
            )?;
            ensure!(
                raw["text"].as_str().is_some_and(|text| !text.is_empty()),
                "Markdown specification unavailable"
            );
            self.spec_loaded = true;
        }
        Ok(())
    }

    pub fn ensure_child(
        &mut self,
        parent: &str,
        title: &str,
        marker: &str,
        content: &str,
        key: &str,
        mutable: bool,
    ) -> Result<Value> {
        let path = self.root.join(format!("{key}.json"));
        let mut state = read_json(&path, json!({}))?;
        let current = self.page(parent)?;
        let mut candidates = Vec::new();
        for child in current["children"].as_array().context("Missing children")? {
            if state.as_object().is_some_and(|state| state.is_empty())
                || child["title"] == title
                || child["id"] == state["page_id"]
            {
                let candidate = self.page(text(child, "id")?)?;
                ensure!(
                    candidate["parent_page_id"] == parent,
                    "Child page parent mismatch"
                );
                if text(&candidate, "content")?.contains(marker) {
                    candidates.push(candidate);
                } else {
                    ensure!(
                        child["title"] != title,
                        "Existing same-title page has unverified ownership"
                    );
                }
            }
        }
        ensure!(
            candidates.len() <= 1,
            "Multiple child pages have the same ownership marker"
        );
        if let Some(candidate) = candidates.pop() {
            ensure!(
                state["page_id"].is_null() || candidate["id"] == state["page_id"],
                "Bound page routing changed"
            );
            let expected = if state["phase"] == "creating" {
                state["content"].as_str().unwrap_or(content)
            } else {
                content
            };
            if !mutable || state["phase"] == "creating" {
                ensure!(
                    canonical(text(&candidate, "content")?) == canonical(expected),
                    "Recovered creation failed complete content verification"
                );
            }
            merge(
                &mut state,
                json!({"phase":"verified","page_id":candidate["id"],"parent":parent,"marker":marker,"verified_at":timestamp()}),
            );
            write_json(&path, &state)?;
            return Ok(candidate);
        }
        ensure!(
            state.as_object().is_some_and(|state| state.is_empty()),
            "Created/bound page not found; creation held for reconciliation"
        );
        state = json!({"phase":"creating","parent":parent,"marker":marker,"title":title,"content":content,"at":timestamp()});
        write_json(&path, &state)?;
        self.spec()?;
        let response = self.call(
            "create",
            &json!({
                "parent":{"page_id":parent},"allow_async":false,
                "pages":[{"properties":{"title":title},"content":content}]
            }),
        )?;
        state["response"] = response;
        write_json(&path, &state)?;
        let current = self.page(parent)?;
        let mut candidates = Vec::new();
        for child in current["children"].as_array().context("Missing children")? {
            if child["title"] == title {
                let candidate = self.page(text(child, "id")?)?;
                if candidate["parent_page_id"] == parent
                    && text(&candidate, "content")?.contains(marker)
                {
                    candidates.push(candidate);
                }
            }
        }
        ensure!(
            candidates.len() == 1,
            "Creation outcome unknown; parent/marker reconciliation required"
        );
        let candidate = candidates.pop().context("Creation outcome unknown")?;
        ensure!(
            canonical(content) == canonical(text(&candidate, "content")?),
            "Created content did not survive readback"
        );
        merge(
            &mut state,
            json!({"phase":"verified","page_id":candidate["id"],"verified_at":timestamp()}),
        );
        write_json(&path, &state)?;
        Ok(candidate)
    }

    pub fn replace(
        &mut self,
        identifier: &str,
        content: &str,
        operation_key: &str,
        expected_before: Option<&str>,
    ) -> Result<Value> {
        let mut key = operation_key.to_string();
        let mut expected_before = expected_before.map(str::to_string);
        let child_page_pattern = Regex::new(r"(?s)<page\b[^>]*>.*?</page>")?;
        for _ in 0..64 {
            let path = self.root.join(format!("{key}.json"));
            let mut state = read_json(&path, json!({}))?;
            if let Some(next) = state["superseded_by"].as_str() {
                key = next.to_string();
                continue;
            }
            let current = self.page(identifier)?;
            let current_content = text(&current, "content")?;
            if let Some(after) = state["after"].as_str()
                && canonical(after) != canonical(content)
            {
                let observed = canonical(current_content);
                ensure!(
                    observed == canonical(state["before"].as_str().unwrap_or(""))
                        || observed == canonical(after),
                    "Earlier write must be reconciled before changing its target"
                );
                let revision = format!("{key}-revision-{}", &digest(content.as_bytes())[..16]);
                merge(
                    &mut state,
                    json!({"superseded_by":revision,"reconciled_at":timestamp(),"observed_sha256":digest(observed.as_bytes())}),
                );
                write_json(&path, &state)?;
                key = revision;
                expected_before = Some(current_content.to_string());
                continue;
            }
            if canonical(current_content) == canonical(content) {
                merge(
                    &mut state,
                    json!({"phase":"verified","page_id":identifier,"after":content,"verified_at":timestamp()}),
                );
                write_json(&path, &state)?;
                return Ok(current);
            }
            let empty = state.as_object().is_some_and(|state| state.is_empty());
            if empty {
                if let Some(before) = &expected_before {
                    ensure!(
                        canonical(current_content) == canonical(before),
                        "Knowledge changed since analysis; preserve the human edit"
                    );
                }
                state = json!({"page_id":identifier,"before":current_content,"after":content});
            } else {
                ensure!(
                    state["phase"] != "verified",
                    "Verified page changed after write; preserve current content"
                );
                ensure!(
                    canonical(current_content) == canonical(text(&state, "before")?),
                    "Unknown write produced different content; reconcile"
                );
                ensure!(
                    state["attempts"].as_u64().unwrap_or(1) < 2,
                    "Write retry exhausted after unchanged readback"
                );
            }
            let children = child_page_pattern
                .find_iter(current_content)
                .map(|matched| matched.as_str())
                .collect::<Vec<_>>();
            let new_content = if children.is_empty() {
                content.to_string()
            } else {
                format!("{content}\n\n{}", children.join("\n\n"))
            };
            let attempts = state["attempts"].as_u64().unwrap_or(0) + 1;
            merge(
                &mut state,
                json!({"after":content,"phase":"writing","attempts":attempts,"at":timestamp()}),
            );
            write_json(&path, &state)?;
            self.spec()?;
            self.call("update", &json!({"page_id":identifier,"command":"replace_content","allow_async":false,"new_str":new_content}))?;
            let result = self.page(identifier)?;
            ensure!(
                canonical(content) == canonical(text(&result, "content")?),
                "Written content failed full readback"
            );
            merge(
                &mut state,
                json!({"phase":"verified","verified_at":timestamp()}),
            );
            write_json(&path, &state)?;
            return Ok(result);
        }
        bail!("Write operation revision chain is invalid")
    }

    pub fn append_once(
        &mut self,
        identifier: &str,
        addition: &str,
        marker: &str,
        key: &str,
    ) -> Result<Value> {
        let path = self.root.join(format!("{key}.json"));
        let mut state = read_json(&path, json!({}))?;
        let current = self.page(identifier)?;
        let content = text(&current, "content")?;
        if content.contains(marker) {
            ensure!(
                canonical(content).contains(&canonical(addition)),
                "Existing marker has different content"
            );
            return Ok(current);
        }
        if !state.as_object().is_some_and(|state| state.is_empty()) {
            ensure!(
                canonical(text(&state, "before")?) == canonical(content),
                "Page changed after unknown append; preserve it"
            );
        }
        let attempts = state["attempts"].as_u64().unwrap_or(0);
        ensure!(attempts < 2, "Append retry exhausted after readback");
        merge(
            &mut state,
            json!({"phase":"appending","before":content,"addition":addition,"attempts":attempts+1,"at":timestamp()}),
        );
        write_json(&path, &state)?;
        self.spec()?;
        self.call("update", &json!({"page_id":identifier,"command":"insert_content","allow_async":false,"position":{"type":"end"},"content":addition}))?;
        let result = self.page(identifier)?;
        ensure!(
            canonical(text(&result, "content")?) == canonical(&format!("{content}\n\n{addition}")),
            "Append readback mismatch"
        );
        merge(
            &mut state,
            json!({"phase":"verified","verified_at":timestamp()}),
        );
        write_json(&path, &state)?;
        Ok(result)
    }

    pub(crate) fn replace_fragment(
        &mut self,
        identifier: &str,
        old: &str,
        new: &str,
    ) -> Result<Value> {
        self.spec()?;
        if old.trim().is_empty() {
            self.call("update", &json!({"page_id":identifier,"command":"insert_content","allow_async":false,"position":{"type":"end"},"content":new}))
        } else {
            self.call("update", &json!({"page_id":identifier,"command":"update_content","allow_async":false,"content_updates":[{"old_str":old,"new_str":new}]}))
        }
    }
}
