use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct LegacyProjection {
    pub owner: Option<String>,
    pub account_status: String,
    pub projects: Vec<Value>,
    pub mappings: Map<String, Value>,
    pub assignments: Map<String, Value>,
    pub membership_hosts: Map<String, Value>,
    pub projectless: BTreeSet<String>,
    pub workspace_hints: Map<String, Value>,
    pub checkpoint: Value,
    pub pending_deletions: Map<String, Value>,
    pub references: Vec<Value>,
    pub sections: Vec<Value>,
    pub deleted_section_ids: BTreeSet<String>,
    pub issues: Vec<String>,
}

pub(crate) type Projection = LegacyProjection;

pub(crate) fn project_key(host: &str, origin: &str, identifier: &str) -> String {
    format!("target:{}", json!([host, origin, identifier]))
}

fn object(value: &Value, issues: &mut Vec<String>, label: &str) -> Map<String, Value> {
    if value.is_null() {
        Map::new()
    } else if let Some(entries) = value.as_object() {
        entries.clone()
    } else {
        issues.push(format!("invalid:{label}"));
        Map::new()
    }
}

pub(crate) fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn canonical_thread(identifier: &str) -> bool {
    uuid::Uuid::parse_str(identifier).is_ok_and(|parsed| parsed.to_string() == identifier)
}

impl LegacyProjection {
    pub fn parse(state: &Value, codex_home: &str, account: &Value) -> Result<Self> {
        ensure!(state.is_object(), "目标全局状态必须是对象");
        ensure!(!codex_home.is_empty(), "缺少目标 Codex Home");
        let mut projection = Self {
            owner: state["electron-renderer-content-owner"]
                .as_str()
                .map(str::to_owned),
            account_status: "accountUnresolved".into(),
            ..Self::default()
        };
        let Some(owner) = projection.owner.clone() else {
            projection.issues.push("missingOrInvalidOwner".into());
            return Ok(projection);
        };
        let (Some(user), Some(account_id)) = (
            account["userId"].as_str(),
            account["workspaceRouting"]["chatgptAccountId"].as_str(),
        ) else {
            projection.issues.push("accountUnresolved".into());
            return Ok(projection);
        };
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&[user, account_id])?)
        );
        if fingerprint != owner {
            projection.account_status = "identityMismatch".into();
            projection.issues.push("identityMismatch".into());
            return Ok(projection);
        }
        projection.account_status = "verified".into();
        projection.read_projects(state);
        let identity = format!("local:{codex_home}");
        projection.read_migration(state, &identity);
        projection.read_membership(state, &identity);
        projection.read_references(state, account_id, &identity);
        Ok(projection)
    }

    fn read_migration(&mut self, state: &Value, identity: &str) {
        self.checkpoint =
            self.owner_value(state, "app-server-projects-migration-by-host")[identity].clone();
        if let Some(checkpoint) = self.checkpoint.as_object_mut() {
            checkpoint.retain(|key, _| {
                [
                    "version",
                    "projectsMigrated",
                    "threadAssignmentsMigrated",
                    "threadAssignmentsReadMigrated",
                    "pendingThreadAssignmentIds",
                ]
                .contains(&key.as_str())
            });
        } else if !self.checkpoint.is_null() {
            self.issues.push("invalid:migrationCheckpoint".into());
            self.checkpoint = Value::Null;
        }
        self.pending_deletions = object(
            &self.owner_value(state, "app-server-pending-project-deletions-by-host")[identity],
            &mut self.issues,
            "pendingProjectDeletions",
        );
        for deletion in self.pending_deletions.values_mut() {
            if let Some(fields) = deletion.as_object_mut() {
                fields.retain(|key, _| ["projectId", "deleteAfterMs"].contains(&key.as_str()));
            }
        }
    }

    fn read_membership(&mut self, state: &Value, identity: &str) {
        self.workspace_hints = object(
            &self.owner_value(state, "thread-workspace-root-hints"),
            &mut self.issues,
            "workspaceHints",
        );
        let mappings = object(
            &state["app-server-project-id-by-legacy-project-id-by-host"][identity],
            &mut self.issues,
            "projectMappings",
        );
        self.mappings = mappings
            .into_iter()
            .filter(|(identifier, server)| {
                server.is_string()
                    && self
                        .projects
                        .iter()
                        .any(|project| project["id"] == *identifier)
            })
            .collect();
        self.assignments = object(
            &state["thread-project-assignments"],
            &mut self.issues,
            "assignments",
        );
        self.membership_hosts = object(
            &state["thread-project-membership-host-ids"],
            &mut self.issues,
            "membershipHosts",
        );
        for assignment in self.assignments.values_mut() {
            if let Some(fields) = assignment.as_object_mut() {
                fields.retain(|key, _| {
                    ["projectKind", "projectId", "projectOrigin", "hostId"].contains(&key.as_str())
                });
            }
        }
        self.projectless = strings(&state["projectless-thread-ids"]);
    }

    fn owner_value(&self, state: &Value, key: &str) -> Value {
        self.owner.as_ref().map_or_else(
            || state[key].clone(),
            |owner| state[format!("electron-owner-value:{key}:{owner}")].clone(),
        )
    }

    fn read_projects(&mut self, state: &Value) {
        let entries = object(&state["local-projects"], &mut self.issues, "localProjects");
        self.projects = entries
            .into_iter()
            .filter_map(|(identifier, mut project)| {
                if project["id"] != identifier
                    || !project["rootPaths"]
                        .as_array()
                        .is_some_and(|roots| roots.iter().all(Value::is_string))
                    || !project["name"].is_string()
                {
                    self.issues.push(format!("invalidProject:{identifier}"));
                    return None;
                }
                if let Some(fields) = project.as_object_mut() {
                    fields.retain(|key, _| {
                        ["id", "name", "rootPaths", "appearance"].contains(&key.as_str())
                    });
                }
                Some(project)
            })
            .collect();
        let order = state["project-order"].as_array();
        self.projects.sort_by_key(|project| {
            order
                .and_then(|items| {
                    items
                        .iter()
                        .position(|identifier| identifier == &project["id"])
                })
                .unwrap_or(usize::MAX)
        });
    }

    pub fn pending(&self, thread_id: &str) -> bool {
        strings(&self.checkpoint["pendingThreadAssignmentIds"]).contains(thread_id)
            || (self.checkpoint["threadAssignmentsMigrated"] != true
                && self.checkpoint["threadAssignmentsReadMigrated"] != true)
    }

    fn read_references(&mut self, state: &Value, account_id: &str, identity: &str) {
        let owner = self.owner.as_deref().unwrap_or_default();
        let mut atoms = object(
            &state[format!("electron-owner-atoms:{owner}")],
            &mut self.issues,
            "ownerAtoms",
        );
        atoms.extend(object(
            &state["electron-persisted-atom-state"],
            &mut self.issues,
            "liveAtoms",
        ));
        let custom = atoms
            .get("sidebar-custom-sections-v3")
            .map(|value| &value[account_id])
            .cloned()
            .unwrap_or(Value::Null);
        let hosts = object(&custom["threadHostIds"], &mut self.issues, "threadHostIds");
        let membership_hosts = object(
            &state["thread-project-membership-host-ids"],
            &mut self.issues,
            "membershipHosts",
        );
        let migrated_pins = strings(
            &self.owner_value(state, "app-server-migrated-pinned-thread-ids-by-host")[identity],
        );
        for identifier in strings(&state["pinned-thread-ids"]) {
            if !migrated_pins.contains(&identifier) {
                self.add_reference(&identifier, "pinned", None, &hosts, &membership_hosts);
            }
        }
        self.read_sections(&custom, &hosts, &membership_hosts);
    }

    fn read_sections(
        &mut self,
        custom: &Value,
        hosts: &Map<String, Value>,
        membership_hosts: &Map<String, Value>,
    ) {
        self.deleted_section_ids = strings(&custom["appServerDeletedSectionIds"]);
        let migrated = strings(&custom["appServerMigratedHostIds"]).contains("local");
        for section in custom["sections"].as_array().into_iter().flatten() {
            let Some(identifier) = section["id"].as_str() else {
                self.issues.push("invalidSectionId".into());
                continue;
            };
            let backend = section["hostSectionIds"]["local"].as_str();
            if backend.is_some_and(|identifier| self.deleted_section_ids.contains(identifier)) {
                continue;
            }
            self.sections
                .push(json!({"id":identifier,"name":section["name"],
                "backendId":backend,"migrated":migrated,"appearance":section["appearance"],
                "legacy":strings(&custom["appServerLegacySectionIds"]).contains(identifier)}));
            if migrated {
                continue;
            }
            for item in strings(&section["itemKeys"]) {
                if strings(&section["pendingDeletionHostIdsByItemKey"][&item]).contains("local") {
                    continue;
                }
                if let Some(thread_id) = item.strip_prefix("codex:thread:local:") {
                    self.add_reference(
                        thread_id,
                        "custom",
                        Some(identifier),
                        hosts,
                        membership_hosts,
                    );
                }
            }
        }
    }

    fn add_reference(
        &mut self,
        thread_id: &str,
        origin: &str,
        section_id: Option<&str>,
        hosts: &Map<String, Value>,
        membership_hosts: &Map<String, Value>,
    ) {
        if !canonical_thread(thread_id) {
            return;
        }
        let host_values = [hosts.get(thread_id), membership_hosts.get(thread_id)];
        if host_values
            .iter()
            .flatten()
            .any(|value| value.as_str() != Some("local"))
        {
            return;
        }
        let qualified = host_values
            .iter()
            .flatten()
            .any(|value| value.as_str() == Some("local"));
        self.references.push(
            json!({"id":thread_id,"threadId":thread_id,"origin":origin,"qualified":qualified,
            "logicalSectionId":section_id,"ownership":if qualified {"local"} else {"unqualified"}}),
        );
    }
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
