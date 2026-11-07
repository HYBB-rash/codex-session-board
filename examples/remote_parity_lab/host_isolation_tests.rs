use super::engine::{Partition, seed};
use anyhow::Result;
use codex_session_board::native_sections::Observation;
use serde_json::json;

const THREAD: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn same_uuid_on_three_hosts_keeps_independent_durable_state() -> Result<()> {
    let root = tempfile::tempdir()?;
    for host in ["local", "mac", "other"] {
        let partition = Partition::open(root.path(), host)?;
        partition.discover(host, &[seed(host, THREAD)])?;
        partition.store.cache_native(
            THREAD,
            &Observation {
                state: "review".into(),
                version: "review-id".into(),
            },
        )?;
        partition
            .store
            .set_state(THREAD, Some("review"), host, 1, Some(0))?;
    }
    for host in ["local", "mac", "other"] {
        let partition = Partition::open(root.path(), host)?;
        assert_eq!(partition.row(THREAD)?["note"], host);
        assert_eq!(partition.store.pending_titles(None)?.len(), 1);
    }
    Ok(())
}

#[test]
fn mixed_host_page_is_rejected_without_partial_import() -> Result<()> {
    let root = tempfile::tempdir()?;
    let partition = Partition::open(root.path(), "mac")?;
    let page = [seed("mac", THREAD), seed("other", THREAD)];
    assert!(partition.discover("mac", &page).is_err());
    assert!(partition.store.sessions()?.is_empty());
    assert!(
        partition
            .discover("other", &[seed("other", THREAD)])
            .is_err()
    );
    Ok(())
}

#[test]
fn host_re_registration_requires_explicit_migration() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = Partition::open(root.path(), "connection-old")?;
    original.discover("connection-old", &[seed("connection-old", THREAD)])?;
    original
        .store
        .set_state(THREAD, Some("review"), "保留人工判断", 0, Some(0))?;
    let replacement = Partition::open(root.path(), "connection-new")?;
    replacement.discover("connection-new", &[seed("connection-new", THREAD)])?;
    assert_eq!(replacement.row(THREAD)?["note"], "");
    assert_eq!(original.row(THREAD)?["note"], "保留人工判断");
    Ok(())
}

#[test]
fn malformed_thread_and_traversal_host_cannot_escape_partition() -> Result<()> {
    let root = tempfile::tempdir()?;
    let partition = Partition::open(root.path(), "../../other")?;
    assert!(
        partition
            .discover(
                "../../other",
                &[json!({"id":"../escape","hostId":"../../other"})]
            )
            .is_err()
    );
    assert!(partition.store.sessions()?.is_empty());
    assert_eq!(std::fs::read_dir(root.path())?.count(), 1);
    Ok(())
}
