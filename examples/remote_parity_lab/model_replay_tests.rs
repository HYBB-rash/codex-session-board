use super::{
    engine::{Partition, RuntimeEvidence},
    surface::{Surface, THREAD, fixture},
};
use anyhow::Result;
use codex_session_board::model::{board_state, now};
use serde_json::{Value, json};
use std::process::Command;
use std::sync::atomic::Ordering;

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
    let mut compared = 0;
    for trace in traces {
        let (root, mut partition, mut surface) = fixture("mac")?;
        let timestamp = now() + 100.0;
        phase(&partition, &mut surface, "running", timestamp)?;
        let base = partition.row(THREAD)?["version"].as_i64().unwrap();
        for step in trace.as_array().unwrap() {
            let action = step["event"].as_str().unwrap();
            if let Some(state) = action.strip_prefix("manual:") {
                partition.save(
                    &mut surface,
                    THREAD,
                    state,
                    "人工备注",
                    &partition.row(THREAD)?,
                )?;
            } else if action == "restart" {
                partition = Partition::open(root.path(), "mac")?;
            } else if action != "view" {
                phase(&partition, &mut surface, action, timestamp)?;
            }
            assert_model(&partition, &step["expected"], base)?;
            compared += 1;
        }
    }
    assert!(compared >= 520);
    eprintln!("remote adapter: 40 states, 520 edges, {compared} compared steps");
    Ok(())
}
