use super::surface::{THREAD, fixture};
use anyhow::Result;

#[test]
fn incorrect_connection_host_is_rejected_before_any_write() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    partition.save(&mut surface, THREAD, "review", "", &partition.row(THREAD)?)?;
    assert_eq!(partition.store.pending_titles(Some(THREAD))?.len(), 1);
    let prior_writes = surface.writes.clone();
    surface.host = "local".into();
    assert!(partition.sync_title(&mut surface, THREAD).is_err());
    assert!(
        partition
            .archive(&mut surface, THREAD, &partition.row(THREAD)?)
            .is_err()
    );
    assert!(
        partition
            .save(&mut surface, THREAD, "to_archive", "", &partition.row(THREAD)?)
            .is_err()
    );
    assert_eq!(surface.writes, prior_writes);
    Ok(())
}
