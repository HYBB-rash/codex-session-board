use super::discovery::{batch_snapshot, host_plan};
use serde_json::json;

#[test]
fn catalog_failure_or_last_project_removal_does_not_erase_known_hosts() {
    let known = vec!["mac".into()];
    let cached = vec![json!({"hostId":"other"})];
    let (hosts, error) = host_plan(&known, &cached, Err("Catalog failed".into()), &[]);
    assert_eq!(hosts, vec!["mac", "other"]);
    assert_eq!(error.as_deref(), Some("Catalog failed"));
    let (hosts, error) = host_plan(&known, &cached, Ok(vec![]), &[]);
    assert_eq!(hosts, vec!["mac", "other"]);
    assert!(error.is_none());
    let (hosts, error) = host_plan(&known, &cached, Ok(vec![]), &["mac".into()]);
    assert_eq!(hosts, vec!["other"]);
    assert!(error.is_none());
}

#[test]
fn batch_scope_is_frozen_and_project_ids_are_scoped_by_host() {
    let mut rows = vec![
        json!({"hostId":"local","projectId":"same","state":"to_archive","canArchive":true,"version":1}),
        json!({"hostId":"mac","projectId":"same","state":"to_archive","canArchive":true,"version":2}),
        json!({"hostId":"mac","projectId":"other","state":"to_archive","canArchive":true}),
        json!({"hostId":"mac","projectId":"same","state":"to_archive","canArchive":false}),
    ];
    let frozen = batch_snapshot(&rows, Some(("mac", "same")));
    rows[1]["version"] = json!(3);
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0]["version"], 2);
    assert_eq!(batch_snapshot(&rows, None).len(), 3);
}
