use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub codex_command: Vec<String>,
    pub data_dir: PathBuf,
    pub port: u16,
    pub poll_seconds: u64,
    pub rpc_timeout_seconds: u64,
    pub page_size: usize,
    pub max_pages: usize,
    pub history_per_poll: usize,
    pub host_id: String,
    pub open_command: Vec<String>,
    pub desktop_socket: PathBuf,
    pub runtime_poll_seconds: u64,
    pub sync_titles: bool,
    pub sync_sections: bool,
    pub native_sections: bool,
    pub sidebar_socket: PathBuf,
    pub sidebar_caller: String,
    pub sidebar_state_file: PathBuf,
    pub status_sections: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            codex_command: vec!["codex".into(), "app-server".into(), "--stdio".into()],
            data_dir: PathBuf::new(),
            port: 8767,
            poll_seconds: 30,
            rpc_timeout_seconds: 20,
            page_size: 100,
            max_pages: 100,
            history_per_poll: 20,
            host_id: "local".into(),
            open_command: vec![
                if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "xdg-open"
                }
                .into(),
            ],
            desktop_socket: env::var_os("CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET")
                .map(PathBuf::from)
                .unwrap_or_default(),
            runtime_poll_seconds: 2,
            sync_titles: true,
            sync_sections: false,
            native_sections: false,
            sidebar_socket: PathBuf::new(),
            sidebar_caller: String::new(),
            sidebar_state_file: PathBuf::new(),
            status_sections: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        for (name, command) in [
            ("codex_command", &self.codex_command),
            ("open_command", &self.open_command),
        ] {
            ensure!(
                !command.is_empty() && command.iter().all(|part| !part.is_empty()),
                "{name} 必须是非空命令参数数组"
            );
        }
        ensure!(!self.host_id.is_empty(), "host_id 必须为非空文本");
        ensure!(!self.data_dir.as_os_str().is_empty(), "data_dir 不得为空");
        if self.sync_sections || self.native_sections {
            self.validate_sections()?;
        }
        if self.native_sections {
            ensure!(!self.sync_sections, "原生状态来源不允许分区同步");
        }
        for (name, value) in [
            ("poll_seconds", self.poll_seconds as u128),
            ("rpc_timeout_seconds", self.rpc_timeout_seconds as u128),
            ("page_size", self.page_size as u128),
            ("max_pages", self.max_pages as u128),
            ("history_per_poll", self.history_per_poll as u128),
            ("runtime_poll_seconds", self.runtime_poll_seconds as u128),
        ] {
            ensure!(value > 0, "{name} 必须为正整数");
        }
        Ok(())
    }

    fn validate_sections(&self) -> Result<()> {
        ensure!(
            self.sidebar_state_file.is_absolute(),
            "sidebar_state_file 必须为绝对路径"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.sidebar_caller).is_ok(),
            "sidebar_caller 必须为会话 ID"
        );
        ensure!(
            self.sidebar_socket.is_absolute(),
            "sidebar_socket 必须为绝对路径"
        );
        ensure!(
            self.status_sections
                .keys()
                .all(|state| crate::model::STATES.iter().any(|item| item.0 == state)),
            "包含未知状态分区"
        );
        let mut identifiers = HashSet::new();
        for (state, _) in crate::model::board_states() {
            let identifier = self.status_sections.get(state).context("缺少状态分区")?;
            ensure!(
                uuid::Uuid::parse_str(identifier).is_ok(),
                "状态分区 ID 无效"
            );
            ensure!(identifiers.insert(identifier), "状态分区不得重复");
        }
        Ok(())
    }
}

fn environment_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn xdg_home(name: &str, fallback: &Path) -> Result<PathBuf> {
    if let Some(path) = environment_path(name) {
        ensure!(path.is_absolute(), "{name} 必须为绝对路径");
        return Ok(path);
    }
    let home = environment_path("HOME").context("HOME 未设置，无法确定独立配置和数据目录")?;
    ensure!(home.is_absolute(), "HOME 必须为绝对路径");
    Ok(home.join(fallback))
}

pub fn default_paths() -> Result<(PathBuf, PathBuf)> {
    let config = match environment_path("CODEX_SESSION_BOARD_CONFIG") {
        Some(path) => path,
        None => xdg_home("XDG_CONFIG_HOME", Path::new(".config"))?
            .join("codex-session-board/config.json"),
    };
    let data = xdg_home("XDG_DATA_HOME", Path::new(".local/share"))?.join("codex-session-board");
    Ok((resolve_path(&config)?, resolve_path(&data)?))
}

pub(crate) fn resolve_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            component => {
                resolved.push(component.as_os_str());
                match fs::symlink_metadata(&resolved) {
                    Ok(_) => resolved = fs::canonicalize(&resolved)?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(resolved)
}

pub fn load(path: Option<&Path>) -> Result<Config> {
    let (default_config, default_data) = default_paths()?;
    let config_path = resolve_path(path.unwrap_or(&default_config))?;
    let mut value = match fs::read(&config_path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .with_context(|| format!("读取配置 {}", config_path.display()))?,
        Err(error) if path.is_none() && error.kind() == std::io::ErrorKind::NotFound => {
            serde_json::json!({})
        }
        Err(error) => return Err(error).context("无法读取配置文件"),
    };
    let object = value.as_object_mut().context("配置必须为 JSON 对象")?;
    object
        .entry("data_dir")
        .or_insert(serde_json::to_value(default_data)?);
    let mut config: Config = serde_json::from_value(value).context("配置字段格式无效")?;
    config.validate()?;
    let base = config_path.parent().context("配置文件缺少父目录")?;
    config.data_dir = resolve_path(&base.join(&config.data_dir))?;
    Ok(config)
}

pub fn initialize_config(path: &Path, data: &Path) -> Result<()> {
    if path.try_exists()? {
        ensure!(path.is_file(), "配置路径不是普通文件");
        return Ok(());
    }
    let config = Config {
        data_dir: resolve_path(data)?,
        ..Config::default()
    };
    config.validate()?;
    create_private_dir(&config.data_dir)?;
    match write_json(path, &serde_json::to_value(config)?, false) {
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
        {
            Ok(())
        }
        result => result,
    }
}

pub(crate) fn create_private_dir(path: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    ensure!(path.is_dir(), "数据路径不是目录");
    Ok(())
}

pub(crate) fn write_json(path: &Path, value: &Value, replace: bool) -> Result<()> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    ensure!(
        !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()),
        "不会通过符号链接发布配置或服务记录"
    );
    let path = resolve_path(path)?;
    let parent = path.parent().context("JSON 文件缺少父目录")?;
    create_private_dir(parent)?;
    let temporary = parent.join(format!(
        ".session-board-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        if replace {
            fs::rename(&temporary, &path)?;
        } else {
            fs::hard_link(&temporary, &path)?;
        }
        Ok(())
    })();
    drop(file);
    let _ = fs::remove_file(&temporary);
    result
}

fn sorted_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let entries: std::collections::BTreeMap<_, _> = object.into_iter().collect();
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, sorted_value(value)))
                    .collect(),
            )
        }
        Value::Array(array) => Value::Array(array.into_iter().map(sorted_value).collect()),
        value => value,
    }
}

pub fn config_id(config: &Config) -> Result<String> {
    let bytes = serde_json::to_vec(&sorted_value(serde_json::to_value(config)?))?;
    Ok(format!("{:x}", Sha256::digest(bytes))[..16].into())
}

pub fn build_id() -> Result<String> {
    static BUILD: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(build) = BUILD.get() {
        return Ok(build.clone());
    }
    let mut executable = fs::File::open("/proc/self/exe")?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let length = executable.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    let build = format!("{:x}", digest.finalize());
    let _ = BUILD.set(build.clone());
    Ok(build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Result<Self> {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let directory =
                env::temp_dir().join(format!("board-config-{}-{stamp}", std::process::id()));
            create_private_dir(&directory)?;
            Ok(Self(directory))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn relative_data_uses_config_directory_and_preserves_order_independence() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.0.join("profile/config.json");
        write_json(
            &path,
            &serde_json::json!({"data_dir":"../data","host_id":"主机甲"}),
            false,
        )?;
        let config = load(Some(&path))?;
        assert_eq!(config.data_dir, fixture.0.join("data"));
        assert_eq!(config.host_id, "主机甲");
        assert_eq!(config.port, 8767);
        let mut reordered = sorted_value(serde_json::to_value(&config)?);
        reordered["host_id"] = serde_json::json!("主机甲");
        let decoded: Config = serde_json::from_value(reordered)?;
        assert_eq!(config_id(&config)?, config_id(&decoded)?);
        let changed = Config {
            port: 8768,
            ..decoded
        };
        assert_ne!(config_id(&config)?, config_id(&changed)?);
        Ok(())
    }

    #[test]
    fn initialization_never_overwrites_and_uses_private_permissions() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.0.join("profile/config.json");
        initialize_config(&path, &fixture.0.join("data"))?;
        let original = fs::read(&path)?;
        initialize_config(&path, &fixture.0.join("different"))?;
        assert_eq!(fs::read(&path)?, original);
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        assert!(write_json(&path, &serde_json::json!({}), false).is_err());
        assert_eq!(fs::read(&path)?, original);
        Ok(())
    }

    #[test]
    fn json_publication_does_not_follow_a_replacement_symlink() -> Result<()> {
        let fixture = Fixture::new()?;
        let original = fixture.0.join("original.json");
        let alias = fixture.0.join("alias.json");
        fs::write(&original, b"retained")?;
        std::os::unix::fs::symlink(&original, &alias)?;
        assert!(write_json(&alias, &serde_json::json!({}), true).is_err());
        assert_eq!(fs::read(&original)?, b"retained");
        Ok(())
    }

    #[test]
    fn invalid_types_and_zero_quotas_are_rejected_before_creating_data() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.0.join("config.json");
        for invalid in [
            serde_json::json!({"poll_seconds":0}),
            serde_json::json!({"runtime_poll_seconds":false}),
            serde_json::json!({"rpc_timeout_seconds":-1}),
            serde_json::json!({"port":65536}),
            serde_json::json!({"sync_titles":"true"}),
            serde_json::json!({"codex_command":[]}),
            serde_json::json!({"open_command":[""]}),
            serde_json::json!({"host_id":""}),
            serde_json::json!({"data_dir":""}),
            serde_json::json!([]),
        ] {
            write_json(&path, &invalid, true)?;
            assert!(load(Some(&path)).is_err(), "{invalid}");
        }
        assert!(!fixture.0.join("data").exists());
        Ok(())
    }

    #[test]
    fn section_sync_requires_explicit_unique_complete_destinations() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut config = Config {
            data_dir: fixture.0.clone(),
            ..Config::default()
        };
        config.validate()?;
        config.sync_sections = true;
        assert!(config.validate().is_err());
        config.sidebar_socket = fixture.0.join("desktop.sock");
        config.sidebar_caller = uuid::Uuid::new_v4().to_string();
        config.sidebar_state_file = fixture.0.join("state.json");
        config.status_sections = crate::model::STATES
            .iter()
            .map(|(state, _)| (state.to_string(), uuid::Uuid::new_v4().to_string()))
            .collect();
        config.validate()?;
        let duplicate = config.status_sections["unknown"].clone();
        config.status_sections.remove("completed");
        config.validate()?;
        config.status_sections.insert("review".into(), duplicate);
        assert!(config.validate().is_err());
        config
            .status_sections
            .insert("review".into(), "pinned".into());
        assert!(config.validate().is_err());
        config.status_sections.remove("review");
        assert!(config.validate().is_err());
        Ok(())
    }

    #[test]
    fn symlink_data_aliases_resolve_to_the_same_identity() -> Result<()> {
        let fixture = Fixture::new()?;
        create_private_dir(&fixture.0.join("actual"))?;
        std::os::unix::fs::symlink(fixture.0.join("actual"), fixture.0.join("alias"))?;
        let path = fixture.0.join("config.json");
        write_json(
            &path,
            &serde_json::json!({"data_dir":"alias/../actual"}),
            false,
        )?;
        let config = load(Some(&path))?;
        assert_eq!(config.data_dir, fixture.0.join("actual"));
        assert_eq!(
            config.desktop_socket.as_os_str().is_empty(),
            Config::default().desktop_socket.as_os_str().is_empty()
        );
        Ok(())
    }
}
