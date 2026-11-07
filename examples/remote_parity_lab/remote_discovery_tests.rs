use super::{
    discovery::{batch_snapshot, host_plan, scan_page},
    engine::{HostRpc, Partition, seed},
    surface::THREAD,
};
use anyhow::{Result, bail};
use codex_session_board::rpc::Rpc;
use serde_json::{Value, json};
use std::collections::VecDeque;

struct Pages {
    host: String,
    replies: VecDeque<Value>,
    cursors: Vec<Value>,
}

impl Rpc for Pages {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        assert_eq!(method, "list_threads");
        assert_eq!(params["hostId"], self.host);
        self.cursors.push(params["cursor"].clone());
        let reply = self.replies.pop_front().unwrap();
        if reply == "disconnected" {
            bail!("Disconnected");
        }
        Ok(reply)
    }
}

impl HostRpc for Pages {
    fn host(&self) -> &str {
        &self.host
    }
    fn execution_authority(&self) -> bool {
        false
    }
}

fn pages(host: &str, replies: Vec<Value>) -> Pages {
    Pages {
        host: host.into(),
        replies: replies.into(),
        cursors: vec![],
    }
}

#[test]
fn catalog_failure_or_last_project_removal_does_not_erase_known_hosts() {
    let known = vec!["mac".into()];
    let cached = vec![json!({"hostId":"other"})];
    let (hosts, error) = host_plan(&known, &cached, Err("Catalog failed".into()), &[]);
    assert_eq!(hosts, vec!["mac", "other"]);
    assert_eq!(error.as_deref(), Some("Catalog failed"));
    let (hosts, error) = host_plan(&known, &cached, Ok(vec![]), &["mac".into()]);
    assert_eq!(hosts, vec!["other"]);
    assert!(error.is_none());
}

#[test]
fn next_page_progress_survives_process_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let partition = Partition::open(root.path(), "mac")?;
    let mut reader = pages(
        "mac",
        vec![json!({"threads":[seed("mac",THREAD)],"nextCursor":"second"})],
    );
    assert!(!scan_page(&partition, &mut reader)?);
    drop(partition);
    let restarted = Partition::open(root.path(), "mac")?;
    let mut second = pages("mac", vec![json!({"threads":[],"nextCursor":null})]);
    assert!(scan_page(&restarted, &mut second)?);
    assert_eq!(second.cursors, vec![json!("second")]);
    assert_eq!(restarted.store.sessions()?.len(), 1);
    Ok(())
}

#[test]
fn failure_keeps_progress_and_does_not_prevent_another_host_page() -> Result<()> {
    let root = tempfile::tempdir()?;
    let first = Partition::open(root.path(), "mac")?;
    let other = Partition::open(root.path(), "other")?;
    let mut reader = pages(
        "mac",
        vec![
            json!({"threads":[seed("mac",THREAD)],"nextCursor":"continue"}),
            json!("disconnected"),
            json!({"threads":[],"nextCursor":null}),
        ],
    );
    assert!(!scan_page(&first, &mut reader)?);
    assert!(scan_page(&first, &mut reader).is_err());
    let mut healthy = pages(
        "other",
        vec![json!({"threads":[seed("other",THREAD)],"nextCursor":null})],
    );
    assert!(scan_page(&other, &mut healthy)?);
    assert!(scan_page(&first, &mut reader)?);
    assert_eq!(
        reader.cursors,
        vec![Value::Null, json!("continue"), json!("continue")]
    );
    assert_eq!(first.store.sessions()?.len(), 1);
    Ok(())
}

#[test]
fn duplicate_cursor_and_cross_host_page_fail_without_erasing_prior_data() -> Result<()> {
    let root = tempfile::tempdir()?;
    let partition = Partition::open(root.path(), "mac")?;
    let mut reader = pages(
        "mac",
        vec![
            json!({"threads":[seed("mac",THREAD)],"nextCursor":"same"}),
            json!({"threads":[],"nextCursor":"same"}),
            json!({"threads":[seed("other",THREAD)],"nextCursor":null}),
        ],
    );
    assert!(!scan_page(&partition, &mut reader)?);
    assert!(scan_page(&partition, &mut reader).is_err());
    assert!(scan_page(&partition, &mut reader).is_err());
    assert_eq!(partition.store.sessions()?.len(), 1);
    Ok(())
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
