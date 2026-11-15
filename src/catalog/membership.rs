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

fn issue(state: &str, basis: &str, pending: bool, code: &str) -> Value {
    result(state, None, basis, pending, Some(code))
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
        if association_conflicts(server, identifier, projects, projection) {
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

fn association_conflicts(
    server: &str,
    identifier: &str,
    projects: &[Value],
    projection: &Projection,
) -> bool {
    projection
        .mappings
        .iter()
        .any(|(legacy, mapped)| legacy != identifier && mapped == server)
        || projects.iter().any(|project| {
            project["id"] == server
                && project["metadata"]["codex.localProjectId"]
                    .as_str()
                    .is_some_and(|legacy| legacy != identifier)
        })
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

fn legacy_intent<'projection>(
    identifier: &str,
    projection: &'projection Projection,
) -> Result<(Option<&'projection str>, bool), &'static str> {
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
        return Err("foreignMembership");
    }
    if assignment.is_some_and(|value| value["projectKind"] != "local") {
        return Err("foreignAssignment");
    }
    let legacy = assignment.and_then(|value| value["projectId"].as_str());
    if assignment.is_some() && legacy.is_none() {
        return Err("invalidLegacyAssignment");
    }
    let explicit_projectless = projection.projectless.contains(identifier);
    if explicit_projectless && legacy.is_some() {
        return Err("conflictingLegacyIntent");
    }
    Ok((legacy, explicit_projectless))
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
        return issue("unresolved", "host", pending, "hostMismatch");
    }
    let (legacy, projectless) = match legacy_intent(identifier, projection) {
        Ok(intent) => intent,
        Err(code) => return issue("unresolved", "legacy", pending, code),
    };
    let backend = BackendProject::from_thread(thread);
    if (pending || backend == BackendProject::Missing)
        && let Some(resolved) =
            legacy_result(legacy, projectless, projects, projection, host, pending)
    {
        return resolved;
    }
    if let Some(resolved) = backend_result(
        backend,
        projects,
        projection,
        host,
        pending,
        projectless,
        legacy,
    ) {
        return resolved;
    }
    infer_thread(identifier, thread, projects, projection, host, pending)
}

fn legacy_result(
    legacy: Option<&str>,
    projectless: bool,
    projects: &[Value],
    projection: &Projection,
    host: &str,
    pending: bool,
) -> Option<Value> {
    if projectless {
        return Some(result(
            "explicitlyProjectless",
            None,
            "legacyProjectless",
            pending,
            None,
        ));
    }
    legacy.map(|identifier| {
        legacy_key(identifier, projects, projection, host).map_or_else(
            || issue("unresolved", "legacy", pending, "unavailableLegacyProject"),
            |key| result("assigned", Some(key), "legacyAssignment", pending, None),
        )
    })
}

fn infer_thread(
    identifier: &str,
    thread: &Value,
    projects: &[Value],
    projection: &Projection,
    host: &str,
    pending: bool,
) -> Value {
    if projection.account_status != "verified" {
        return issue(
            "unassigned",
            "legacyUnavailable",
            pending,
            "legacyUnavailable",
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
    legacy: Option<&str>,
) -> Option<Value> {
    Some(match backend {
        BackendProject::Assigned(server) => backend_assignment(
            server,
            projects,
            projection,
            host,
            pending,
            explicit_projectless,
            legacy,
        ),
        BackendProject::Invalid => issue("unresolved", "backend", pending, "invalidBackendProject"),
        BackendProject::Unassigned if explicit_projectless || legacy.is_some() => result(
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

fn backend_assignment(
    server: &str,
    projects: &[Value],
    projection: &Projection,
    host: &str,
    pending: bool,
    explicit_projectless: bool,
    legacy: Option<&str>,
) -> Value {
    if !projects.iter().any(|project| project["id"] == server) {
        return issue("unresolved", "backend", pending, "unknownBackendProject");
    }
    if deleting(server, projection) {
        return issue("unresolved", "backend", pending, "projectPendingDeletion");
    }
    let key = project_key(host, "targetServer", server);
    let conflict = !pending
        && (explicit_projectless
            || legacy.is_some_and(|identifier| {
                legacy_key(identifier, projects, projection, host)
                    .is_none_or(|previous| previous != key)
            }));
    result(
        "assigned",
        Some(key),
        "backend",
        pending,
        conflict.then_some("backendLegacyConflict"),
    )
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

fn root_candidates(
    projects: &[Value],
    projection: &Projection,
    host: &str,
) -> Vec<(String, Value)> {
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
    backend_projects.chain(legacy_projects).collect()
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
    for (key, roots) in root_candidates(projects, projection, host) {
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
