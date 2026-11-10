use super::{Config, config_id, default_paths, resolve_path, write_json};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn migrate_sections(source: &Path, target: &Path) -> Result<Value> {
    let source = resolve_path(source)?;
    let target = resolve_path(target)?;
    ensure!(source != target, "迁移必须写入新配置，不能覆盖源配置");
    let bytes = std::fs::read(&source)?;
    let mut value: Value = serde_json::from_slice(&bytes)?;
    let removed = retire(&mut value)?;
    let data = value.get("data_dir").and_then(Value::as_str);
    let data = data.map(std::path::PathBuf::from).unwrap_or(default_paths()?.1);
    let data = resolve_path(&source.parent().context("配置缺少父目录")?.join(data))?;
    value["data_dir"] = json!(data);
    let config: Config = serde_json::from_value(value.clone())?;
    config.validate()?;
    write_json(&target, &value, false)?;
    Ok(json!({
        "source":source,"target":target,"removedStates":removed,
        "sourceSha256":format!("{:x}", Sha256::digest(&bytes)),
        "configId":config_id(&config)?,"dataDir":data
    }))
}

fn retire(value: &mut Value) -> Result<Vec<&'static str>> {
    ensure!(value["native_sections"] == true && value["legacy_mode"] != true,
        "仅迁移日常原生分区配置，不改变六状态兼容模式");
    let sections = value["status_sections"].as_object_mut().context("状态分区配置无效")?;
    Ok(if sections.remove("completed").is_some() {
        vec!["completed"]
    } else {
        vec![]
    })
}
