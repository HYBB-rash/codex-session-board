#![allow(clippy::unwrap_used)]

use anyhow::Result;
use codex_session_board::store::{Store, remote::identity};
use serde_json::json;

const THREAD: &str = "11111111-1111-4111-8111-111111111111";

#[path = "remote_sessions/discovery.rs"]
mod discovery;

fn discover(store: &Store, host: &str, updated: i64) -> Result<()> {
    store.discover_remote(
        host,
        host,
        &[json!({
            "id":THREAD,"hostId":host,"title":"远程","updatedAt":updated
        })],
    )
}

#[test]
fn remote_identity_and_manual_data_survive_refresh_restart_without_touching_local() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), true)?;
    store.discover(&[json!({"id":THREAD,"name":"本机","updatedAt":1})])?;
    let local = store.sessions()?;
    for host in ["mac", "other:mac"] {
        discover(&store, host, 1)?;
    }
    let identifier = identity("mac", THREAD);
    let body =
        json!({"id":identifier,"state":"blocked","note":"等待设备","version":0,"inputVersion":0});
    store.save_remote(&body)?;
    assert!(store.save_remote(&body).is_err());
    discover(&store, "mac", 2)?;
    assert!(
        store
            .save_remote(&json!({
                "id":identifier,"state":"review","note":"过期","version":1,"inputVersion":0
            }))
            .is_err()
    );
    assert_eq!(store.sessions()?, local);
    assert!(store.pending_titles(None)?.is_empty());
    drop(store);
    let store = Store::open(directory.path(), true)?;
    let saved = store.remote_session(&identifier)?;
    assert_eq!(saved["note"], "等待设备");
    assert_eq!(saved["state"], "blocked");
    assert_eq!(saved["version"], 1);
    assert_eq!(store.remote_sessions()?.len(), 2);
    assert_eq!(
        store.remote_session(&identity("other:mac", THREAD))?["state"],
        "unknown"
    );
    Ok(())
}

#[test]
fn remote_validation_is_atomic_and_rejects_cross_host_data() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), false)?;
    assert!(
        store
            .discover_remote(
                "mac",
                "Mac",
                &[
                    json!({"id":THREAD,"hostId":"mac","title":"有效","updatedAt":1}),
                    json!({"id":THREAD,"hostId":"other","title":"错误","updatedAt":1}),
                ]
            )
            .is_err()
    );
    assert!(store.remote_sessions()?.is_empty());
    store.discover_remote(
        "mac",
        "Mac",
        &[json!({
            "id":THREAD,"hostId":"mac","title":"有效","updatedAt":1
        })],
    )?;
    for (state, note) in [
        ("blocked", ""),
        ("completed", "旧状态"),
        ("invalid", "无效"),
    ] {
        assert!(store.save_remote(&json!({
            "id":identity("mac",THREAD),"state":state,"note":note,"version":0,"inputVersion":0
        })).is_err());
    }
    assert_eq!(
        store.remote_session(&identity("mac", THREAD))?["version"],
        0
    );
    Ok(())
}
