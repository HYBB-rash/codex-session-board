use super::{
    engine::{Partition, RuntimeEvidence},
    surface::{Surface, THREAD, fixture},
};
use anyhow::Result;
use codex_session_board::{model::now, native_sections::Observation};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

fn observe(
    partition: &Partition,
    surface: &mut Surface,
    status: &Value,
    turn: &Value,
) -> Result<()> {
    partition.runtime(
        surface,
        THREAD,
        RuntimeEvidence {
            generation: partition.generation.load(Ordering::SeqCst),
            status,
            turn,
            since: 100.0,
            clock_offset: 0.0,
        },
    )
}

#[test]
fn execution_approval_input_recovery_and_completion_reuse_the_same_state_rules() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let mut turn = json!({"id":"current","status":"inProgress","startedAt":now()+1.0});
    let cases = [
        (json!({"type":"active","activeFlags":[]}), "in_progress"),
        (
            json!({"type":"active","activeFlags":["waitingOnApproval"]}),
            "blocked",
        ),
        (json!({"type":"active","activeFlags":[]}), "in_progress"),
        (
            json!({"type":"active","activeFlags":["waitingOnUserInput"]}),
            "blocked",
        ),
        (json!({"type":"active","activeFlags":[]}), "in_progress"),
    ];
    for (status, expected) in cases {
        observe(&partition, &mut surface, &status, &turn)?;
        assert_eq!(partition.row(THREAD)?["state"], expected);
    }
    turn["status"] = json!("completed");
    turn["completedAt"] = json!(now() + 2.0);
    observe(&partition, &mut surface, &json!({"type":"idle"}), &turn)?;
    assert_eq!(partition.row(THREAD)?["state"], "review");
    Ok(())
}

#[test]
fn manual_choice_survives_repeated_phase_but_new_phase_takes_effect() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let turn = json!({"id":"current","status":"inProgress","startedAt":now()+1.0});
    let active = json!({"type":"active","activeFlags":[]});
    observe(&partition, &mut surface, &active, &turn)?;
    partition.save(
        &mut surface,
        THREAD,
        "to_archive",
        "人工决定",
        &partition.row(THREAD)?,
    )?;
    let writes = surface.writes.len();
    observe(&partition, &mut surface, &active, &turn)?;
    assert_eq!(surface.writes.len(), writes);
    assert_eq!(partition.row(THREAD)?["state"], "to_archive");
    observe(
        &partition,
        &mut surface,
        &json!({"type":"active","activeFlags":["waitingOnApproval"]}),
        &turn,
    )?;
    assert_eq!(partition.row(THREAD)?["state"], "blocked");
    assert_eq!(partition.row(THREAD)?["note"], "人工决定");
    Ok(())
}

#[test]
fn history_only_connection_cannot_drive_runtime_or_authorize_archive() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    surface.authoritative = false;
    let turn = json!({"id":"old","status":"completed","startedAt":1,"completedAt":2});
    assert!(
        observe(
            &partition,
            &mut surface,
            &json!({"type":"notLoaded"}),
            &turn
        )
        .is_err()
    );
    assert!(
        partition
            .archive(&mut surface, THREAD, &partition.row(THREAD)?)
            .is_err()
    );
    assert!(surface.writes.is_empty());
    assert_eq!(partition.row(THREAD)?["state"], "unknown");
    Ok(())
}

#[test]
fn late_snapshot_and_runtime_from_a_previous_connection_are_rejected() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    partition.reconnect();
    let old = Observation {
        state: "blocked".into(),
        version: "blocked-id".into(),
    };
    assert!(partition.cache_snapshot(0, THREAD, &old).is_err());
    let turn = json!({"id":"current","status":"inProgress","startedAt":now()+1.0});
    assert!(
        partition
            .runtime(
                &mut surface,
                THREAD,
                RuntimeEvidence {
                    generation: 0,
                    status: &json!({"type":"active"}),
                    turn: &turn,
                    since: 100.0,
                    clock_offset: 0.0
                }
            )
            .is_err()
    );
    assert_eq!(partition.row(THREAD)?["state"], "unknown");
    assert!(surface.writes.is_empty());
    Ok(())
}

#[test]
fn pinned_threads_are_not_moved_by_new_automatic_phases() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    surface.section = "pinned-id".into();
    partition.refresh(&mut surface, THREAD)?;
    let turn = json!({"id":"current","status":"inProgress","startedAt":now()+1.0});
    observe(&partition, &mut surface, &json!({"type":"active"}), &turn)?;
    assert_eq!(surface.section, "pinned-id");
    assert!(surface.writes.is_empty());
    Ok(())
}

#[test]
fn complete_user_ids_drive_review_and_survive_restart_without_metadata_false_alarms() -> Result<()>
{
    let (root, partition, mut surface) = fixture("mac")?;
    let mut turn = json!({"id":"turn","status":"completed","startedAt":1,"completedAt":2,"userItems":["first"]});
    partition.store.observe(THREAD, &turn, None)?;
    partition.save(
        &mut surface,
        THREAD,
        "to_archive",
        "已核对",
        &partition.row(THREAD)?,
    )?;
    let confirmed = partition.row(THREAD)?;
    turn["userItems"] = json!(["first", "second"]);
    partition.store.observe(THREAD, &turn, None)?;
    let changed = partition.row(THREAD)?;
    assert_eq!(changed["followUp"], true);
    assert!(changed["inputVersion"].as_i64() > confirmed["inputVersion"].as_i64());
    assert!(
        partition
            .save(&mut surface, THREAD, "to_archive", "", &confirmed)
            .is_err()
    );
    drop(partition);
    let restarted = Partition::open(root.path(), "mac")?;
    assert_eq!(restarted.row(THREAD)?["followUp"], true);
    assert_eq!(restarted.row(THREAD)?["note"], "已核对");
    Ok(())
}

#[test]
fn external_section_change_rejects_old_save_before_any_mutation() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let old = partition.row(THREAD)?;
    surface.section = "blocked-id".into();
    assert!(
        partition
            .save(&mut surface, THREAD, "review", "旧备注", &old)
            .is_err()
    );
    assert!(surface.writes.is_empty());
    assert_eq!(partition.row(THREAD)?["note"], "");
    Ok(())
}

#[test]
fn exposes_cross_host_clock_skew_in_wall_clock_baseline() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let turn = json!({"id":"new-but-clock-behind","status":"inProgress","startedAt":50});
    observe(&partition, &mut surface, &json!({"type":"active"}), &turn)?;
    assert_eq!(partition.row(THREAD)?["state"], "unknown");
    assert!(surface.writes.is_empty());
    Ok(())
}

#[test]
fn calibrated_host_clock_reuses_existing_rules_without_a_second_state_machine() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let turn = json!({"id":"new-but-clock-behind","status":"inProgress","startedAt":50});
    partition.runtime(
        &mut surface,
        THREAD,
        RuntimeEvidence {
            generation: 0,
            status: &json!({"type":"active"}),
            turn: &turn,
            since: 100.0,
            clock_offset: 100.0,
        },
    )?;
    assert_eq!(partition.row(THREAD)?["state"], "in_progress");
    Ok(())
}

#[test]
fn exposes_aba_moves_without_a_server_revision() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let old = partition.row(THREAD)?;
    surface.section = "blocked-id".into();
    surface.section = "unknown-id".into();
    partition.save(&mut surface, THREAD, "review", "无法发现中间移动", &old)?;
    assert_eq!(partition.row(THREAD)?["state"], "review");
    assert_eq!(surface.writes.len(), 1);
    Ok(())
}

#[test]
fn snapshot_started_before_a_successful_save_cannot_overwrite_the_confirmation() -> Result<()> {
    let (_root, partition, mut surface) = fixture("mac")?;
    let old_generation = partition.generation.load(Ordering::SeqCst);
    let old = Observation {
        state: "unknown".into(),
        version: "unknown-id".into(),
    };
    partition.save(
        &mut surface,
        THREAD,
        "review",
        "保留新确认",
        &partition.row(THREAD)?,
    )?;
    assert!(
        partition
            .cache_snapshot(old_generation, THREAD, &old)
            .is_err()
    );
    assert_eq!(partition.row(THREAD)?["state"], "review");
    assert_eq!(partition.row(THREAD)?["note"], "保留新确认");
    Ok(())
}
