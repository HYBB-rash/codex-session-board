use super::*;
use crate::catalog::{legacy::Projection, projects, test_support::*};

fn assigned() -> Snapshot {
    let mut snapshot = snapshot();
    snapshot.source["accountFingerprint"] = json!("c".repeat(64));
    snapshot.projects = projects::catalog(
        &[
            json!({"id":"known","name":"Known","roots":["/known"]}),
            json!({"id":"guessed","name":"Guessed","roots":["/target/project"]}),
        ],
        &Projection::default(),
        HOST,
    );
    let project = snapshot.projects[0]["id"].clone();
    snapshot.active[0]["projectId"] = json!("known");
    snapshot.active[0]["catalogProjectId"] = project.clone();
    snapshot.active[0]["membership"] = json!({"state":"assigned","projectId":project,
        "basis":"backend","pending":false,"projectIssue":null});
    snapshot
}

fn missing(mut snapshot: Snapshot) -> Snapshot {
    snapshot.active[0]
        .as_object_mut()
        .unwrap()
        .remove("projectId");
    let project = snapshot.projects[1]["id"].clone();
    snapshot.active[0]["catalogProjectId"] = project.clone();
    snapshot.active[0]["membership"] = json!({"state":"assigned","projectId":project,
        "basis":"cwd","pending":false,"projectIssue":null});
    snapshot
}

#[test]
fn missing_membership_preserves_confirmed_project_instead_of_new_cwd_guess() -> Result<()> {
    let (_directory, app) = app()?;
    let original = assigned();
    let expected = original.active[0]["catalogProjectId"].clone();
    let at = original.captured_at;
    publish(&app, original.clone())?;
    for version in [json!("0.169.0"), json!("0.170.0"), serde_json::Value::Null] {
        let mut next = missing(original.clone());
        next.source["version"] = version;
        publish(&app, next)?;
        let state = app.catalog.lock().unwrap();
        let snapshot = state.snapshot.as_ref().unwrap();
        let row = &snapshot.active[0];
        assert_eq!(row["catalogProjectId"], expected);
        assert_eq!(row["membership"]["basis"], "previousObservation");
        assert_eq!(row["membership"]["projectIssue"], "backendProjectMissing");
        assert_eq!(row["membership"]["lastKnownAt"], at);
        assert_eq!(snapshot.coverage["membership"], "partial");
    }
    Ok(())
}

#[test]
fn explicit_null_new_source_and_deleted_project_do_not_retain_old_assignment() -> Result<()> {
    for change in ["null", "account", "peer", "deleted"] {
        let (_directory, app) = app()?;
        let original = assigned();
        publish(&app, original.clone())?;
        let mut next = missing(original);
        let expected = if change == "null" {
            serde_json::Value::Null
        } else {
            next.active[0]["catalogProjectId"].clone()
        };
        match change {
            "null" => {
                next.active[0]["projectId"] = json!(null);
                next.active[0]["catalogProjectId"] = json!(null);
                next.active[0]["membership"] = json!({"state":"unassigned","projectId":null,
                    "basis":"backendUnassigned","pending":false,"projectIssue":null});
            }
            "account" => next.source["accountFingerprint"] = json!("d".repeat(64)),
            "peer" => next.source["peerFingerprint"] = json!("another-peer"),
            "deleted" => {
                next.projects.remove(0);
            }
            _ => unreachable!(),
        }
        publish(&app, next)?;
        assert_eq!(
            app.catalog
                .lock()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .active[0]["catalogProjectId"],
            expected
        );
    }
    Ok(())
}

#[test]
fn known_projectless_state_survives_missing_and_current_legacy_intent_still_wins() -> Result<()> {
    let (_directory, app) = app()?;
    let mut original = assigned();
    original.active[0]["projectId"] = json!(null);
    original.active[0]["catalogProjectId"] = json!(null);
    original.active[0]["membership"] = json!({"state":"unassigned","projectId":null,
        "basis":"backendUnassigned","pending":false,"projectIssue":null});
    publish(&app, original.clone())?;
    publish(&app, missing(original.clone()))?;
    assert!(
        app.catalog
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .active[0]["catalogProjectId"]
            .is_null()
    );
    let mut next = missing(original);
    next.active[0]["membership"]["basis"] = json!("legacyAssignment");
    next.active[0]["membership"]["pending"] = json!(true);
    let expected = next.active[0]["catalogProjectId"].clone();
    publish(&app, next)?;
    let state = app.catalog.lock().unwrap();
    let row = &state.snapshot.as_ref().unwrap().active[0];
    assert_eq!(row["catalogProjectId"], expected);
    assert_eq!(row["membership"]["basis"], "legacyAssignment");
    Ok(())
}

#[test]
fn unknown_or_missing_account_identity_never_continues_previous_membership() -> Result<()> {
    for (previous_identity, next_identity) in [
        (json!(null), json!(null)),
        (json!("c".repeat(64)), json!(null)),
        (json!(null), json!("c".repeat(64))),
        (json!("invalid"), json!("invalid")),
    ] {
        let (_directory, app) = app()?;
        let mut original = assigned();
        original.source["accountFingerprint"] = previous_identity;
        publish(&app, original.clone())?;
        let mut next = missing(original);
        next.source["accountFingerprint"] = next_identity;
        let expected = next.active[0]["catalogProjectId"].clone();
        publish(&app, next)?;
        assert_eq!(
            app.catalog
                .lock()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .active[0]["catalogProjectId"],
            expected
        );
    }
    Ok(())
}

#[test]
fn old_snapshot_without_account_identity_cannot_continue_membership() -> Result<()> {
    let (_directory, app) = app()?;
    let mut original = assigned();
    original
        .source
        .as_object_mut()
        .unwrap()
        .remove("accountFingerprint");
    publish(&app, original.clone())?;
    let mut next = missing(original);
    next.source["accountFingerprint"] = json!(null);
    let expected = next.active[0]["catalogProjectId"].clone();
    publish(&app, next)?;
    assert_eq!(
        app.catalog
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .active[0]["catalogProjectId"],
        expected
    );
    Ok(())
}
