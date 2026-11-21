use serde_json::Value;
use std::collections::BTreeSet;

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
