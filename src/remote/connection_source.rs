use super::Host;
use crate::{config, rpc};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub hostname: String,
    pub user: String,
    pub port: u16,
}

impl Endpoint {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let mut values = std::collections::HashMap::new();
        for line in std::str::from_utf8(bytes)?.lines() {
            let Some((key, value)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            if !["hostname", "user", "port"].contains(&key) {
                continue;
            }
            ensure!(
                values.insert(key, value.trim()).is_none(),
                "SSH 配置字段重复"
            );
        }
        let hostname = values
            .get("hostname")
            .context("SSH 配置缺少 hostname")?
            .to_string();
        let user = values.get("user").context("SSH 配置缺少 user")?.to_string();
        let port = values.get("port").context("SSH 配置缺少 port")?.parse()?;
        ensure!(
            !hostname.is_empty()
                && !hostname.starts_with('-')
                && !user.is_empty()
                && port != 0
                && !hostname
                    .chars()
                    .chain(user.chars())
                    .any(|character| character.is_control() || character.is_whitespace()),
            "SSH 有效目标无效"
        );
        Ok(Self {
            hostname,
            user,
            port,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Peer {
    pub uid: u32,
    pub home: String,
    pub codex_home: String,
    pub declared_codex_home: String,
}

impl Peer {
    pub(crate) fn validate(&self) -> Result<()> {
        for value in [&self.home, &self.codex_home, &self.declared_codex_home] {
            ensure!(
                value.starts_with('/')
                    && value.len() <= 4096
                    && !value.chars().any(char::is_control)
                    && !Path::new(value)
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir)),
                "远端身份目录无效"
            );
        }
        Ok(())
    }

    pub(crate) fn initialize(&self, response: &serde_json::Value) -> Result<()> {
        let home = response["codexHome"]
            .as_str()
            .context("初始化缺少 codexHome，来源尚未验证")?;
        ensure!(
            home == self.codex_home,
            "代理进程真实 CODEX_HOME 与 daemon codexHome 不匹配，来源尚未验证"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    host_id: String,
    ssh: Endpoint,
    peer: Option<Peer>,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub(crate) host_id: String,
    pub(crate) path: PathBuf,
    pub(crate) ssh: Vec<String>,
    pub(crate) endpoint: Endpoint,
    program: Vec<String>,
    timeout_seconds: u64,
    registry: PathBuf,
}

fn load(path: &Path) -> Result<Record> {
    ensure!(
        std::fs::symlink_metadata(path)?.is_file(),
        "源记录必须是普通文件，不能是符号链接或管道"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(16385)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16384, "源记录过长");
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        value["schemaVersion"] == 2,
        "旧源记录缺少 SSH 与 Peer 证据；保留数据，需显式确认来源"
    );
    let record: Record = serde_json::from_value(value)?;
    if let Some(peer) = &record.peer {
        peer.validate()?;
    }
    Ok(record)
}

fn publish(path: &Path, record: &Record, replace: bool) -> Result<()> {
    let value = serde_json::to_value(record)?;
    ensure!(
        serde_json::to_vec_pretty(&value)?.len() < 16384,
        "源身份记录过长"
    );
    config::write_json(path, &value, replace)
}

impl Binding {
    pub(crate) fn existing(host: &Host, settings: &config::Config) -> Result<Self> {
        let path = host.directory(settings).join("source.json");
        let record = load(&path)?;
        let binding = Self {
            host_id: host.host_id.clone(),
            path,
            ssh: host.ssh(settings)?,
            endpoint: record.ssh,
            program: settings.ssh_command.clone(),
            timeout_seconds: settings.rpc_timeout_seconds,
            registry: settings.sidebar_state_file.clone(),
        };
        binding.require_confirmed()?;
        Ok(binding)
    }

    pub(crate) fn prepare(host: &Host, settings: &config::Config) -> Result<Self> {
        let path = host.directory(settings).join("source.json");
        if path.try_exists()? {
            load(&path)?;
        }
        let ssh = host.ssh(settings)?;
        let deadline = rpc::deadline_after(Duration::from_secs(settings.rpc_timeout_seconds))?;
        let endpoint = Self::resolve(&ssh, deadline)?;
        let binding = Self {
            host_id: host.host_id.clone(),
            path,
            ssh,
            endpoint,
            program: settings.ssh_command.clone(),
            timeout_seconds: settings.rpc_timeout_seconds,
            registry: settings.sidebar_state_file.clone(),
        };
        binding.register()?;
        Ok(binding)
    }

    pub(crate) fn current(&self) -> Result<Self> {
        let mut binding = self.clone();
        if !self.registry.as_os_str().is_empty() {
            let host = super::registered_at(&self.registry)?
                .into_iter()
                .find(|entry| entry.host_id == self.host_id)
                .context("已绑定的远程主机已移除")?;
            binding.ssh = host.ssh_with(&self.program, self.timeout_seconds)?;
        }
        Ok(binding)
    }

    fn register(&self) -> Result<()> {
        if !self.path.try_exists()? {
            ensure!(
                !self
                    .path
                    .parent()
                    .context("源记录缺少目录")?
                    .join("status.sqlite3")
                    .try_exists()?,
                "已有数据缺少源记录；保留数据，需显式确认来源"
            );
            let record = Record {
                schema_version: 2,
                host_id: self.host_id.clone(),
                ssh: self.endpoint.clone(),
                peer: None,
            };
            publish(&self.path, &record, false)?;
        }
        self.record()?;
        Ok(())
    }

    fn record(&self) -> Result<Record> {
        let record = load(&self.path)?;
        ensure!(
            record.host_id == self.host_id && record.ssh == self.endpoint,
            "主机有效连接身份已改变；保留数据，拒绝写入"
        );
        Ok(record)
    }

    pub(crate) fn resolve(command: &[String], deadline: Instant) -> Result<Endpoint> {
        let mut command = command.to_vec();
        let position = command
            .iter()
            .rposition(|argument| argument == "--")
            .context("SSH 命令缺少目标边界")?;
        command.insert(position, "-G".into());
        Endpoint::parse(&rpc::capture::output(&command, deadline)?)
    }

    pub(crate) fn verify(&self, deadline: Instant) -> Result<()> {
        let current = Self::resolve(&self.ssh, deadline)?;
        ensure!(
            current == self.endpoint,
            "SSH alias 的有效目标已改变；保留数据，拒绝写入"
        );
        self.record()?;
        Ok(())
    }

    pub(crate) fn check_peer(&self, peer: &Peer) -> Result<()> {
        peer.validate()?;
        let previous = self.record()?.peer;
        ensure!(
            previous.as_ref().is_none_or(|expected| expected == peer),
            "远端 UID、HOME 或 CODEX_HOME 已改变；拒绝写入"
        );
        Ok(())
    }

    pub(crate) fn require_confirmed(&self) -> Result<()> {
        let record = self.record()?;
        ensure!(
            record.peer.is_some(),
            "远端尚未通过 initialize 来源核验，暂不允许写入"
        );
        self.unique_registration(&record)?;
        Ok(())
    }

    fn unique_registration(&self, record: &Record) -> Result<()> {
        if self.registry.as_os_str().is_empty() {
            return Ok(());
        }
        let root = self
            .path
            .ancestors()
            .nth(3)
            .context("源记录缺少数据根目录")?;
        let settings = config::Config {
            data_dir: root.into(),
            ..config::Config::default()
        };
        for host in super::registered_at(&self.registry)? {
            if host.host_id == self.host_id {
                continue;
            }
            let path = host.directory(&settings).join("source.json");
            let Ok(other) = load(&path) else {
                continue;
            };
            let same_data =
                other
                    .peer
                    .as_ref()
                    .zip(record.peer.as_ref())
                    .is_some_and(|(first, second)| {
                        first.uid == second.uid && first.codex_home == second.codex_home
                    });
            ensure!(
                other.ssh.hostname != record.ssh.hostname
                    || other.ssh.port != record.ssh.port
                    || !same_data,
                "多个主机登记使用同一可观测 SSH 用户和 Codex 数据目录；请保留一个登记后再修改会话"
            );
        }
        Ok(())
    }

    pub(crate) fn confirm(&self, peer: &Peer, deadline: Instant) -> Result<()> {
        use fs2::FileExt;
        use std::os::unix::fs::OpenOptionsExt;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.path.with_extension("lock"))?;
        loop {
            match lock.try_lock_exclusive() {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let left = deadline
                        .checked_duration_since(Instant::now())
                        .context("源记录锁等待超时")?;
                    std::thread::sleep(left.min(Duration::from_millis(1)));
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.check_peer(peer)?;
        let mut record = self.record()?;
        if record.peer.is_none() {
            record.peer = Some(peer.clone());
            publish(&self.path, &record, true)?;
        }
        Ok(())
    }

    pub(crate) fn launch(&self, operation: &str, marker: &str) -> Result<Vec<String>> {
        let mut command = self.ssh.clone();
        let position = command
            .iter()
            .rposition(|argument| argument == "--")
            .context("SSH 命令缺少目标边界")?;
        command.splice(
            position..position,
            [
                "-o".into(),
                format!("Hostname={}", self.endpoint.hostname),
                "-l".into(),
                self.endpoint.user.clone(),
                "-p".into(),
                self.endpoint.port.to_string(),
            ],
        );
        command.push(rpc::login::wrap(&rpc::login::framed(operation, marker)?)?);
        Ok(command)
    }
}

pub(crate) fn verify_existing(host: &Host, settings: &config::Config) -> Result<()> {
    let path = host.directory(settings).join("source.json");
    if !path.try_exists()? {
        return Ok(());
    }
    let record = load(&path)?;
    let endpoint = Binding::resolve(
        &host.ssh(settings)?,
        rpc::deadline_after(Duration::from_secs(settings.rpc_timeout_seconds))?,
    )?;
    ensure!(
        record.host_id == host.host_id && record.ssh == endpoint,
        "主机有效连接身份已改变；保留数据，拒绝写入"
    );
    Ok(())
}

pub(crate) fn cached_host(directory: &Path, settings: &config::Config) -> Result<Host> {
    let record = load(&directory.join("source.json"))?;
    ensure!(
        !record.host_id.is_empty() && record.host_id != "local",
        "缓存主机 ID 无效"
    );
    let host = Host {
        display_name: record.host_id.clone(),
        host_id: record.host_id,
        hostname: Some(record.ssh.hostname),
        ssh_port: Some(record.ssh.port),
        alias: None,
        identity: None,
        enabled: false,
    };
    ensure!(
        host.directory(settings) == directory,
        "缓存目录与主机身份不一致"
    );
    Ok(host)
}

#[cfg(test)]
#[path = "connection_source_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "duplicate_source_tests.rs"]
mod duplicate_tests;
