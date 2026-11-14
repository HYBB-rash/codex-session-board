use super::{
    Snapshot, input,
    paging::{ALL_SOURCES, PageScan, ordinary_params},
};
use crate::rpc::{RequestError, Rpc};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Capture {
    pub source: Value,
    pub initial: input::Input,
    pub jobs: Vec<PageScan>,
    pub expanded: bool,
    pub archive_check: Option<usize>,
    pub reference_index: usize,
    pub references: Vec<Value>,
    #[serde(default)]
    pub unresolved_references: Vec<Value>,
    #[serde(default)]
    pub metadata_checks: BTreeMap<String, Value>,
    pub issues: Vec<String>,
}

impl Capture {
    pub(crate) fn begin(reader: &mut dyn Rpc, source: Value, limit: usize) -> Result<Self> {
        let home = source["codexHome"]
            .as_str()
            .context("目录来源缺少 Codex Home")?;
        let initial = input::read(reader, home)?;
        let limit = limit.clamp(1, 100);
        Ok(Self {
            source,
            initial,
            jobs: vec![
                PageScan::new("project/list", json!({"limit":limit})),
                PageScan::new("thread/list", ordinary_params(limit, false)),
                PageScan::new("thread/list", ordinary_params(limit, true)),
                PageScan::new("threadSection/list", json!({"limit":limit})),
            ],
            expanded: false,
            archive_check: None,
            reference_index: 0,
            references: Vec::new(),
            unresolved_references: Vec::new(),
            metadata_checks: BTreeMap::new(),
            issues: Vec::new(),
        })
    }

    pub(crate) fn advance(
        &mut self,
        reader: &mut dyn Rpc,
        budget: usize,
    ) -> Result<Option<Snapshot>> {
        let host = self.source["hostId"]
            .as_str()
            .context("目录来源缺少主机")?
            .to_owned();
        for _ in 0..budget.max(1) {
            if !self.step(reader, &host)? {
                let home = self.source["codexHome"].as_str().context("缺少目录来源")?;
                let final_input = input::read(reader, home)?;
                ensure!(self.initial.hash == final_input.hash, "目录读取期间来源状态或账户发生变化");
                return self.snapshot(&host).map(Some);
            }
        }
        Ok(None)
    }

    fn step(&mut self, reader: &mut dyn Rpc, host: &str) -> Result<bool> {
        if !self.expanded && self.jobs.iter().all(|job| job.complete) {
            self.expand()?;
        }
        if let Some(index) = self.jobs.iter().position(|job| !job.complete) {
            let job = &mut self.jobs[index];
            job.step(reader, host)?;
            if self.archive_check == Some(index) {
                for row in &mut job.items {
                    *row = json!({"id":row["id"]});
                }
            }
            return Ok(true);
        }
        if self.step_reference(reader)? {
            return Ok(true);
        }
        if let Some(identifier) = self.next_conflict(host) {
            self.check_membership(reader, &identifier)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn step_reference(&mut self, reader: &mut dyn Rpc) -> Result<bool> {
        while let Some(candidate) = self.initial.projection.references.get(self.reference_index) {
            let identifier = candidate["id"].as_str().context("持久引用缺少会话身份")?.to_owned();
            if self.listed(&identifier) || self.references.iter().any(|row| row["id"] == identifier)
                || self.unresolved_references.iter().any(|row| row["id"] == identifier)
            {
                self.reference_index += 1;
                continue;
            }
            match reader.call("thread/read", json!({"threadId":identifier,"includeTurns":false})) {
                Ok(result) => self.accept_reference(&identifier, result.get("thread").context("会话元数据响应无效")?)?,
                Err(error) if error.downcast_ref::<RequestError>().is_some() => {
                    self.issues.push(format!("referenceUnavailable:{identifier}:{error}"));
                    self.unresolved_reference(&identifier, "unavailable");
                }
                Err(error) => return Err(error),
            }
            self.reference_index += 1;
            return Ok(true);
        }
        Ok(false)
    }

    fn supported_sources(&self) -> bool {
        self.source["version"].as_str().is_some_and(|version| version.starts_with("0.169."))
    }

    fn unresolved_reference(&mut self, identifier: &str, reason: &str) {
        self.unresolved_references.push(json!({"id":identifier,"reason":reason}));
        self.issues.push(format!("referenceUnresolved:{identifier}:{reason}"));
    }

    fn accept_reference(&mut self, identifier: &str, thread: &Value) -> Result<()> {
        ensure!(thread["id"] == identifier, "会话元数据身份不匹配");
        if let Some(reason) = self.reference_issue(thread) {
            self.unresolved_reference(identifier, reason);
        } else if eligible(thread) {
            let row = super::paging::project_fields(thread, "thread/list")?;
            self.metadata_checks.insert(identifier.to_owned(), row.clone());
            self.references.push(row);
        }
        Ok(())
    }

    fn reference_issue(&self, thread: &Value) -> Option<&'static str> {
        if !self.supported_sources() {
            Some("unsupportedArchiveScope")
        } else if !thread.get("attachedToThreadId").is_some_and(|value| value.is_null() || value.is_string())
            || !thread["ephemeral"].is_boolean()
        {
            Some("missingVisibilityEvidence")
        } else if !self.archive_check.is_some_and(|index| self.jobs[index].complete)
            || super::paging::source_projection(&thread["source"]).is_none()
        {
            Some("unprovenArchiveScope")
        } else {
            None
        }
    }

    fn rows(&self) -> impl Iterator<Item = &Value> {
        self.jobs.iter().enumerate()
            .filter(move |(index, job)| job.method == "thread/list" && self.archive_check != Some(*index))
            .flat_map(|(_, job)| &job.items).chain(&self.references)
    }

    fn next_conflict(&self, host: &str) -> Option<String> {
        self.rows().find_map(|row| {
            let identifier = row["id"].as_str()?;
            let membership = super::membership::resolve(row, &self.jobs[0].items, &self.initial.projection, host);
            (membership["projectIssue"] == "backendLegacyConflict"
                && !self.metadata_checks.contains_key(identifier)).then(|| identifier.to_owned())
        })
    }

    fn check_membership(&mut self, reader: &mut dyn Rpc, identifier: &str) -> Result<()> {
        let row = match reader.call("thread/read", json!({"threadId":identifier,"includeTurns":false})) {
            Ok(result) => {
                let thread = result.get("thread").context("归属元数据响应无效")?;
                ensure!(thread["id"] == identifier, "归属元数据身份不匹配");
                super::paging::project_fields(thread, "thread/list")?
            }
            Err(error) if error.downcast_ref::<RequestError>().is_some() => {
                self.issues.push(format!("membershipUnavailable:{identifier}:{error}"));
                Value::Null
            }
            Err(error) => return Err(error),
        };
        self.metadata_checks.insert(identifier.to_owned(), row);
        Ok(())
    }

    fn checked_row(&self, row: &Value) -> Value {
        let mut row = row.clone();
        if let Some(metadata) = row["id"].as_str().and_then(|identifier| self.metadata_checks.get(identifier)) {
            for field in ["projectId", "ephemeral", "attachedToThreadId", "threadSource"] {
                if let Some(value) = metadata.get(field) {
                    row[field] = value.clone();
                }
            }
        }
        row
    }

    fn membership(&self, row: &Value, host: &str) -> Value {
        let mut membership = super::membership::resolve(row, &self.jobs[0].items, &self.initial.projection, host);
        if membership["projectIssue"] == "backendLegacyConflict" {
            let verified = row["id"].as_str().and_then(|identifier| self.metadata_checks.get(identifier))
                .is_some_and(|metadata| metadata.get("projectId").is_some());
            if verified {
                membership["projectIssue"] = Value::Null;
                membership["basis"] = json!("backendMetadata");
            } else {
                membership["state"] = json!("unresolved");
                membership["projectId"] = Value::Null;
            }
        }
        membership
    }

    fn listed(&self, identifier: &str) -> bool {
        self.jobs
            .iter()
            .enumerate()
            .filter(|(index, job)| {
                job.method == "thread/list" && self.archive_check != Some(*index)
            })
            .map(|(_, job)| job)
            .flat_map(|job| &job.items)
            .any(|row| row["id"] == identifier)
    }

    fn expand(&mut self) -> Result<()> {
        let supported = self.supported_sources();
        let limit = self.jobs[1].params["limit"].clone();
        self.expand_sections(supported, &limit)?;
        let unresolved = self.initial.projection.references.iter().any(|row| {
            row["id"].as_str().is_some_and(|identifier| !self.listed(identifier))
        });
        if unresolved && supported {
            let mut params = ordinary_params(100, true);
            params["limit"] = limit;
            params["sourceKinds"] = json!(ALL_SOURCES);
            self.archive_check = Some(self.jobs.len());
            self.jobs.push(PageScan::new("thread/list", params));
        }
        if !supported {
            self.issues.push("sourceKindsUnsupportedVersion".into());
        }
        self.expanded = true;
        Ok(())
    }

    fn expand_sections(&mut self, supported: bool, limit: &Value) -> Result<()> {
        let sections = self.jobs[3].items.clone();
        for section in sections {
            let identifier = section["id"].as_str().context("分区缺少身份")?;
            if self
                .initial
                .projection
                .deleted_section_ids
                .contains(identifier)
            {
                continue;
            }
            for archived in [false, true] {
                for sources in [json!([]), json!(ALL_SOURCES)] {
                    if !supported && sources != json!([]) {
                        continue;
                    }
                    let mut params = ordinary_params(100, archived);
                    params["limit"] = limit.clone();
                    params["sectionId"] = json!(identifier);
                    params["sourceKinds"] = sources;
                    self.jobs.push(PageScan::new("thread/list", params));
                }
            }
        }
        Ok(())
    }

    fn snapshot(&self, host: &str) -> Result<Snapshot> {
        let projects = super::projects::catalog(&self.jobs[0].items, &self.initial.projection, host);
        let (active, archived, membership_issues) = self.snapshot_rows(host)?;
        let mut issues = self.initial.projection.issues.clone();
        issues.extend(self.issues.clone());
        issues.extend(membership_issues.clone());
        Ok(Snapshot {
            source: self.snapshot_source()?,
            captured_at: crate::model::now(), projects, active, archived,
            coverage: json!({"projects":"complete","ordinaryActive":"complete","ordinaryArchived":"complete",
                "publishedReferences":if issues.is_empty() {"complete"} else {"partial"},
                "membership":if membership_issues.is_empty() {"complete"} else {"partial"},
                "legacy":if self.initial.projection.issues.is_empty() {"complete"} else {"partial"}}),
            issues,
        })
    }

    fn snapshot_rows(&self, host: &str) -> Result<(Vec<Value>, Vec<Value>, Vec<String>)> {
        let ordinary = self.ordinary_ids();
        let archived = self.archived_ids();
        let mut active_rows = Vec::new();
        let mut archived_rows = Vec::new();
        let mut seen = BTreeSet::new();
        let mut membership_issues = Vec::new();
        for original in self.rows() {
            let mut row = self.checked_row(original);
            let identifier = row["id"].as_str().context("目录会话缺少身份")?;
            if !eligible(&row) || !seen.insert(identifier.to_owned()) {
                continue;
            }
            let archived = archived.contains(identifier);
            row["ordinary"] = json!(ordinary.contains(identifier));
            row["archived"] = json!(archived);
            row["membership"] = self.membership(&row, host);
            if let Some(issue) = row["membership"]["projectIssue"].as_str() {
                membership_issues.push(format!("membership:{}:{issue}", row["id"]));
            }
            if row.get("projectId").is_none() {
                membership_issues.push(format!("membership:{}:backendProjectMissing", row["id"]));
            }
            row["catalogProjectId"] = row["membership"]["projectId"].clone();
            if archived {
                archived_rows.push(row);
            } else {
                active_rows.push(row);
            }
        }
        Ok((active_rows, archived_rows, membership_issues))
    }

    fn ordinary_ids(&self) -> BTreeSet<&str> {
        self.jobs[1..3].iter().flat_map(|job| &job.items)
            .filter_map(|row| row["id"].as_str()).collect()
    }

    fn archived_ids(&self) -> BTreeSet<&str> {
        self.jobs.iter().filter(|job| job.params["archived"] == true)
            .flat_map(|job| &job.items).filter_map(|row| row["id"].as_str()).collect()
    }

    fn snapshot_source(&self) -> Result<Value> {
        let mut source = self.source.clone();
        source["generation"] = json!(input::digest(&serde_json::to_vec(&json!([
            self.source,
            self.initial.hash,
            self.jobs.iter().map(|job| &job.params).collect::<Vec<_>>()
        ]))?));
        source["stateHash"] = json!(self.initial.hash);
        source["stateMetadata"] = self.initial.metadata.clone();
        source["unresolvedReferences"] = json!(self.unresolved_references);
        Ok(source)
    }
}

fn eligible(row: &Value) -> bool {
    row["ephemeral"] != true
        && row["threadSource"] != "ambient_suggestions"
        && row["attachedToThreadId"].is_null()
        && row["id"].as_str().is_some_and(|identifier| {
            uuid::Uuid::parse_str(identifier).is_ok_and(|parsed| parsed.to_string() == identifier)
        })
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
