use super::{Snapshot, capture::Capture};
use crate::{app::App, model::now, rpc::Role};
use anyhow::{Context, Result};
use serde_json::json;
use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

#[cfg(test)]
#[path = "continuity_tests.rs"]
mod continuity_tests;
mod refresh;
#[cfg(test)]
mod tests;

pub(crate) fn start(app: &Arc<App>) -> Result<()> {
    if app.config.host_id == "local" {
        return Ok(());
    }
    let worker = Arc::clone(app);
    app.workers.spawn(move || run(worker))
}

fn run(app: Arc<App>) -> Result<()> {
    while app.enabled.load(Ordering::SeqCst) {
        if let Err(error) = cycle(&app) {
            failed(&app, error)?;
        }
        crate::workers::pause(&app.enabled, Duration::from_secs(2), None);
    }
    Ok(())
}

fn failed(app: &App, error: anyhow::Error) -> Result<()> {
    let Some(_operation) = app.worker_operation()? else {
        return Ok(());
    };
    let mut state = app
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
    let message = format!("{error:#}");
    state.live = false;
    state.error = Some(message.clone());
    if message.contains("读取期间")
        || message.contains("游标")
        || message.contains("续读缓存无效")
        || message.to_ascii_lowercase().contains("cursor")
    {
        state.progress = None;
        state.restarts += 1;
        app.store.save_catalog_value("catalog_progress_v1", None)?;
        if state.restarts < 3 {
            state.last_attempt = 0.0;
            state.last_check = 0.0;
        }
    }
    Ok(())
}

pub(crate) fn cycle(app: &App) -> Result<()> {
    let requested = app.catalog_refresh.swap(false, Ordering::SeqCst);
    let Some(check) = refresh::Check::due(app, requested)? else {
        return Ok(());
    };
    let mut reader = app.connection(Duration::from_secs(20), Role::CatalogReader)?;
    let mut source = reader.catalog_source()?;
    source["hostId"] = json!(app.config.host_id);
    let Some(mut capture) = check.prepare(app, &mut reader, source, requested)? else {
        return Ok(());
    };
    let result = capture.advance(&mut reader, app.config.max_pages);
    let _operation = app.operation()?;
    if let Ok(Some(snapshot)) = result {
        return publish(app, snapshot);
    }
    checkpoint(app, &capture)?;
    result.context("目录扫描尚未完成")?;
    app.catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?
        .error = None;
    Ok(())
}

fn checkpoint(app: &App, capture: &Capture) -> Result<()> {
    let progress = serde_json::to_value(capture)?;
    app.store
        .save_catalog_value("catalog_progress_v1", Some(&progress))?;
    let mut state = app
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
    state.progress = Some(progress);
    state.last_check = now();
    Ok(())
}

fn publish(app: &App, mut snapshot: Snapshot) -> Result<()> {
    {
        let state = app
            .catalog
            .lock()
            .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
        if let Some(previous) = &state.snapshot {
            super::continuity::retain(&mut snapshot, previous);
        }
    }
    snapshot.validate(&app.config.host_id)?;
    let ordinary: Vec<_> = snapshot
        .active
        .iter()
        .filter(|row| row["ordinary"] == true)
        .cloned()
        .collect();
    app.store.discover(&ordinary)?;
    let published: Vec<_> = snapshot
        .active
        .iter()
        .chain(&snapshot.archived)
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect();
    app.store.catalog_mark_published(&published)?;
    app.store.save_catalog_value(
        "catalog_snapshot_v1",
        Some(&serde_json::to_value(&snapshot)?),
    )?;
    app.store.save_catalog_value("catalog_progress_v1", None)?;
    let mut state = app
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
    state.snapshot = Some(snapshot);
    state.progress = None;
    state.live = true;
    state.error = None;
    state.last_check = now();
    state.restarts = 0;
    Ok(())
}
