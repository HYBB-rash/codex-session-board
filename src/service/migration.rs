use crate::config::{self, Config};
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;

mod database;
mod paths;
mod plan;
mod transaction;

#[cfg(test)]
pub(super) use database::import_database;
pub(super) use paths::{configuration, load_source, lock};
use plan::Plan;
use transaction::{Created, Stage};

pub(super) fn publish(config_path: &Path, config: &Config, source: &Path) -> Result<bool> {
    let config_path = configuration(config_path)?;
    publish_with(&config_path, config, source, |value| {
        config::write_json(&config_path, value, false)
    })
}

fn publish_with(
    config_path: &Path,
    config: &Config,
    source: &Path,
    publish_config: impl FnOnce(&Value) -> Result<()>,
) -> Result<bool> {
    let config_path = configuration(config_path)?;
    let config = Config {
        data_dir: config::resolve_path(&config.data_dir)?,
        ..config.clone()
    };
    let source = paths::resolve_parent(source)?;
    let plan = Plan::new(&source)?;
    plan.validate(&config_path, &config.data_dir)?;
    let mut created = Created::default();
    let result = (|| -> Result<bool> {
        created.directory(&config.data_dir)?;
        let stage = Stage::new(&config.data_dir)?;
        plan.prepare(&stage.0)?;
        plan.install(&stage.0, &config.data_dir, &mut created)?;
        fs::remove_dir_all(&stage.0)?;
        created.directory(config_path.parent().context("目标配置缺少父目录")?)?;
        publish_config(&serde_json::to_value(&config)?)?;
        Ok(plan.main_database)
    })();
    if let Err(error) = result {
        return match created.rollback() {
            Ok(()) => Err(error),
            Err(cleanup) => Err(error.context(cleanup)),
        };
    }
    result
}
