use super::super::{Snapshot, capture::Capture, input};
use crate::{app::App, model::now, rpc::Rpc};
use anyhow::{Context, Result};
use serde_json::Value;

pub(super) struct Check {
    progress: Option<Value>,
    snapshot: Option<Snapshot>,
    last_check: f64,
    recovery: bool,
}

impl Check {
    pub(super) fn due(app: &App, requested: bool) -> Result<Option<Self>> {
        let mut state = app
            .catalog
            .lock()
            .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
        if !requested
            && (state.progress.is_none() && now() - state.last_check < 30.0
                || state.error.is_some() && now() - state.last_attempt < 30.0)
        {
            return Ok(None);
        }
        state.last_attempt = now();
        if requested && state.error.is_some() {
            state.progress = None;
        }
        Ok(Some(Self {
            progress: state.progress.clone(),
            snapshot: state.snapshot.clone(),
            last_check: state.last_check,
            recovery: !state.live || state.error.is_some(),
        }))
    }

    pub(super) fn prepare(
        self,
        app: &App,
        reader: &mut dyn Rpc,
        source: Value,
        requested: bool,
    ) -> Result<Option<Capture>> {
        let same_source = self.snapshot.as_ref().is_some_and(|snapshot| {
            ["hostId", "codexHome", "peerFingerprint", "version"]
                .iter()
                .all(|field| snapshot.source[*field] == source[*field])
        });
        if !same_source {
            let _operation = app.operation()?;
            app.catalog
                .lock()
                .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?
                .live = false;
        }
        if let Some(progress) = &self.progress {
            let saved: Capture =
                serde_json::from_value(progress.clone()).context("目录续读缓存无效")?;
            if saved.source == source {
                return Ok(Some(saved));
            }
        }
        if !requested && !self.recovery && same_source && self.unchanged(reader, &source)? {
            let _operation = app.operation()?;
            app.catalog
                .lock()
                .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?
                .last_check = now();
            return Ok(None);
        }
        Ok(Some(Capture::begin(reader, source, app.config.page_size)?))
    }

    fn unchanged(&self, reader: &mut dyn Rpc, source: &Value) -> Result<bool> {
        let Some(snapshot) = &self.snapshot else {
            return Ok(false);
        };
        if now() - snapshot.captured_at >= 300.0 || now() - self.last_check < 30.0 {
            return Ok(false);
        }
        let path = format!(
            "{}/.codex-global-state.json",
            source["codexHome"].as_str().context("缺少 Codex Home")?
        );
        Ok(snapshot.source["stateMetadata"] == input::metadata(reader, &path)?)
    }
}
