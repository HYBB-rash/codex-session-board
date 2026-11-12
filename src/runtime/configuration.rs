use super::*;
use crate::config;
use fs2::FileExt;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;

pub(super) fn load(entry: &Path) -> Result<Config> {
    if entry != config::entry_path()? || std::env::var_os("CODEX_SESSION_BOARD_CONFIG").is_some() {
        return Ok(service::deployment(entry)?.1);
    }
    let parent = entry.parent().context("配置入口缺少父目录")?;
    config::create_private_dir(parent)?;
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .mode(0o600).open(parent.join(".runtime-config.lock"))?;
    lock.lock_exclusive()?;
    let marker = parent.join("runtime-config-migration.json");
    if marker.try_exists()? {
        let migration: Value = serde_json::from_slice(&fs::read(&marker)?)?;
        ensure!(migration["complete"] == true, "正式配置迁移记录无效");
        return config::load(Some(entry));
    }
    let (_, data) = config::default_paths()?;
    let receipt = data.join("installation/current.json");
    let source = legacy_source(&receipt)?;
    if let Some(source) = &source {
        import(entry, source)?;
    }
    let settings = service::deployment(entry)?.1;
    config::write_json(&marker, &json!({"complete":true,"source":source}), false)?;
    Ok(settings)
}

fn legacy_source(receipt: &Path) -> Result<Option<PathBuf>> {
    if !receipt.try_exists()? {
        return Ok(None);
    }
    let installation: Value = serde_json::from_slice(&fs::read(receipt)?)?;
    let legacy = installation.get("legacy_installation").unwrap_or(&installation);
    let Some(path) = legacy.get("config") else {
        return Ok(None);
    };
    let source = PathBuf::from(path.as_str().context("旧部署配置路径无效")?);
    ensure!(source.is_absolute(), "旧部署配置路径必须为绝对路径");
    Ok(Some(source))
}

fn import(entry: &Path, source: &Path) -> Result<()> {
    let settings = config::load(Some(source)).context("旧正式配置不可读，不会切换至默认数据库")?;
    if config::resolve_path(entry)? == config::resolve_path(source)? {
        return Ok(());
    }
    let parent = entry.parent().context("配置入口缺少父目录")?;
    let identifier = uuid::Uuid::new_v4();
    if entry.try_exists()? {
        fs::copy(entry, parent.join(format!("config-before-runtime-{identifier}.json")))?;
    }
    let temporary = parent.join(format!(".runtime-config-{identifier}.json"));
    config::write_json(&temporary, &serde_json::to_value(settings)?, false)?;
    fs::rename(temporary, entry)?;
    Ok(())
}
