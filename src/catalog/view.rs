use crate::app::App;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

pub(crate) struct View {
    pub rows: Vec<Value>,
    pub projects: Vec<Value>,
    pub health: Value,
}

pub(crate) fn read(app: &App, drafts: &[Value]) -> Result<View> {
    let (snapshot, live, health) = {
        let state = app
            .catalog
            .lock()
            .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
        (state.snapshot.clone(), state.live, state.health())
    };
    let mut saved: BTreeMap<_, _> = app
        .store
        .catalog_sessions()?
        .into_iter()
        .map(|row| (row["id"].as_str().unwrap_or_default().to_owned(), row))
        .collect();
    let runtime = app
        .active_sessions
        .lock()
        .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?
        .clone()
        .unwrap_or_default();
    let published = app.store.catalog_published_ids()?;
    let drafts: BTreeMap<_, _> = drafts
        .iter()
        .filter_map(|draft| Some((draft["threadId"].as_str()?, &draft["project"])))
        .collect();
    let mut rows = Vec::new();
    let mut projects = Vec::new();
    if let Some(snapshot) = &snapshot {
        projects = snapshot.projects.clone();
        for metadata in snapshot.active.iter().chain(&snapshot.archived) {
            let identifier = metadata["id"].as_str().unwrap_or_default();
            let mut row = saved.remove(identifier).unwrap_or_else(|| {
                json!({
                    "id":identifier,"title":metadata["name"].as_str()
                        .or_else(|| metadata["preview"].as_str()).unwrap_or("空会话"),
                    "cwd":metadata["cwd"],"updatedAt":metadata["updatedAt"],
                    "state":"unknown","note":"","manual":false,"followUp":false,
                    "version":0,"inputVersion":0,"sectionVersion":null,
                    "reason":"仅目录元数据，未读取执行历史。"
                })
            });
            let archived = metadata["archived"] == true || row["archived"] == true;
            row["archived"] = json!(archived);
            row["ordinary"] = metadata["ordinary"].clone();
            row["catalogCurrent"] = json!(true);
            row["catalogProjectId"] = metadata["catalogProjectId"].clone();
            row["membership"] = metadata["membership"].clone();
            row["presence"] = json!(if !live {
                "cachedUnknown"
            } else if archived {
                "archived"
            } else {
                "active"
            });
            row["catalogEvidence"] = json!("snapshot");
            row["catalogAt"] = json!(snapshot.captured_at);
            row["readOnly"] = json!(!live || archived || metadata["ordinary"] != true);
            if row["readOnly"] == true {
                row["canArchive"] = json!(false);
            }
            rows.push(row);
        }
    }
    for (identifier, mut row) in saved {
        let pending = drafts
            .get(identifier.as_str())
            .filter(|_| !published.contains(&identifier));
        let fresh = runtime.contains(&identifier)
            && row["seenAt"].as_f64().is_some_and(|seen| {
                seen > snapshot
                    .as_ref()
                    .map_or(0.0, |snapshot| snapshot.captured_at)
            })
            && app.enabled.load(Ordering::SeqCst);
        let current = (pending.is_some() || fresh) && row["archived"] != true;
        row["catalogCurrent"] = json!(current);
        row["ordinary"] = json!(fresh || pending.is_some());
        row["presence"] = json!(if current { "active" } else { "cachedUnknown" });
        row["catalogEvidence"] = json!(if pending.is_some() {
            "pendingDraft"
        } else if fresh {
            "runtime"
        } else {
            "cache"
        });
        row["readOnly"] = json!(!current);
        if let Some(project) = pending {
            row["catalogProjectId"] = (*project).clone();
        }
        if !current {
            row["canArchive"] = json!(false);
        }
        rows.push(row);
    }
    Ok(View {
        rows,
        projects,
        health,
    })
}

pub(crate) fn known(app: &App, identifier: &str) -> Result<bool> {
    Ok(app
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.contains(identifier)))
}
