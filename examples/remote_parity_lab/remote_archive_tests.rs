use super::{
    engine::Partition,
    surface::{Surface, THREAD, fixture},
};
use anyhow::Result;
use codex_session_board::rpc::summarize_turn;
use serde_json::Value;

pub(super) fn ready() -> Result<(tempfile::TempDir, Partition, Surface, Value)> {
    let (root, partition, mut surface) = fixture("mac")?;
    partition
        .store
        .observe(THREAD, &summarize_turn(&surface.turn), None)?;
    partition.save(
        &mut surface,
        THREAD,
        "to_archive",
        "核对完毕",
        &partition.row(THREAD)?,
    )?;
    let row = partition.row(THREAD)?;
    surface.writes.clear();
    Ok((root, partition, surface, row))
}

#[test]
fn lost_move_receipt_does_not_commit_notes_or_automatically_replay() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let row = partition.row(THREAD)?;
    surface.lost = Some("move_thread_to_sidebar_section");
    assert!(
        partition
            .save(&mut surface, THREAD, "review", "不能假报保存", &row)
            .is_err()
    );
    assert_eq!(surface.section, "review-id");
    assert_eq!(partition.row(THREAD)?["note"], "");
    let count = surface.writes.len();
    partition.refresh(&mut surface, THREAD)?;
    assert_eq!(surface.writes.len(), count);
    assert_eq!(partition.row(THREAD)?["state"], "review");
    Ok(())
}
