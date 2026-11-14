use super::{Snapshot, capture::Capture, input};
use crate::{app::App, model::now, rpc::Role};
use anyhow::{Context, Result};
use serde_json::json;
use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

pub(crate) fn start(app: &Arc<App>) -> Result<()> {
    if app.config.host_id == "local" {
        return Ok(());
    }
    let worker = Arc::clone(app);
    app.workers.spawn(move || run(worker))
}

fn run(app: Arc<App>) -> Result<()> {
    while app.enabled.load(Ordering::SeqCst) {
        if let Err(error) = cycle(&app)
            && let Some(_operation) = app.worker_operation()?
        {
            let mut state = app
                .catalog
                .lock()
                .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
            state.live = false;
            state.error = Some(error.to_string());
            if error.to_string().contains("读取期间") {
                state.progress = None;
                state.restarts += 1;
                app.store.save_catalog_value("catalog_progress_v1", None)?;
                if state.restarts < 3 {
                    state.last_attempt = 0.0;
                    state.last_check = 0.0;
                }
            }
        }
        crate::workers::pause(&app.enabled, Duration::from_secs(2), None);
    }
    Ok(())
}

pub(crate) fn cycle(app: &App) -> Result<()> {
    let requested = app.catalog_refresh.swap(false, Ordering::SeqCst);
    let (progress, snapshot, last_check, recovery) = {
        let mut state = app
            .catalog
            .lock()
            .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
        if !requested && state.progress.is_none() && now() - state.last_check < 30.0 {
            return Ok(());
        }
        if !requested && state.error.is_some() && now() - state.last_attempt < 30.0 {
            return Ok(());
        }
        state.last_attempt = now();
        (
            state.progress.clone(),
            state.snapshot.clone(),
            state.last_check,
            !state.live || state.error.is_some(),
        )
    };
    let mut reader = app.connection(Duration::from_secs(20), Role::CatalogReader)?;
    let mut source = reader.catalog_source()?;
    source["hostId"] = json!(app.config.host_id);
    let mut capture = match progress {
        Some(progress) => {
            let saved: Capture = serde_json::from_value(progress).context("目录续读缓存无效")?;
            (saved.source == source).then_some(saved)
        }
        None => None,
    };
    if capture.is_none() {
        let same_source = snapshot.as_ref().is_some_and(|snapshot| {
            ["hostId", "codexHome", "peerFingerprint", "version"]
                .iter()
                .all(|field| snapshot.source[*field] == source[*field])
        });
        if !requested
            && !recovery
            && same_source
            && snapshot
                .as_ref()
                .is_some_and(|snapshot| now() - snapshot.captured_at < 300.0)
            && now() - last_check >= 30.0
        {
            let path = format!(
                "{}/.codex-global-state.json",
                source["codexHome"].as_str().context("缺少 Codex Home")?
            );
            let metadata = input::metadata(&mut reader, &path)?;
            if snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.source["stateMetadata"] == metadata)
            {
                let mut state = app
                    .catalog
                    .lock()
                    .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
                state.last_check = now();
                return Ok(());
            }
        }
        capture = Some(Capture::begin(&mut reader, source, app.config.page_size)?);
    }
    let mut capture = capture.context("未创建目录扫描")?;
    let result = capture.advance(&mut reader, app.config.max_pages);
    let _operation = app.operation()?;
    let progress = serde_json::to_value(&capture)?;
    match result {
        Ok(Some(snapshot)) => publish(app, snapshot),
        result => {
            app.store
                .save_catalog_value("catalog_progress_v1", Some(&progress))?;
            let mut state = app
                .catalog
                .lock()
                .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
            state.progress = Some(progress);
            state.last_check = now();
            match result {
                Ok(None) => {
                    state.error = None;
                    Ok(())
                }
                Err(error) => Err(error),
                Ok(Some(_)) => unreachable!(),
            }
        }
    }
}

fn publish(app: &App, snapshot: Snapshot) -> Result<()> {
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
