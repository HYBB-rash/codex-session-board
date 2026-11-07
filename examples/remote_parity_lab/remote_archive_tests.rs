use super::{
    engine::{HostRpc, Partition},
    surface::{Surface, THREAD, fixture},
};
use anyhow::Result;
use codex_session_board::rpc::{Rpc, summarize_turn};
use serde_json::{Value, json};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn ready() -> Result<(tempfile::TempDir, Partition, Surface, Value)> {
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
fn active_or_unknown_execution_status_cannot_be_archived() -> Result<()> {
    for status in ["active", "systemError", "unknown"] {
        let (_root, partition, mut surface, row) = ready()?;
        surface.status = json!({"type":status});
        assert!(partition.archive(&mut surface, THREAD, &row).is_err());
        assert!(surface.writes.is_empty());
        assert!(!surface.archived);
    }
    Ok(())
}

#[test]
fn unseen_remote_input_and_incomplete_full_items_block_archive() -> Result<()> {
    for items in [json!([{"type":"userMessage","id":"late"}]), json!([{}])] {
        let (_root, partition, mut surface, row) = ready()?;
        surface.turn["items"] = items;
        assert!(partition.archive(&mut surface, THREAD, &row).is_err());
        assert!(!surface.archived);
        assert!(surface.writes.is_empty());
        assert_eq!(partition.row(THREAD)?["note"], "核对完毕");
    }
    Ok(())
}

#[test]
fn lost_archive_receipt_is_reconciled_after_restart_without_replay() -> Result<()> {
    let (root, partition, mut surface, row) = ready()?;
    surface.lost = Some("thread/archive");
    assert!(partition.archive(&mut surface, THREAD, &row).is_err());
    assert!(surface.archived);
    assert!(partition.row(THREAD).is_ok());
    drop(partition);
    let restarted = Partition::open(root.path(), "mac")?;
    let count = surface.writes.len();
    restarted.archive(&mut surface, THREAD, &row)?;
    assert_eq!(surface.writes.len(), count);
    assert!(restarted.store.sessions()?.is_empty());
    Ok(())
}

#[test]
fn changed_native_section_blocks_archive_without_local_confirmation() -> Result<()> {
    let (_root, partition, mut surface, row) = ready()?;
    surface.section = "review-id".into();
    assert!(partition.archive(&mut surface, THREAD, &row).is_err());
    assert!(surface.writes.is_empty());
    assert!(partition.row(THREAD).is_ok());
    Ok(())
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

struct Blocking {
    surface: Surface,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

impl HostRpc for Blocking {
    fn host(&self) -> &str {
        &self.surface.host
    }
    fn execution_authority(&self) -> bool {
        true
    }
}

impl Rpc for Blocking {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        if method == "move_thread_to_sidebar_section" {
            self.entered.send(())?;
            self.release.recv_timeout(Duration::from_secs(5))?;
        }
        self.surface.call(method, params)
    }
}

#[test]
fn blocked_remote_write_does_not_hold_other_host_database_or_operation_lock() -> Result<()> {
    let (_remote_root, remote, surface) = fixture("mac")?;
    let (_local_root, local, mut local_surface) = fixture("local")?;
    let (entered_sender, entered) = mpsc::channel();
    let (release, release_receiver) = mpsc::channel();
    let mut blocked = Blocking {
        surface,
        entered: entered_sender,
        release: release_receiver,
    };
    std::thread::scope(|scope| -> Result<()> {
        let worker = scope.spawn(move || {
            remote.save(&mut blocked, THREAD, "review", "远端", &remote.row(THREAD)?)
        });
        entered.recv_timeout(Duration::from_secs(3))?;
        let start = Instant::now();
        local.save(
            &mut local_surface,
            THREAD,
            "review",
            "本机",
            &local.row(THREAD)?,
        )?;
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_eq!(local.row(THREAD)?["note"], "本机");
        release.send(())?;
        worker.join().unwrap()?;
        Ok(())
    })
}
