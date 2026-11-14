use super::*;

fn fixture() -> (Projection, Vec<Value>) {
    let projection = Projection {
        account_status: "verified".into(),
        projects: vec![json!({"id":"old","name":"项目","rootPaths":["/repo"]})],
        mappings: serde_json::from_value(json!({"old":"server"})).unwrap_or_default(),
        assignments: serde_json::from_value(
            json!({"thread":{"projectKind":"local","projectId":"old"}}),
        )
        .unwrap_or_default(),
        checkpoint: json!({"version":1,"projectsMigrated":true,"threadAssignmentsMigrated":false}),
        ..Projection::default()
    };
    (
        projection,
        vec![json!({"id":"server","roots":[{"path":"/repo"}],"metadata":{}})],
    )
}

#[test]
fn pending_matrix_preserves_only_the_corresponding_unapplied_intent() {
    for migrated in [false, true] {
        for read in [false, true] {
            for in_pending in [false, true] {
                let (mut projection, projects) = fixture();
                projection.checkpoint = json!({"threadAssignmentsMigrated":migrated,
                    "threadAssignmentsReadMigrated":read,
                    "pendingThreadAssignmentIds":if in_pending {vec!["thread"]} else {vec![]}});
                let output = resolve(
                    &json!({"id":"thread","projectId":null,"cwd":"/elsewhere/worktree"}),
                    &projects,
                    &projection,
                    "remote:target",
                );
                let pending = in_pending || (!migrated && !read);
                assert_eq!(output["pending"], pending);
                assert_eq!(
                    output["state"],
                    if pending {
                        "assigned"
                    } else {
                        "explicitlyProjectless"
                    }
                );
                assert_eq!(
                    output["projectId"],
                    if pending {
                        json!(project_key("remote:target", "targetServer", "server"))
                    } else {
                        Value::Null
                    }
                );
            }
        }
    }
}

#[test]
fn missing_null_and_explicit_projectless_are_distinct() {
    let (mut projection, projects) = fixture();
    projection.checkpoint =
        json!({"threadAssignmentsMigrated":true,"threadAssignmentsReadMigrated":true});
    assert_eq!(
        resolve(
            &json!({"id":"thread","cwd":"/elsewhere"}),
            &projects,
            &projection,
            "host"
        )["basis"],
        "legacyAssignment"
    );
    assert_eq!(
        resolve(
            &json!({"id":"thread","projectId":null,"cwd":"/repo"}),
            &projects,
            &projection,
            "host"
        )["basis"],
        "backendCleared"
    );
    projection.assignments.clear();
    assert_eq!(
        resolve(
            &json!({"id":"thread","projectId":null,"cwd":"/repo"}),
            &projects,
            &projection,
            "host"
        )["basis"],
        "backendUnassigned"
    );
    projection.checkpoint =
        json!({"threadAssignmentsMigrated":false,"threadAssignmentsReadMigrated":false});
    assert_eq!(
        resolve(
            &json!({"id":"thread","projectId":null,"cwd":"/repo"}),
            &projects,
            &projection,
            "host"
        )["basis"],
        "cwd"
    );
    projection.projectless.insert("thread".into());
    assert_eq!(
        resolve(
            &json!({"id":"thread","projectId":null,"cwd":"/repo"}),
            &projects,
            &projection,
            "host"
        )["state"],
        "explicitlyProjectless"
    );
    assert_eq!(
        BackendProject::from_thread(&json!({})),
        BackendProject::Missing
    );
    assert_eq!(
        BackendProject::from_thread(&json!({"projectId":null})),
        BackendProject::Unassigned
    );
}

#[test]
fn deleted_mapping_foreign_assignment_and_host_conflicts_are_unresolved() {
    let (mut projection, projects) = fixture();
    assert_eq!(
        resolve(
            &json!({"id":"thread","projectId":null}),
            &[],
            &projection,
            "host"
        )["projectIssue"],
        "unavailableLegacyProject"
    );
    projection
        .pending_deletions
        .insert("old".into(), json!({"projectId":"server"}));
    assert_eq!(
        resolve(
            &json!({"id":"other","projectId":"server"}),
            &projects,
            &projection,
            "host"
        )["projectIssue"],
        "projectPendingDeletion"
    );
    projection.assignments.insert(
        "thread".into(),
        json!({"projectKind":"remote","projectId":"old"}),
    );
    assert_eq!(
        resolve(
            &json!({"id":"thread","cwd":"/repo"}),
            &projects,
            &projection,
            "host"
        )["projectIssue"],
        "foreignAssignment"
    );
    assert_eq!(
        resolve(
            &json!({"id":"thread","hostId":"foreign"}),
            &projects,
            &projection,
            "host"
        )["projectIssue"],
        "hostMismatch"
    );
}

#[test]
fn remote_paths_use_longest_root_and_preserve_same_directory_conflicts() {
    let (mut projection, mut projects) = fixture();
    projection.assignments.clear();
    projects.push(json!({"id":"inner","roots":[{"path":"/repo/inner"},{"path":"/second"}]}));
    let thread = json!({"id":"thread","cwd":"/repo/inner/file"});
    assert_eq!(
        resolve(&thread, &projects, &projection, "host")["projectId"],
        project_key("host", "targetServer", "inner")
    );
    projects.push(json!({"id":"same-path","roots":[{"path":"/repo/inner"}]}));
    assert_eq!(
        resolve(&thread, &projects, &projection, "host")["projectIssue"],
        "ambiguousRoots"
    );
    assert_eq!(
        resolve(
            &json!({"id":"thread","cwd":"/repo/../elsewhere"}),
            &projects,
            &projection,
            "host"
        )["state"],
        "unassigned"
    );
    assert_eq!(
        resolve(
            &json!({"id":"thread","cwd":"/repo-sibling"}),
            &projects,
            &projection,
            "host"
        )["state"],
        "unassigned"
    );
    projection
        .workspace_hints
        .insert("thread".into(), json!("/second"));
    assert_eq!(
        resolve(
            &json!({"id":"thread","cwd":"/worktree"}),
            &projects,
            &projection,
            "host"
        )["basis"],
        "workspaceHint"
    );
}

#[test]
fn historical_mapping_cannot_restore_a_legacy_only_project() {
    let (mut projection, _) = fixture();
    projection.checkpoint = json!({"version":1,"projectsMigrated":false});
    assert_eq!(
        resolve(&json!({"id":"thread"}), &[], &projection, "host")["state"],
        "unresolved"
    );
    projection.mappings.clear();
    assert_eq!(
        resolve(&json!({"id":"thread"}), &[], &projection, "host")["projectId"],
        project_key("host", "targetLegacy", "old")
    );
    projection.checkpoint = json!({});
    assert_eq!(
        resolve(&json!({"id":"thread"}), &[], &projection, "host")["state"],
        "unresolved"
    );
}

#[test]
fn unavailable_legacy_overlay_does_not_infer_away_unknown_manual_intent() {
    let (_, projects) = fixture();
    for thread in [
        json!({"id":"thread","cwd":"/repo"}),
        json!({"id":"thread","cwd":"/repo","projectId":null}),
    ] {
        let output = resolve(&thread, &projects, &Projection::default(), "host");
        assert_eq!(output["state"], "unassigned");
        assert_eq!(output["projectIssue"], "legacyUnavailable");
    }
    let assigned = resolve(
        &json!({"id":"thread","projectId":"server"}),
        &projects,
        &Projection::default(),
        "host",
    );
    assert_eq!(assigned["basis"], "backend");
}
