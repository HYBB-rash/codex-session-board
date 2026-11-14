use crate::app::App;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
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
    let mut view = View {
        rows: Vec::new(),
        projects: Vec::new(),
        health,
    };
    if let Some(snapshot) = &snapshot {
        view.projects = snapshot.projects.clone();
        for metadata in snapshot.active.iter().chain(&snapshot.archived) {
            let identifier = metadata["id"].as_str().unwrap_or_default();
            let row = saved
                .remove(identifier)
                .unwrap_or_else(|| placeholder(metadata));
            view.rows
                .push(snapshot_row(row, metadata, live, snapshot.captured_at));
        }
    }
    let at = snapshot
        .as_ref()
        .map_or(0.0, |snapshot| snapshot.captured_at);
    append_cache(app, saved, drafts, at, &mut view.rows)?;
    Ok(view)
}

fn placeholder(metadata: &Value) -> Value {
    json!({
        "id":metadata["id"],"title":"空会话","cwd":metadata["cwd"],
        "updatedAt":metadata["updatedAt"],"state":"unknown","note":"","manual":false,
        "followUp":false,"version":0,"inputVersion":0,"sectionVersion":null,
        "reason":"仅目录元数据，未读取执行历史。"
    })
}

fn snapshot_row(mut row: Value, metadata: &Value, live: bool, at: f64) -> Value {
    if let Some(title) = ["name", "preview"]
        .into_iter()
        .filter_map(|key| metadata[key].as_str())
        .find(|title| !title.is_empty())
    {
        row["title"] = json!(title);
    }
    for key in ["cwd", "updatedAt"] {
        if let Some(value) = metadata.get(key) {
            row[key] = value.clone();
        }
    }
    let archived = metadata["archived"] == true || row["archived"] == true;
    row["archived"] = json!(archived);
    row["ordinary"] = metadata["ordinary"].clone();
    row["catalogCurrent"] = json!(true);
    row["catalogProjectId"] = metadata["catalogProjectId"].clone();
    row["membership"] = metadata["membership"].clone();
    row["projectIssue"] = metadata["membership"]["projectIssue"].clone();
    row["presence"] = json!(if !live {
        "cachedUnknown"
    } else if archived {
        "archived"
    } else {
        "active"
    });
    row["catalogEvidence"] = json!("snapshot");
    row["catalogAt"] = json!(at);
    row["readOnly"] = json!(!live || archived || metadata["ordinary"] != true);
    if row["readOnly"] == true {
        row["canArchive"] = json!(false);
    }
    row
}

fn append_cache(
    app: &App,
    saved: BTreeMap<String, Value>,
    drafts: &[Value],
    at: f64,
    rows: &mut Vec<Value>,
) -> Result<()> {
    let runtime = active_runtime(app)?;
    let published = app.store.catalog_published_ids()?;
    let drafts: BTreeMap<_, _> = drafts
        .iter()
        .filter_map(|draft| Some((draft["threadId"].as_str()?, &draft["project"])))
        .collect();
    for (identifier, row) in saved {
        let pending = drafts
            .get(identifier.as_str())
            .filter(|_| !published.contains(&identifier));
        let fresh = runtime.contains(&identifier)
            && row["seenAt"].as_f64().is_some_and(|seen| seen > at)
            && app.enabled.load(Ordering::SeqCst);
        rows.push(cached_row(row, pending.copied(), fresh));
    }
    Ok(())
}

fn active_runtime(app: &App) -> Result<HashSet<String>> {
    let live = app
        .health
        .lock()
        .map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?["runtimeVerified"]
        == true;
    if !live {
        return Ok(HashSet::new());
    }
    Ok(app
        .active_sessions
        .lock()
        .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?
        .clone()
        .unwrap_or_default())
}

fn cached_row(mut row: Value, pending: Option<&Value>, fresh: bool) -> Value {
    let current = (pending.is_some() || fresh) && row["archived"] != true;
    row["catalogCurrent"] = json!(current);
    row["ordinary"] = json!(fresh);
    row["presence"] = json!(if current { "active" } else { "cachedUnknown" });
    row["catalogEvidence"] = json!(if pending.is_some() {
        "pendingDraft"
    } else if fresh {
        "runtime"
    } else {
        "cache"
    });
    row["readOnly"] = json!(!current);
    row["catalogProjectId"] = pending.cloned().unwrap_or(Value::Null);
    row["projectIssue"] = if pending.is_some() {
        Value::Null
    } else {
        json!("尚未核对这条记录的项目归属")
    };
    if !current {
        row["canArchive"] = json!(false);
    }
    row
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
