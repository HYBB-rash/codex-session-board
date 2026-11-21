use super::{
    engine::Partition,
    remote_archive_tests::ready,
    surface::{Surface, THREAD, fixture},
};
use anyhow::Result;
use serde_json::Value;

#[test]
fn incorrect_connection_host_is_rejected_before_any_write() -> Result<()> {
    check_target_host("thread/name/set", |partition, surface, _| {
        partition.sync_title(surface, THREAD)
    })?;
    check_target_host("thread/archive", |partition, surface, row| {
        partition.archive(surface, THREAD, row)
    })?;
    check_target_host("move_thread_to_sidebar_section", |partition, surface, row| {
        partition.save(surface, THREAD, "review", "", row)
    })
}

fn check_target_host(
    method: &str,
    write: impl Fn(&Partition, &mut Surface, &Value) -> Result<()>,
) -> Result<()> {
    let (_root, partition, mut surface, row) = ready()?;
    surface.connection_host = "local".into();
    assert!(write(&partition, &mut surface, &row).is_err(), "{method}");
    assert!(surface.writes.is_empty(), "{method}");
    surface.connection_host = partition.host.clone();
    write(&partition, &mut surface, &row)?;
    assert_eq!(surface.writes, [method]);
    Ok(())
}
