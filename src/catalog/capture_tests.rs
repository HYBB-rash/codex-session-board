use super::*;
use crate::catalog::legacy::Projection;

fn capture(version: &str) -> Capture {
    let mut jobs = vec![
        PageScan::new("project/list", json!({})),
        PageScan::new("thread/list", ordinary_params(10, false)),
        PageScan::new("thread/list", ordinary_params(10, true)),
        PageScan::new("threadSection/list", json!({})),
        PageScan::new("thread/list", ordinary_params(10, true)),
    ];
    for job in &mut jobs {
        job.complete = true;
    }
    Capture {
        source: json!({"hostId":"local","codexHome":"/target","version":version}),
        initial: input::Input { hash:"state".into(), metadata:Value::Null, projection:Projection::default() },
        jobs, expanded:true, archive_check:Some(4), reference_index:0, references:Vec::new(),
        unresolved_references:Vec::new(), metadata_checks:BTreeMap::new(), issues:Vec::new(),
    }
}

#[test]
fn uncertain_references_never_become_active_and_remain_visible_as_unresolved() -> Result<()> {
    let identifier = uuid::Uuid::new_v4().to_string();
    let thread = json!({"id":identifier,"ephemeral":false,"attachedToThreadId":null,"source":"cli"});
    let mut unknown = capture("0.170.0");
    unknown.accept_reference(&identifier, &thread)?;
    let snapshot = unknown.snapshot("local")?;
    assert!(snapshot.active.is_empty());
    assert_eq!(snapshot.source["unresolvedReferences"][0]["id"], identifier);
    assert_eq!(snapshot.coverage["publishedReferences"], "partial");
    let mut missing = capture("0.169.0");
    let mut thread = thread;
    thread.as_object_mut().unwrap().remove("attachedToThreadId");
    missing.accept_reference(&identifier, &thread)?;
    assert!(missing.snapshot("local")?.active.is_empty());
    assert_eq!(missing.unresolved_references[0]["reason"], "missingVisibilityEvidence");
    Ok(())
}

#[test]
fn archive_checks_classify_references_without_publishing_the_check_inventory() -> Result<()> {
    let identifier = uuid::Uuid::new_v4().to_string();
    let unrelated = uuid::Uuid::new_v4().to_string();
    let mut capture = capture("0.169.0");
    capture.jobs[4].items = vec![json!({"id":identifier}), json!({"id":unrelated})];
    capture.accept_reference(&identifier, &json!({"id":identifier,"ephemeral":false,
        "attachedToThreadId":null,"source":{"subAgent":{"other":"never cache"}}}))?;
    let snapshot = capture.snapshot("local")?;
    assert!(snapshot.active.is_empty());
    assert_eq!(snapshot.archived.len(), 1);
    assert_eq!(snapshot.archived[0]["id"], identifier);
    assert!(!serde_json::to_string(&snapshot)?.contains("never cache"));
    Ok(())
}

fn conflicting_capture(identifier: &str) -> Capture {
    let mut capture = capture("0.169.0");
    capture.initial.projection.account_status = "verified".into();
    capture.initial.projection.checkpoint = json!({"threadAssignmentsMigrated":true});
    capture.initial.projection.projectless.insert(identifier.into());
    capture.jobs[0].items = vec![json!({"id":"backend","roots":[]})];
    capture.jobs[1].items = vec![json!({"id":identifier,"projectId":"backend"})];
    capture
}

struct Metadata(Value);

impl Rpc for Metadata {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        assert_eq!(method, "thread/read");
        assert_eq!(params, json!({"threadId":self.0["id"],"includeTurns":false}));
        Ok(json!({"thread":self.0}))
    }
}

#[test]
fn membership_conflicts_require_targeted_metadata_before_claiming_completeness() -> Result<()> {
    let identifier = uuid::Uuid::new_v4().to_string();
    let mut capture = conflicting_capture(&identifier);
    assert_eq!(capture.next_conflict("local"), Some(identifier.clone()));
    let snapshot = capture.snapshot("local")?;
    assert_eq!(snapshot.coverage["membership"], "partial");
    assert_eq!(snapshot.active[0]["membership"]["state"], "unresolved");
    capture.check_membership(&mut Metadata(json!({"id":identifier,"projectId":"backend"})), &identifier)?;
    assert!(capture.next_conflict("local").is_none());
    let snapshot = capture.snapshot("local")?;
    assert_eq!(snapshot.coverage["membership"], "complete");
    assert_eq!(snapshot.active[0]["membership"]["basis"], "backendMetadata");
    let mut missing = conflicting_capture(&identifier);
    missing.check_membership(&mut Metadata(json!({"id":identifier})), &identifier)?;
    assert_eq!(missing.snapshot("local")?.coverage["membership"], "partial");
    Ok(())
}
