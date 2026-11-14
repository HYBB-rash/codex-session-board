use super::legacy::{Projection, project_key};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackendProject<'value> {
    Missing,
    Unassigned,
    Assigned(&'value str),
    Invalid,
}

impl<'value> BackendProject<'value> {
    pub fn from_thread(thread: &'value Value) -> Self {
        match thread.get("projectId") {
            None => Self::Missing,
            Some(Value::Null) => Self::Unassigned,
            Some(Value::String(identifier)) if !identifier.is_empty() => Self::Assigned(identifier),
            Some(_) => Self::Invalid,
        }
    }
}

fn result(
    state: &str,
    key: Option<String>,
    basis: &str,
    pending: bool,
    issue: Option<&str>,
) -> Value {
    json!({"state":state,"projectId":key,"basis":basis,"pending":pending,"projectIssue":issue})
}

pub(crate) fn legacy_key(
    identifier: &str,
    projects: &[Value],
    projection: &Projection,
    host: &str,
) -> Option<String> {
    if projection.pending_deletions.contains_key(identifier)
        || !projection
            .projects
            .iter()
            .any(|project| project["id"] == identifier)
    {
        return None;
    }
    let candidates: BTreeSet<_> = projects
        .iter()
        .filter(|project| {
            projection.mappings.get(identifier) == project.get("id")
                || project["metadata"]["codex.localProjectId"] == identifier
        })
        .filter_map(|project| project["id"].as_str())
        .collect();
    if candidates.len() == 1 {
        let server = candidates.first()?;
        if projection
            .mappings
            .iter()
            .any(|(legacy, mapped)| legacy != identifier && mapped == *server)
        {
            return None;
        }
        return Some(project_key(host, "targetServer", server));
    }
    if candidates.is_empty()
        && !projection.mappings.contains_key(identifier)
        && projection.checkpoint["version"] == 1
        && projection.checkpoint["projectsMigrated"] == false
        && projection
            .projects
            .iter()
            .any(|project| project["id"] == identifier)
    {
        return Some(project_key(host, "targetLegacy", identifier));
    }
    None
}

pub(crate) fn deleting(server: &str, projection: &Projection) -> bool {
    projection
        .pending_deletions
        .iter()
        .any(|(legacy, deletion)| {
            deletion["projectId"] == server
                || projection
                    .mappings
                    .get(legacy)
                    .is_some_and(|value| value == server)
        })
}

pub(crate) fn resolve(
    thread: &Value,
    projects: &[Value],
    projection: &Projection,
    host: &str,
) -> Value {
    let identifier = thread["id"]
        .as_str()
        .or_else(|| thread["threadId"].as_str())
        .unwrap_or_default();
    let pending = projection.pending(identifier);
    if thread["hostId"].as_str().is_some_and(|value| value != host) {
        return result("unresolved", None, "host", pending, Some("hostMismatch"));
    }
    let assignment = projection
        .assignments
        .get(identifier)
        .filter(|value| !value.is_null());
    if projection
        .membership_hosts
        .get(identifier)
        .is_some_and(|value| value != "local")
        || assignment
            .and_then(|value| value["hostId"].as_str())
            .is_some_and(|host| host != "local")
    {
        return result(
            "unresolved",
            None,
            "legacy",
            pending,
            Some("foreignMembership"),
        );
    }
    if assignment.is_some_and(|value| value["projectKind"] != "local") {
        return result(
            "unresolved",
            None,
            "legacy",
            pending,
            Some("foreignAssignment"),
        );
    }
    let legacy = assignment.and_then(|value| value["projectId"].as_str());
    if assignment.is_some() && legacy.is_none() {
        return result(
            "unresolved",
            None,
            "legacy",
            pending,
            Some("invalidLegacyAssignment"),
        );
    }
    let explicit_projectless = projection.projectless.contains(identifier);
    if explicit_projectless && legacy.is_some() {
        return result(
            "unresolved",
            None,
            "legacy",
            pending,
            Some("conflictingLegacyIntent"),
        );
    }
    let previous = legacy.and_then(|identifier| legacy_key(identifier, projects, projection, host));
    let backend = BackendProject::from_thread(thread);
    if pending || backend == BackendProject::Missing {
        if explicit_projectless {
            return result(
                "explicitlyProjectless",
                None,
                "legacyProjectless",
                pending,
                None,
            );
        }
        if legacy.is_some() {
            return previous.map_or_else(
                || {
                    result(
                        "unresolved",
                        None,
                        "legacy",
                        pending,
                        Some("unavailableLegacyProject"),
                    )
                },
                |key| result("assigned", Some(key), "legacyAssignment", pending, None),
            );
        }
    }
    if let Some(resolved) = backend_result(
        backend,
        projects,
        projection,
        host,
        pending,
        explicit_projectless,
        legacy.is_some(),
    ) {
        return resolved;
    }
    if projection.account_status != "verified" {
        return result(
            "unassigned",
            None,
            "legacyUnavailable",
            pending,
            Some("legacyUnavailable"),
        );
    }
    if legacy.is_some() {
        return result(
            "unresolved",
            None,
            "legacy",
            pending,
            Some("unavailableLegacyProject"),
        );
    }
    if let Some(hint) = projection
        .workspace_hints
        .get(identifier)
        .and_then(Value::as_str)
    {
        let hinted = infer(hint, projects, projection, host, pending, "workspaceHint");
        if hinted["state"] != "unassigned" {
            return hinted;
        }
    }
    infer(
        thread["cwd"].as_str().unwrap_or_default(),
        projects,
        projection,
        host,
        pending,
        "cwd",
    )
}

fn backend_result(
    backend: BackendProject<'_>,
    projects: &[Value],
    projection: &Projection,
    host: &str,
    pending: bool,
    explicit_projectless: bool,
    has_legacy: bool,
) -> Option<Value> {
    Some(match backend {
        BackendProject::Assigned(server) => {
            if !projects.iter().any(|project| project["id"] == server) {
                return Some(result(
                    "unresolved",
                    None,
                    "backend",
                    pending,
                    Some("unknownBackendProject"),
                ));
            }
            if deleting(server, projection) {
                return Some(result(
                    "unresolved",
                    None,
                    "backend",
                    pending,
                    Some("projectPendingDeletion"),
                ));
            }
            result(
                "assigned",
                Some(project_key(host, "targetServer", server)),
                "backend",
                pending,
                None,
            )
        }
        BackendProject::Invalid => result(
            "unresolved",
            None,
            "backend",
            pending,
            Some("invalidBackendProject"),
        ),
        BackendProject::Unassigned if explicit_projectless || has_legacy => result(
            "explicitlyProjectless",
            None,
            "backendCleared",
            pending,
            None,
        ),
        BackendProject::Unassigned if !pending => {
            result("unassigned", None, "backendUnassigned", pending, None)
        }
        _ => return None,
    })
}

fn path(value: &str) -> Option<String> {
    if !value.starts_with('/') {
        return None;
    }
    let mut components = Vec::new();
    for component in value.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    Some(format!("/{}", components.join("/")))
}

fn infer(
    cwd: &str,
    projects: &[Value],
    projection: &Projection,
    host: &str,
    pending: bool,
    basis: &str,
) -> Value {
    let Some(cwd) = path(cwd) else {
        return result("unassigned", None, "none", pending, None);
    };
    let mut candidates = BTreeSet::new();
    let mut longest = 0;
    let legacy_projects = projection.projects.iter().filter_map(|project| {
        let identifier = project["id"].as_str()?;
        let key = legacy_key(identifier, projects, projection, host)?;
        Some((key, project["rootPaths"].clone()))
    });
    let backend_projects = projects.iter().filter_map(|project| {
        let identifier = project["id"].as_str()?;
        if deleting(identifier, projection) {
            return None;
        }
        Some((
            project_key(host, "targetServer", identifier),
            project["roots"].clone(),
        ))
    });
    for (key, roots) in backend_projects.chain(legacy_projects) {
        for root in roots.as_array().into_iter().flatten() {
            let Some(root) = root
                .as_str()
                .or_else(|| root["path"].as_str())
                .and_then(path)
            else {
                continue;
            };
            if cwd != root && !cwd.starts_with(&format!("{}/", root.trim_end_matches('/'))) {
                continue;
            }
            if root.len() > longest {
                longest = root.len();
                candidates.clear();
            }
            if root.len() == longest {
                candidates.insert(key.clone());
            }
        }
    }
    match candidates.len() {
        0 => result("unassigned", None, "none", pending, None),
        1 => result(
            "assigned",
            candidates.into_iter().next(),
            basis,
            pending,
            None,
        ),
        _ => result("unresolved", None, basis, pending, Some("ambiguousRoots")),
    }
}

#[cfg(test)]
#[path = "membership_tests.rs"]
mod tests;
