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
        initial: input::Input { hash:"state".into(), account_fingerprint:None, metadata:Value::Null, projection:Projection::default() },
        jobs, expanded:true, archive_check:Some(4), reference_index:0, references:Vec::new(),
        unresolved_references:Vec::new(), metadata_checks:BTreeMap::new(), issues:Vec::new(),
    }
}

#[test]
fn uncertain_references_never_become_active_and_remain_visible_as_unresolved() -> Result<()> {
    let identifier = uuid::Uuid::new_v4().to_string();
    let thread = json!({"id":identifier,"ephemeral":false,"attachedToThreadId":null,"source":"cli"});
    let mut unknown = capture("0.170.0");
    unknown.jobs[4].complete = false;
    unknown.accept_reference(&identifier, &thread)?;
    let snapshot = unknown.snapshot("local")?;
    assert!(snapshot.active.is_empty());
    assert_eq!(snapshot.source["unresolvedReferences"][0]["id"], identifier);
    assert_eq!(snapshot.coverage["publishedReferences"], "partial");
    let mut missing = capture("0.169.0");
    let mut thread = thread;
    thread.as_object_mut().context("测试会话应为对象")?.remove("attachedToThreadId");
    missing.accept_reference(&identifier, &thread)?;
    assert!(missing.snapshot("local")?.active.is_empty());
    assert_eq!(missing.unresolved_references[0]["reason"], "missingVisibilityEvidence");
    Ok(())
}

#[test]
fn archive_checks_classify_references_without_publishing_the_check_inventory() -> Result<()> {
    let identifier = uuid::Uuid::new_v4().to_string();
    let unrelated = uuid::Uuid::new_v4().to_string();
    for version in ["0.169.0", "0.170.0", "1.0.0", ""] {
        let mut capture = capture(version);
        capture.jobs[4].items = vec![json!({"id":identifier}), json!({"id":unrelated})];
        capture.accept_reference(&identifier, &json!({"id":identifier,"ephemeral":false,
            "attachedToThreadId":null,"source":{"subAgent":{"other":"never cache"}}}))?;
        let snapshot = capture.snapshot("local")?;
        assert!(snapshot.active.is_empty());
        assert_eq!(snapshot.archived.len(), 1);
        assert_eq!(snapshot.archived[0]["id"], identifier);
        assert!(!serde_json::to_string(&snapshot)?.contains("never cache"));
    }
    Ok(())
}

struct RejectedSource(Option<i64>);

impl Rpc for RejectedSource {
    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        let Some(code) = self.0 else { anyhow::bail!("connection lost"); };
        Err(RequestError { method:method.into(), details:json!({
            "code":code,"message":"source query rejected"}) }.into())
    }
}

#[test]
fn source_query_rejections_never_invent_archive_evidence_or_hide_transport_errors() -> Result<()> {
    for code in [Some(-32601), Some(-32602), Some(-32603), None] {
        let mut capture = capture("0.170.0");
        capture.jobs[4].complete = false;
        capture.jobs[4].params["sourceKinds"] = json!(ALL_SOURCES);
        let result = capture.step_page(&mut RejectedSource(code), "local", 4);
        if matches!(code, Some(-32601 | -32602)) {
            result?;
            assert!(capture.archive_check.is_none());
            assert!(capture.jobs[4].items.is_empty());
            assert_eq!(capture.snapshot("local")?.coverage["publishedReferences"], "partial");
        } else {
            assert!(result.is_err());
            assert_eq!(capture.archive_check, Some(4));
            assert!(!capture.jobs[4].complete);
        }
        assert!(capture.jobs[1].complete && capture.jobs[2].complete);
    }
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
    let mut reader = Metadata(json!({"id":identifier,"projectId":"backend"}));
    assert!(capture.step(&mut reader, "local")?);
    assert!(!capture.step(&mut reader, "local")?);
    assert!(capture.next_conflict("local").is_none());
    let snapshot = capture.snapshot("local")?;
    assert_eq!(snapshot.coverage["membership"], "complete");
    assert_eq!(snapshot.active[0]["membership"]["basis"], "backendMetadata");
    let mut missing = conflicting_capture(&identifier);
    missing.check_membership(&mut Metadata(json!({"id":identifier})), &identifier)?;
    assert_eq!(missing.snapshot("local")?.coverage["membership"], "partial");
    Ok(())
}

#[test]
fn references_outside_known_source_scope_keep_archive_status_unresolved() -> Result<()> {
    let identifier = uuid::Uuid::new_v4().to_string();
    let mut capture = capture("0.169.0");
    capture.accept_reference(&identifier, &json!({"id":identifier,"ephemeral":false,
        "attachedToThreadId":null,"source":{"custom":"unproven-source"}}))?;
    let snapshot = capture.snapshot("local")?;
    assert!(snapshot.active.is_empty() && snapshot.archived.is_empty());
    assert_eq!(snapshot.source["unresolvedReferences"][0]["reason"], "unprovenArchiveScope");
    assert!(!serde_json::to_string(&snapshot)?.contains("unproven-source"));
    Ok(())
}
