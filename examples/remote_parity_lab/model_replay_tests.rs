use super::{
    engine::{Partition, RuntimeEvidence},
    surface::{Surface, THREAD, fixture_in},
};
use anyhow::Result;
use codex_session_board::model::{board_state, now};
use serde_json::{Value, json};
use std::process::Command;
use std::path::Path;
use std::sync::atomic::Ordering;
#[path = "../../tests/support/model_database.rs"]
mod model_database;
#[path = "../../tests/support/model_replay.rs"]
mod model_replay;
use model_replay::{ReplayCounts, TraceDatabase};

fn reference() -> Result<Value> {
    let output = Command::new("python3")
        .current_dir(std::env::current_dir()?)
        .args([
            "-c",
            r#"
import dataclasses,json
from acceptance.model import Flow,EVENTS,reachable_paths
traces=[]
for origin,path in reachable_paths():
    for event in EVENTS:
        flow=Flow()
        steps=[]
        for action in (*path,event):
            flow=flow.step(action)
            steps.append({"event":action,"expected":dataclasses.asdict(flow)})
        traces.append(steps)
print(json.dumps({"states":len(reachable_paths()),"events":len(EVENTS),"traces":traces}))
"#,
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn phase(partition: &Partition, surface: &mut Surface, event: &str, timestamp: f64) -> Result<()> {
    let (kind, flags, phase) = match event {
        "running" => ("active", vec![], "inProgress"),
        "approval" => ("active", vec!["waitingOnApproval"], "inProgress"),
        "input" => ("active", vec!["waitingOnUserInput"], "inProgress"),
        "error" => ("systemError", vec![], "inProgress"),
        "finished" => ("idle", vec![], "completed"),
        _ => panic!("Unknown reference event {event}"),
    };
    partition.runtime(
        surface,
        THREAD,
        RuntimeEvidence {
            generation: partition.generation.load(Ordering::SeqCst),
            status: &json!({"type":kind,"activeFlags":flags}),
            turn: &json!({"id":"model-turn","status":phase,"startedAt":timestamp,"completedAt":timestamp+1.0}),
            since: timestamp - 1.0,
            clock_offset: 0.0,
        },
    )
}

fn assert_model(partition: &Partition, expected: &Value, base: i64) -> Result<()> {
    let row = partition.row(THREAD)?;
    assert_eq!(
        row["state"],
        board_state(expected["state"].as_str().unwrap())
    );
    assert_eq!(row["manual"], expected["manual"]);
    assert_eq!(row["note"], expected["note"]);
    assert_eq!(
        row["version"].as_i64().unwrap() - base,
        expected["version"].as_i64().unwrap() - 1
    );
    Ok(())
}

#[test]
fn all_520_frozen_edges_replay_through_host_adapter_and_native_writes() -> Result<()> {
    let model = reference()?;
    assert_eq!(model["states"], 40);
    assert_eq!(model["events"], 13);
    let traces = model["traces"].as_array().unwrap();
    assert_eq!(traces.len(), 520);
    let root = tempfile::tempdir()?;
    let database = trace_database(root.path())?;
    let mut counts = ReplayCounts::default();
    for trace in traces {
        database.reset()?;
        counts += replay_trace(root.path(), trace)?;
    }
    database.reset()?;
    assert!(counts.steps >= 520);
    assert_eq!(counts, ReplayCounts { traces: 520, steps: 1417, restarts: 40, views: 40 });
    eprintln!("remote adapter: 40 states, 520 edges, {} compared steps", counts.steps);
    Ok(())
}

fn trace_database(root: &Path) -> Result<TraceDatabase> {
    let partition = Partition::open(root, "mac")?;
    let database = partition.store.connection()?;
    TraceDatabase::capture(&database, root)
}

fn snapshot(partition: &Partition) -> Result<model_database::Snapshot> {
    let database = partition.store.connection()?;
    Ok(model_database::snapshot(&database)?)
}

fn replay_trace(root: &Path, trace: &Value) -> Result<ReplayCounts> {
    let (mut partition, mut surface) = fixture_in(root, "mac")?;
    assert_eq!(partition.generation.load(Ordering::SeqCst), 0);
    let timestamp = now() + 100.0;
    phase(&partition, &mut surface, "running", timestamp)?;
    let initial = partition.row(THREAD)?;
    assert_eq!(initial["version"], 2);
    assert_eq!(initial["inputVersion"], 0);
    let base = initial["version"].as_i64().unwrap();
    assert_model(&partition, &json!({"state":"in_progress","manual":false,"note":"","version":1}), base)?;
    let mut counts = ReplayCounts::default();
    for step in trace.as_array().unwrap() {
        let action = step["event"].as_str().unwrap();
        let before = if matches!(action, "restart" | "view") {
            Some((snapshot(&partition)?, surface.clone(), partition.generation.load(Ordering::SeqCst)))
        } else { None };
        if action == "restart" {
            model_database::mark_restart(&*partition.store.connection()?)?;
            drop(partition);
            partition = Partition::open(root, "mac")?;
            assert!(!model_database::restart_marker_exists(&*partition.store.connection()?)?);
        } else if action != "view" {
            apply_action(&partition, &mut surface, action, timestamp)?;
        }
        assert_model(&partition, &step["expected"], base)?;
        if let Some((database, previous_surface, generation)) = before {
            assert_eq!(snapshot(&partition)?, database, "{action}");
            assert_eq!(surface, previous_surface, "{action}");
            assert_eq!(partition.generation.load(Ordering::SeqCst), if action == "restart" { 0 } else { generation });
        }
        counts.record(action);
    }
    counts.traces = 1;
    Ok(counts)
}

fn apply_action(partition: &Partition, surface: &mut Surface, action: &str, timestamp: f64) -> Result<()> {
    if let Some(state) = action.strip_prefix("manual:") {
        partition.save(surface, THREAD, state, "人工备注", &partition.row(THREAD)?)
    } else {
        phase(partition, surface, action, timestamp)
    }
}
