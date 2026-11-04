use anyhow::{Context, Result, bail, ensure};
use chrono::{FixedOffset, SecondsFormat, Utc};
use fs2::FileExt;
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn now() -> String {
    let now = Utc::now();
    FixedOffset::east_opt(8 * 3600)
        .map(|offset| {
            now.with_timezone(&offset)
                .to_rfc3339_opts(SecondsFormat::Secs, false)
        })
        .unwrap_or_else(|| now.to_rfc3339_opts(SecondsFormat::Secs, false))
}

pub fn canonical_id(value: &str) -> Result<String> {
    Ok(Uuid::parse_str(value)
        .context("Expected a complete UUID")?
        .to_string())
}

pub fn page_id(value: &str) -> Result<String> {
    let pattern =
        Regex::new(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}|[0-9a-fA-F]{32}")?;
    canonical_id(pattern.find(value).context("No page identifier")?.as_str())
}

pub fn thread_id(value: &str) -> Option<String> {
    let matched = Regex::new(r"codex://threads/([0-9a-fA-F-]{36})")
        .ok()?
        .captures(value)?;
    canonical_id(&matched[1]).ok()
}

pub fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub fn read_json(path: &Path, default: Value) -> Result<Value> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid JSON in {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(error) => Err(error).with_context(|| format!("Read {}", path.display())),
    }
}

pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name()
            .context("Missing filename")?
            .to_string_lossy(),
        Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("Write {}", path.display()))
}

pub fn append_event(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    Ok(())
}

pub fn lock(path: &Path) -> Result<Option<File>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .append(true)
        .open(path)?;
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .with_context(|| format!("Missing or empty {key}"))
}

pub fn command(value: &Value) -> Result<Vec<String>> {
    let items = value.as_array().context("Command must be a JSON array")?;
    ensure!(!items.is_empty(), "Command cannot be empty");
    items
        .iter()
        .map(|item| {
            Ok(item
                .as_str()
                .filter(|text| !text.is_empty())
                .context("Command arguments must be nonempty strings")?
                .to_string())
        })
        .collect()
}

pub fn config_path(config: &Value, key: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(required_str(config, key)?))
}

pub fn load_config(path: &Path) -> Result<Value> {
    let absolute = fs::canonicalize(path)
        .with_context(|| format!("Configuration does not exist: {}", path.display()))?;
    let mut config = read_json(&absolute, Value::Null)?;
    ensure!(config.is_object(), "Configuration must be an object");
    let parent = absolute.parent().context("Configuration has no parent")?;
    for key in [
        "workspace",
        "data_dir",
        "task_workspace_root",
        "archive_data_dir",
        "worker_cwd",
        "daemon_socket",
    ] {
        if let Some(text) = config[key].as_str() {
            let path = Path::new(text);
            if path.is_relative() {
                config[key] = json!(parent.join(path));
            }
        }
    }
    if config.get("version").is_some() && config["version"] != 1 {
        bail!("Unsupported configuration version");
    }
    config["_config_path"] = json!(absolute);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_json_and_exclusive_lock() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("state.json");
        write_json(&path, &json!({"round": 1, "phase": "waiting_execution"}))?;
        assert_eq!(read_json(&path, Value::Null)?["round"], 1);
        let lock_path = directory.path().join("scheduler.lock");
        let first = lock(&lock_path)?;
        assert!(first.is_some());
        assert!(lock(&lock_path)?.is_none());
        drop(first);
        assert!(lock(&lock_path)?.is_some());
        Ok(())
    }

    #[test]
    fn preserves_unknown_state_fields() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("state.json");
        let state = json!({"phase": "start_prepared", "future_extension": {"keep": true}});
        write_json(&path, &state)?;
        assert_eq!(read_json(&path, Value::Null)?, state);
        Ok(())
    }

    #[test]
    fn resolves_paths_against_config() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.json");
        write_json(&path, &json!({"version": 1, "data_dir": "data"}))?;
        assert_eq!(
            load_config(&path)?["data_dir"],
            json!(directory.path().join("data"))
        );
        Ok(())
    }
}
