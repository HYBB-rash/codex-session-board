use super::Snapshot;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn retain(snapshot: &mut Snapshot, previous: &Snapshot) {
    if !same_source(snapshot, previous) {
        return;
    }
    let known: BTreeMap<_, _> = previous
        .active
        .iter()
        .chain(&previous.archived)
        .filter_map(|row| Some((row["id"].as_str()?, row)))
        .collect();
    let mut retained = false;
    for row in snapshot.active.iter_mut().chain(&mut snapshot.archived) {
        let Some(prior) = row["id"].as_str().and_then(|id| known.get(id)) else {
            continue;
        };
        if can_retain(row, prior, &snapshot.projects) {
            row["membership"] = prior["membership"].clone();
            row["membership"]["basis"] = json!("previousObservation");
            row["membership"]["projectIssue"] = json!("backendProjectMissing");
            row["membership"]["lastKnownAt"] = prior["membership"]
                .get("lastKnownAt")
                .cloned()
                .unwrap_or_else(|| json!(previous.captured_at));
            row["catalogProjectId"] = row["membership"]["projectId"].clone();
            retained = true;
        }
    }
    if retained {
        snapshot.coverage["membership"] = json!("partial");
        snapshot
            .issues
            .push("membership:backendProjectMissing".into());
    }
}

fn same_source(snapshot: &Snapshot, previous: &Snapshot) -> bool {
    snapshot.source["accountFingerprint"]
        .as_str()
        .is_some_and(|fingerprint| {
            fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        && [
            "hostId",
            "codexHome",
            "peerFingerprint",
            "version",
            "accountFingerprint",
        ]
        .iter()
        .all(|key| snapshot.source[key] == previous.source[key])
}

fn can_retain(row: &Value, prior: &Value, projects: &[Value]) -> bool {
    if row.get("projectId").is_some()
        || !matches!(
            row["membership"]["basis"].as_str(),
            Some("cwd" | "workspaceHint" | "none" | "legacyUnavailable")
        )
    {
        return false;
    }
    let membership = &prior["membership"];
    if !matches!(
        membership["basis"].as_str(),
        Some(
            "backend"
                | "backendUnassigned"
                | "backendCleared"
                | "legacyAssignment"
                | "legacyProjectless"
                | "previousObservation"
        )
    ) {
        return false;
    }
    if membership["projectIssue"]
        .as_str()
        .is_some_and(|issue| issue != "backendProjectMissing")
    {
        return false;
    }
    if let Some(project) = membership["projectId"].as_str() {
        return projects
            .iter()
            .any(|value| value["id"] == project && value["pendingDeletion"] != true);
    }
    matches!(
        membership["basis"].as_str(),
        Some("backendUnassigned" | "backendCleared" | "legacyProjectless" | "previousObservation")
    )
}
