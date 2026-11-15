use super::*;
use anyhow::Context;

fn fixture() -> (Value, Value) {
    let account = json!({"userId":"user","workspaceRouting":{"chatgptAccountId":"account"}});
    let owner = format!("{:x}", Sha256::digest(br#"["user","account"]"#));
    let state = json!({
        "electron-renderer-content-owner":owner,
        "local-projects":{"old":{"id":"old","name":"项目","rootPaths":["/target/repo"]}},
        "app-server-project-id-by-legacy-project-id-by-host":{"local:/target/codex":{"old":"server","ghost":"deleted"}},
        (format!("electron-owner-value:app-server-projects-migration-by-host:{owner}")):{
            "local:/target/codex":{"version":1,"projectsMigrated":true,"threadAssignmentsMigrated":false}
        }
    });
    (state, account)
}

#[test]
fn verifies_identity_and_retains_only_current_mapping() -> Result<()> {
    let (mut state, account) = fixture();
    state["auth-token"] = json!("never copy");
    let projection = Projection::parse(&state, "/target/codex", &account)?;
    assert_eq!(projection.account_status, "verified");
    assert_eq!(projection.mappings.len(), 1);
    assert_eq!(projection.projects.len(), 1);
    assert!(!serde_json::to_string(&projection)?.contains("never copy"));
    let mismatch = Projection::parse(
        &state,
        "/target/codex",
        &json!({"userId":"other","workspaceRouting":{"chatgptAccountId":"account"}}),
    )?;
    assert_eq!(mismatch.account_status, "identityMismatch");
    assert!(mismatch.projects.is_empty());
    assert!(mismatch.assignments.is_empty());
    state["local-projects"]["bad"] = json!({"id":"bad","name":"坏根","rootPaths":[123]});
    assert!(
        Projection::parse(&state, "/target/codex", &account)?
            .issues
            .contains(&"invalidProject:bad".into())
    );
    state["electron-renderer-content-owner"] = json!(33);
    assert_eq!(
        Projection::parse(&state, "/target/codex", &account)?.account_status,
        "accountUnresolved"
    );
    Ok(())
}

#[test]
fn owner_atoms_are_shallow_overridden_and_foreign_references_are_excluded() -> Result<()> {
    let (mut state, account) = fixture();
    let owner = state["electron-renderer-content-owner"]
        .as_str()
        .context("owner")?
        .to_owned();
    let local = "01984de2-8f74-7c91-a3b2-5c5e937cf311";
    let foreign = "01984de2-8f74-7c91-a3b2-5c5e937cf312";
    let unknown = "01984de2-8f74-7c91-a3b2-5c5e937cf313";
    state[format!("electron-owner-atoms:{owner}")] = json!({"sidebar-custom-sections-v3":{
        "account":{"sections":[{"id":"owner-section","itemKeys":[format!("codex:thread:local:{local}")]}]}
    }});
    let owner_only = Projection::parse(&state, "/target/codex", &account)?;
    assert_eq!(owner_only.references.len(), 1);
    state["electron-persisted-atom-state"] = json!({"sidebar-custom-sections-v3":{
        "account":{"sections":[{"id":"live-section","itemKeys":[format!("codex:thread:local:{local}"),format!("codex:thread:local:{foreign}"),format!("codex:thread:local:{unknown}")]}],
        "threadHostIds":{(local):"local",(foreign):"remote:foreign"}}
    }});
    let projection = Projection::parse(&state, "/target/codex", &account)?;
    assert_eq!(projection.sections[0]["id"], "live-section");
    assert_eq!(projection.references.len(), 2);
    assert_eq!(projection.references[0]["qualified"], true);
    assert_eq!(projection.references[1]["ownership"], "unqualified");
    state["thread-project-membership-host-ids"] = json!({(local):"remote:other"});
    assert_eq!(
        Projection::parse(&state, "/target/codex", &account)?
            .references
            .len(),
        1
    );
    Ok(())
}

#[test]
fn migrated_pins_use_target_namespace_without_stale_top_level_fallback() -> Result<()> {
    let (mut state, account) = fixture();
    let thread = "01984de2-8f74-7c91-a3b2-5c5e937cf311";
    state["pinned-thread-ids"] = json!([thread, "task_ignored", thread.to_uppercase()]);
    state["app-server-migrated-pinned-thread-ids-by-host"] =
        json!({"local:/target/codex":[thread]});
    assert_eq!(
        Projection::parse(&state, "/target/codex", &account)?
            .references
            .len(),
        1
    );
    let owner = state["electron-renderer-content-owner"]
        .as_str()
        .context("owner")?
        .to_owned();
    state[format!("electron-owner-value:app-server-migrated-pinned-thread-ids-by-host:{owner}")] = json!({
        "local:/target/codex":[thread],"remote:operator:/other":[]
    });
    assert!(
        Projection::parse(&state, "/target/codex", &account)?
            .references
            .is_empty()
    );
    assert_eq!(
        Projection::parse(&state, "/other", &account)?
            .references
            .len(),
        1
    );
    Ok(())
}

#[test]
fn deletion_and_migration_markers_do_not_resurrect_custom_members() -> Result<()> {
    let (mut state, account) = fixture();
    let thread = "01984de2-8f74-7c91-a3b2-5c5e937cf311";
    let item = format!("codex:thread:local:{thread}");
    state["electron-persisted-atom-state"] = json!({"sidebar-custom-sections-v3":{"account":{
        "sections":[
            {"id":"deleted","hostSectionIds":{"local":"backend-deleted"},"itemKeys":[item]},
            {"id":"kept","hostSectionIds":{"local":"backend-kept"},"itemKeys":[item],
                "pendingDeletionHostIdsByItemKey":{(item):["local"]}}
        ],"appServerDeletedSectionIds":["backend-deleted"],"appServerLegacySectionIds":["kept"]
    }}});
    let projection = Projection::parse(&state, "/target/codex", &account)?;
    assert_eq!(projection.sections.len(), 1);
    assert_eq!(projection.sections[0]["backendId"], "backend-kept");
    assert_eq!(projection.sections[0]["legacy"], true);
    assert!(projection.references.is_empty());
    state["electron-persisted-atom-state"]["sidebar-custom-sections-v3"]["account"]["sections"]
        [1]["pendingDeletionHostIdsByItemKey"] = json!({});
    state["electron-persisted-atom-state"]["sidebar-custom-sections-v3"]["account"]["appServerMigratedHostIds"] =
        json!(["local"]);
    assert!(
        Projection::parse(&state, "/target/codex", &account)?
            .references
            .is_empty()
    );
    Ok(())
}

fn malformed_projection_state() -> Result<(Value, Value)> {
    let (mut state, account) = fixture();
    let owner = state["electron-renderer-content-owner"]
        .as_str()
        .context("owner")?
        .to_owned();
    let body = json!({"private-body":{"text":"do not persist"}});
    state["thread-project-assignments"] = json!({
        "cleared":null, "assigned":{"projectKind":"local","projectId":"old","hostId":"local"},
        "bad":{"projectKind":body,"projectId":body,"hostId":body}, "badValue":[body]
    });
    state["thread-project-membership-host-ids"] = json!({"bad":body,"assigned":"local"});
    state["local-projects"]["old"]["appearance"] = body.clone();
    state[format!("electron-owner-value:thread-workspace-root-hints:{owner}")] =
        json!({"bad":body,"assigned":"/target/repo"});
    state[format!("electron-owner-value:app-server-projects-migration-by-host:{owner}")] = json!({
        "local:/target/codex":{"version":body,"projectsMigrated":body,
            "threadAssignmentsMigrated":false,"threadAssignmentsReadMigrated":body,
            "pendingThreadAssignmentIds":["assigned",body]}
    });
    state[format!("electron-owner-value:app-server-pending-project-deletions-by-host:{owner}")] = json!({"local:/target/codex":{"old":{"projectId":body,"deleteAfterMs":body},
            "valid":{"projectId":"server","deleteAfterMs":123},"badValue":[body]}});
    state["electron-persisted-atom-state"] = json!({"sidebar-custom-sections-v3":{"account":{
        "sections":[{"id":"section","name":body,"appearance":body}]
    }}});
    Ok((state, account))
}

#[test]
fn malformed_nested_values_are_not_persisted_in_the_projection() -> Result<()> {
    let (state, account) = malformed_projection_state()?;
    let projection = Projection::parse(&state, "/target/codex", &account)?;
    let persisted = serde_json::to_string(&projection)?;
    assert!(!persisted.contains("private-body"));
    assert!(!persisted.contains("do not persist"));
    assert!(!persisted.contains("appearance"));
    assert!(!projection.issues.is_empty());
    assert_eq!(projection.assignments["bad"], json!({}));
    assert_eq!(projection.assignments["badValue"], json!({}));
    assert_eq!(projection.membership_hosts["bad"], Value::Null);
    assert!(!projection.workspace_hints.contains_key("bad"));
    assert_eq!(projection.pending_deletions["old"], json!({}));
    assert_eq!(
        projection.pending_deletions["valid"],
        json!({"projectId":"server"})
    );
    assert!(projection.checkpoint.get("version").is_none());
    assert_eq!(
        projection.checkpoint["pendingThreadAssignmentIds"],
        json!(["assigned"])
    );
    assert_eq!(projection.sections[0]["name"], Value::Null);
    Ok(())
}

#[test]
fn sanitized_assignments_preserve_missing_null_and_assigned_states() -> Result<()> {
    let (state, account) = malformed_projection_state()?;
    let projection = Projection::parse(&state, "/target/codex", &account)?;
    assert!(!projection.assignments.contains_key("missing"));
    assert_eq!(projection.assignments.get("cleared"), Some(&Value::Null));
    assert_eq!(
        projection.assignments["assigned"],
        json!({"projectKind":"local","projectId":"old","hostId":"local"})
    );
    assert_eq!(projection.workspace_hints["assigned"], "/target/repo");
    assert_eq!(projection.checkpoint["threadAssignmentsMigrated"], false);
    assert!(projection.pending("assigned"));
    Ok(())
}

#[test]
fn malformed_projected_fields_report_validation_issues() -> Result<()> {
    let (state, account) = malformed_projection_state()?;
    let projection = Projection::parse(&state, "/target/codex", &account)?;
    for issue in [
        "invalid:assignment:bad:projectId",
        "invalid:assignment:badValue",
        "invalid:membershipHost:bad",
        "invalid:workspaceHint:bad",
        "invalid:migrationCheckpoint:projectsMigrated",
        "invalid:migrationCheckpoint:pendingThreadAssignmentId",
        "invalid:pendingDeletion:old:projectId",
        "invalid:sectionName:section",
    ] {
        assert!(projection.issues.iter().any(|actual| actual == issue));
    }
    Ok(())
}
