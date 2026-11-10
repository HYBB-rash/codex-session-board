use super::*;
use crate::native_sections::Observation;
use crate::store::remote::identity;

fn fixture() -> Result<(tempfile::TempDir, App, App, String)> {
    let directory = tempfile::tempdir()?;
    let root = App::new(Config {
        data_dir: directory.path().join("root"),
        native_sections: true,
        sync_titles: false,
        ..Config::default()
    })?;
    let remote = App::new(Config {
        data_dir: directory.path().join("remote"),
        host_id: "remote:a".into(),
        native_sections: true,
        sync_titles: false,
        ..Config::default()
    })?;
    let thread = uuid::Uuid::new_v4().to_string();
    remote
        .store
        .discover(&[json!({"id":thread,"name":"已保存的空会话","updatedAt":1})])?;
    root.store.discover_remote(
        "remote:a",
        "远程",
        &[json!({
            "id":thread,"hostId":"remote:a","title":"旧缓存","updatedAt":1,
        })],
    )?;
    remote.discover(&[], true)?;
    Ok((directory, root, remote, thread))
}

fn merged(root: &App, remote: &App) -> Result<Vec<Value>> {
    let mut rows = root.store.remote_sessions()?;
    let legacy = rows.clone();
    root.append_managed_host(remote, &mut rows, &legacy)?;
    root.annotate_connections(&mut rows);
    Ok(rows)
}

#[test]
fn unpublished_empty_chat_survives_complete_scan_and_restart_as_read_only() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    remote
        .store
        .set_state(&thread, Some("review"), "保留的备注", 0, Some(0))?;
    let rows = merged(&root, &remote)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], identity("remote:a", &thread));
    assert_eq!(rows[0]["title"], "已保存的空会话");
    assert_eq!(rows[0]["note"], "保留的备注");
    assert_eq!(rows[0]["state"], "unknown");
    assert_eq!(rows[0]["inputVersion"], 0);
    assert_eq!(rows[0]["sectionVersion"], Value::Null);
    assert_eq!(rows[0]["readOnly"], false);
    assert_eq!(rows[0]["canArchive"], Value::Null);
    let remote_config = remote.config.clone();
    let root_config = root.config.clone();
    drop(remote);
    drop(root);
    let root = App::new(root_config)?;
    let remote = App::new(remote_config)?;
    remote.discover(&[], true)?;
    assert_eq!(merged(&root, &remote)?, rows);
    Ok(())
}

#[test]
fn published_chat_uses_confirmed_position_and_existing_missing_record_filter() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    remote.discover(
        &[json!({"id":thread,"name":"已保存的空会话","updatedAt":1})],
        true,
    )?;
    remote.store.cache_native(
        &thread,
        &Observation {
            state: "review".into(),
            version: "confirmed-section".into(),
        },
    )?;
    let rows = merged(&root, &remote)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "review");
    assert_eq!(rows[0]["sectionVersion"], "confirmed-section");
    assert_eq!(rows[0]["readOnly"], false);
    remote.discover(&[], true)?;
    assert!(merged(&root, &remote)?.is_empty());
    assert!(remote.store.contains(&thread)?);
    Ok(())
}

#[test]
fn archived_unpublished_chat_never_reappears_from_the_root_cache() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    let legacy = Store::open(&remote.config.data_dir, false)?;
    legacy.set_state(&thread, Some("to_archive"), "", 0, Some(0))?;
    let row = &legacy.sessions()?[0];
    legacy.archive(&thread, row["version"].as_i64().unwrap(), 0, |_| Ok(()))?;
    assert!(merged(&root, &remote)?.is_empty());
    remote.discover(
        &[json!({"id":thread,"name":"重复发现","updatedAt":2})],
        true,
    )?;
    assert!(merged(&root, &remote)?.is_empty());
    assert!(remote.store.has_session(&thread)?);
    assert!(!remote.store.contains(&thread)?);
    assert!(
        root.store
            .remote_session(&identity("remote:a", &thread))
            .is_ok()
    );
    Ok(())
}

#[test]
fn disabled_host_keeps_unpublished_cache_without_enabling_operations() -> Result<()> {
    let (_directory, root, remote, thread) = fixture()?;
    remote.deactivate()?;
    let rows = merged(&root, &remote)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["threadId"], thread);
    assert_eq!(rows[0]["readOnly"], true);
    assert_eq!(rows[0]["canArchive"], false);
    assert!(remote.operation().is_err());
    Ok(())
}
