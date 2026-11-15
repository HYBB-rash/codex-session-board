use super::Snapshot;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeSet;

const SCOPES: [&str; 6] = [
    "projects",
    "ordinaryActive",
    "ordinaryArchived",
    "publishedReferences",
    "membership",
    "legacy",
];

pub(super) fn complete(coverage: &Value) -> bool {
    SCOPES.iter().all(|scope| coverage[*scope] == "complete")
        && coverage
            .as_object()
            .is_some_and(|scopes| scopes.values().all(|scope| scope == "complete"))
}

pub(super) fn validate(snapshot: &Snapshot, host: &str) -> Result<()> {
    ensure!(
        SCOPES.iter().all(|scope| matches!(
            snapshot.coverage[*scope].as_str(),
            Some("complete" | "partial")
        )),
        "目录覆盖证据无效"
    );
    for field in ["generation", "stateHash"] {
        ensure!(
            snapshot.source[field].as_str().is_some_and(|hash| {
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            }),
            "目录缺少有效代次证据"
        );
    }
    let projects = project_ids(snapshot, host)?;
    validate_threads(snapshot, &projects)
}

fn project_ids(snapshot: &Snapshot, host: &str) -> Result<BTreeSet<String>> {
    let mut projects = BTreeSet::new();
    for project in &snapshot.projects {
        let identifier = project["id"].as_str().context("目录项目缺少身份")?;
        let fields: Vec<String> = serde_json::from_str(
            identifier
                .strip_prefix("target:")
                .context("目录项目来源无效")?,
        )?;
        ensure!(
            fields.len() == 3
                && fields[0] == host
                && matches!(fields[1].as_str(), "targetServer" | "targetLegacy")
                && !fields[2].is_empty()
                && project["hostId"] == host
                && project["origin"] == fields[1],
            "目录项目主机不匹配"
        );
        ensure!(projects.insert(identifier.to_owned()), "目录项目身份重复");
        ensure!(
            ["canCreate", "canMove", "canOpenProject"]
                .iter()
                .all(|permission| project[*permission] == false),
            "目录项目能力无效"
        );
    }
    Ok(projects)
}

fn validate_threads(snapshot: &Snapshot, projects: &BTreeSet<String>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (rows, archived) in [(&snapshot.active, false), (&snapshot.archived, true)] {
        for row in rows {
            let identifier = row["id"].as_str().context("目录会话缺少身份")?;
            ensure!(
                uuid::Uuid::parse_str(identifier)?.to_string() == identifier
                    && seen.insert(identifier.to_owned()),
                "目录会话身份无效或重复"
            );
            ensure!(
                row["ordinary"].is_boolean()
                    && row["archived"] == archived
                    && row["membership"].is_object(),
                "目录会话范围证据无效"
            );
            ensure!(
                row["catalogProjectId"].is_null()
                    || row["catalogProjectId"]
                        .as_str()
                        .is_some_and(|project| projects.contains(project)),
                "目录会话引用未核验项目"
            );
        }
    }
    Ok(())
}
