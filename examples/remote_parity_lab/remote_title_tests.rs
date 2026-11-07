use super::{
    engine::{Partition, seed},
    surface::{THREAD, fixture},
};
use anyhow::Result;
use codex_session_board::titles;
use serde_json::json;

#[test]
fn latest_complete_title_survives_truncated_discovery_and_repeated_sync() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let row = partition.row(THREAD)?;
    partition.save(&mut surface, THREAD, "review", "备注", &row)?;
    surface.title = format!("其他标签 {}", "最新完整正文".repeat(100));
    partition.sync_title(&mut surface, THREAD)?;
    assert_eq!(
        surface.title,
        titles::projected_title(
            &format!("其他标签 {}", "最新完整正文".repeat(100)),
            "review"
        )?
    );
    assert!(surface.title.chars().count() > 300);
    let writes = surface.writes.len();
    partition.sync_title(&mut surface, THREAD)?;
    assert_eq!(surface.writes.len(), writes);
    Ok(())
}

#[test]
fn lost_title_receipt_survives_restart_and_readback_avoids_duplicate_write() -> Result<()> {
    let (root, partition, mut surface) = fixture("mac")?;
    partition.save(
        &mut surface,
        THREAD,
        "review",
        "备注",
        &partition.row(THREAD)?,
    )?;
    surface.lost = Some("thread/name/set");
    partition.sync_title(&mut surface, THREAD)?;
    assert_eq!(partition.store.pending_titles(None)?.len(), 1);
    assert_eq!(partition.row(THREAD)?["note"], "备注");
    drop(partition);
    let restarted = Partition::open(root.path(), "mac")?;
    let writes = surface.writes.len();
    restarted.sync_title(&mut surface, THREAD)?;
    assert_eq!(surface.writes.len(), writes);
    assert!(restarted.store.pending_titles(None)?.is_empty());
    Ok(())
}

#[test]
fn delayed_old_receipt_cannot_delete_a_newer_title_intent() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    partition.save(&mut surface, THREAD, "review", "", &partition.row(THREAD)?)?;
    let old = partition.store.pending_titles(None)?[0]["version"]
        .as_i64()
        .unwrap();
    partition.save(
        &mut surface,
        THREAD,
        "to_archive",
        "",
        &partition.row(THREAD)?,
    )?;
    assert!(
        !partition
            .store
            .title_synced(THREAD, old, "[待审阅] 旧标题")?
    );
    partition.sync_title(&mut surface, THREAD)?;
    assert!(surface.title.starts_with("[待归档]"));
    Ok(())
}

#[test]
fn metadata_updates_do_not_become_new_user_input() -> Result<()> {
    let (_root, partition, _surface) = fixture("mac")?;
    let before = partition.row(THREAD)?["inputVersion"].clone();
    let mut changed = seed("mac", THREAD);
    changed["updatedAt"] = json!(100);
    changed["title"] = json!("外部改名");
    partition.discover("mac", &[changed])?;
    assert_eq!(partition.row(THREAD)?["inputVersion"], before);
    Ok(())
}

#[test]
fn exposes_title_overwrite_between_read_and_unconditional_write() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    partition.save(&mut surface, THREAD, "review", "", &partition.row(THREAD)?)?;
    surface.race_title = Some("另一客户端刚改的正文".into());
    partition.sync_title(&mut surface, THREAD)?;
    assert_eq!(surface.title, "[待审阅] 完整标题");
    assert!(!surface.title.contains("另一客户端"));
    Ok(())
}

#[test]
fn incorrect_connection_host_is_rejected_before_any_write() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    surface.host = "local".into();
    assert!(partition.sync_title(&mut surface, THREAD).is_err());
    assert!(
        partition
            .archive(&mut surface, THREAD, &partition.row(THREAD)?)
            .is_err()
    );
    assert!(
        partition
            .save(&mut surface, THREAD, "review", "", &partition.row(THREAD)?)
            .is_err()
    );
    assert!(surface.writes.is_empty());
    Ok(())
}
