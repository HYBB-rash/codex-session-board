use super::engine::{HostRpc, Partition};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

#[derive(Default, Deserialize, Serialize)]
struct Progress {
    cursor: Option<String>,
    seen: BTreeSet<String>,
}

pub fn host_plan(
    known: &[String],
    cached: &[Value],
    catalog: std::result::Result<Vec<Value>, String>,
    removed: &[String],
) -> (Vec<String>, Option<String>) {
    let (projects, error) = match catalog {
        Ok(projects) => (projects, None),
        Err(error) => (vec![], Some(error)),
    };
    let mut hosts: BTreeSet<_> = known.iter().cloned().collect();
    hosts.extend(
        cached
            .iter()
            .chain(&projects)
            .filter_map(|record| record["hostId"].as_str().map(str::to_owned)),
    );
    hosts.retain(|host| !host.is_empty() && host != "local" && !removed.contains(host));
    (hosts.into_iter().collect(), error)
}

pub fn scan_page(partition: &Partition, reader: &mut dyn HostRpc) -> Result<bool> {
    ensure!(reader.host() == partition.host, "Wrong connection host");
    let generation = partition.generation.load(Ordering::SeqCst);
    let mut progress = load(partition)?;
    let mut arguments = json!({
        "mode":"history","source":"codex","hostId":partition.host,
        "archived":false,"limit":50
    });
    if let Some(cursor) = &progress.cursor {
        arguments["cursor"] = json!(cursor);
    }
    let response = reader.call("list_threads", arguments)?;
    ensure!(
        generation == partition.generation.load(Ordering::SeqCst),
        "Old discovery generation"
    );
    let threads = response["threads"]
        .as_array()
        .context("Missing remote page")?;
    let next = response.get("nextCursor").cloned().unwrap_or(Value::Null);
    ensure!(next.is_null() || next.is_string(), "Invalid cursor");
    progress.cursor = next
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned);
    if let Some(cursor) = &progress.cursor {
        ensure!(progress.seen.insert(cursor.clone()), "Repeated page cursor");
    } else {
        progress.seen.clear();
    }
    partition.discover(&partition.host, threads)?;
    partition.store.connection()?.execute(
        "INSERT INTO settings(key,value) VALUES ('lab_scan_progress',?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [serde_json::to_string(&progress)?],
    )?;
    Ok(progress.cursor.is_none())
}

fn load(partition: &Partition) -> Result<Progress> {
    let value: Option<String> = partition
        .store
        .connection()?
        .query_row(
            "SELECT value FROM settings WHERE key='lab_scan_progress'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(Into::into)
}

pub fn batch_snapshot(rows: &[Value], project: Option<(&str, &str)>) -> Vec<Value> {
    rows.iter()
        .filter(|row| {
            row["state"] == "to_archive"
                && row["canArchive"] == true
                && project.is_none_or(|(host, project)| {
                    row["hostId"] == host && row["projectId"] == project
                })
        })
        .cloned()
        .collect()
}
