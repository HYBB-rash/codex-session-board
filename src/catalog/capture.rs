use super::{
    Snapshot, input,
    legacy::Projection,
    paging::{ALL_SOURCES, PageScan, ordinary_params},
};
use crate::rpc::{RequestError, Rpc};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Capture {
    pub source: Value,
    pub initial: input::Input,
    pub jobs: Vec<PageScan>,
    pub expanded: bool,
    pub archive_check: Option<usize>,
    pub reference_index: usize,
    pub references: Vec<Value>,
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
        let mut remaining = budget.max(1);
        loop {
            if let Some(index) = self.jobs.iter().position(|job| !job.complete) {
                if remaining == 0 {
                    return Ok(None);
                }
                let job = &mut self.jobs[index];
                job.step(reader, &host)?;
                if self.archive_check == Some(index) {
                    for row in &mut job.items {
                        *row = json!({"id":row["id"]});
                    }
                }
                remaining -= 1;
                continue;
            }
            if !self.expanded {
                self.expand()?;
                continue;
            }
            let candidates = &self.initial.projection.references;
            if self.reference_index < candidates.len() {
                let identifier = candidates[self.reference_index]["id"]
                    .as_str()
                    .context("持久引用缺少会话身份")?
                    .to_owned();
                if !self.listed(&identifier)
                    && !self.references.iter().any(|row| row["id"] == identifier)
                {
                    if remaining == 0 {
                        return Ok(None);
                    }
                    match reader.call(
                        "thread/read",
                        json!({"threadId":identifier,"includeTurns":false}),
                    ) {
                        Ok(result) => {
                            let thread = result.get("thread").context("会话元数据响应无效")?;
                            ensure!(thread["id"] == identifier, "会话元数据身份不匹配");
                            if eligible(thread) {
                                self.references
                                    .push(super::paging::project_fields(thread, "thread/list"));
                            }
                        }
                        Err(error) if error.downcast_ref::<RequestError>().is_some() => {
                            self.issues
                                .push(format!("referenceUnavailable:{identifier}:{error}"));
                        }
                        Err(error) => return Err(error),
                    }
                    remaining -= 1;
                }
                self.reference_index += 1;
                continue;
            }
            let home = self.source["codexHome"].as_str().context("缺少目录来源")?;
            let final_input = input::read(reader, home)?;
            ensure!(
                self.initial.hash == final_input.hash,
                "目录读取期间来源状态或账户发生变化"
            );
            return self.snapshot(&host).map(Some);
        }
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
        let supported = self.source["version"]
            .as_str()
            .is_some_and(|version| version.starts_with("0.169."));
        let limit = self.jobs[1].params["limit"].clone();
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
        let unresolved = self.initial.projection.references.iter().any(|row| {
            row["id"]
                .as_str()
                .is_some_and(|identifier| !self.listed(identifier))
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

    fn snapshot(&self, host: &str) -> Result<Snapshot> {
        let projects =
            super::projects::catalog(&self.jobs[0].items, &self.initial.projection, host);
        let ordinary: BTreeSet<_> = self.jobs[1..3]
            .iter()
            .flat_map(|job| &job.items)
            .filter_map(|row| row["id"].as_str())
            .collect();
        let archived: BTreeSet<_> = self
            .jobs
            .iter()
            .filter(|job| job.params["archived"] == true)
            .flat_map(|job| &job.items)
            .filter_map(|row| row["id"].as_str())
            .collect();
        let mut active_rows = Vec::new();
        let mut archived_rows = Vec::new();
        let mut seen = BTreeSet::new();
        for row in self
            .jobs
            .iter()
            .enumerate()
            .filter(|(index, job)| {
                job.method == "thread/list" && self.archive_check != Some(*index)
            })
            .map(|(_, job)| job)
            .flat_map(|job| &job.items)
            .chain(&self.references)
        {
            let identifier = row["id"].as_str().context("目录会话缺少身份")?;
            if !eligible(row) || !seen.insert(identifier) {
                continue;
            }
            let archived = archived.contains(identifier);
            let mut row = row.clone();
            row["ordinary"] = json!(ordinary.contains(identifier));
            row["archived"] = json!(archived);
            row["membership"] = super::membership::resolve(
                &row,
                &self.jobs[0].items,
                &self.initial.projection,
                host,
            );
            row["catalogProjectId"] = row["membership"]["projectId"].clone();
            if archived {
                archived_rows.push(row);
            } else {
                active_rows.push(row);
            }
        }
        let mut issues = self.initial.projection.issues.clone();
        issues.extend(self.issues.clone());
        let mut source = self.source.clone();
        source["generation"] = json!(input::digest(&serde_json::to_vec(&json!([
            self.source,
            self.initial.hash,
            self.jobs.iter().map(|job| &job.params).collect::<Vec<_>>()
        ]))?));
        source["stateHash"] = json!(self.initial.hash);
        source["stateMetadata"] = self.initial.metadata.clone();
        Ok(Snapshot {
            source,
            captured_at: crate::model::now(),
            projects,
            active: active_rows,
            archived: archived_rows,
            coverage: json!({"projects":"complete","ordinaryActive":"complete","ordinaryArchived":"complete",
                "publishedReferences":if issues.is_empty() {"complete"} else {"partial"},
                "legacy":if self.initial.projection.issues.is_empty() {"complete"} else {"partial"}}),
            issues,
        })
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
