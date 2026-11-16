use super::*;
use std::collections::VecDeque;

struct Reader {
    replies: VecDeque<Value>,
    calls: Vec<(String, Value)>,
    reject_update: bool,
}

impl Rpc for Reader {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.calls.push((method.into(), params));
        if method == "thread/metadata/update" && self.reject_update {
            bail!("unsupported method");
        }
        self.replies.pop_front().context("unexpected RPC")
    }
}

fn project() -> Value {
    json!({"nativeId":"canonical","paths":["/remote/project"]})
}

fn thread() -> Value {
    json!({"id":"actual-uuid","cwd":"/remote/project","source":"vscode",
        "originator":"Codex Desktop","ephemeral":false,"projectId":null,
        "turns":[{"items":[{"type":"userMessage"}]}]})
}

fn reader(before: Value, after: Value) -> Reader {
    Reader { replies: VecDeque::from([
        json!({"project":{"id":"canonical","roots":[{"path":"/remote/project"}]}}),
        json!({"thread":before}), json!({}), json!({"thread":after}),
    ]), calls: Vec::new(), reject_update: false }
}

#[test]
fn assignment_reads_writes_and_confirms_only_the_selected_uuid() -> Result<()> {
    let mut assigned = thread();
    assigned["projectId"] = json!("canonical");
    let mut reader = reader(thread(), assigned);
    assign(&mut reader, "actual-uuid", &project())?;
    assert_eq!(reader.calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
        ["project/read","thread/read","thread/metadata/update","thread/read"]);
    for (_, params) in &reader.calls[1..] {
        assert_eq!(params["threadId"], "actual-uuid");
    }
    assert_eq!(reader.calls[2].1, json!({"threadId":"actual-uuid","projectId":"canonical"}));
    Ok(())
}

#[test]
fn assignment_retry_is_idempotent_and_never_recreates() -> Result<()> {
    let mut assigned = thread();
    assigned["projectId"] = json!("canonical");
    let mut reader = reader(assigned.clone(), assigned.clone());
    reader.replies.remove(2);
    assign(&mut reader, "actual-uuid", &project())?;
    assert_eq!(reader.calls.len(), 3);
    assert!(reader.calls.iter().all(|call| call.0 != "thread/metadata/update"));
    Ok(())
}

#[test]
fn assignment_rejects_changed_identity_directory_membership_source_and_missing_input() {
    for (key, value) in [("id", json!("other-uuid")), ("cwd", json!("/other")),
        ("projectId", json!("other-project")), ("projectId", json!(0)),
        ("source", json!("cli")), ("originator", json!("plugin")),
        ("ephemeral", json!(true)), ("archived", json!(true)), ("turns", json!([]))] {
        let mut before = thread();
        before[key] = value;
        let mut reader = reader(before, thread());
        assert!(assign(&mut reader, "actual-uuid", &project()).is_err(), "{key}");
        assert_eq!(reader.calls.len(), 2);
    }
}

#[test]
fn assignment_preserves_uuid_when_api_is_unsupported_or_readback_is_uncertain() {
    let mut unsupported = reader(thread(), thread());
    unsupported.reject_update = true;
    assert!(assign(&mut unsupported, "actual-uuid", &project()).is_err());
    assert_eq!(unsupported.calls.len(), 3);
    let mut uncertain = reader(thread(), thread());
    assert!(assign(&mut uncertain, "actual-uuid", &project()).is_err());
    assert_eq!(uncertain.calls.len(), 4);
    assert!(uncertain.calls.iter().all(|call| !matches!(call.0.as_str(),
        "thread/start" | "turn/start")));
}

#[test]
fn assignment_rejects_a_project_changed_since_the_catalog_snapshot() {
    let mut reader = reader(thread(), thread());
    reader.replies[0]["project"]["roots"] = json!([{"path":"/changed"}]);
    assert!(assign(&mut reader, "actual-uuid", &project()).is_err());
    assert_eq!(reader.calls.len(), 1);
}
