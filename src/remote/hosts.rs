use crate::config::Config;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[path = "connection_source.rs"]
pub mod source;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub host_id: String,
    pub display_name: String,
    pub alias: Option<String>,
    pub hostname: Option<String>,
    pub ssh_port: Option<u16>,
    pub identity: Option<String>,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

pub fn registered(config: &Config) -> Result<Vec<Host>> {
    registered_at(&config.sidebar_state_file)
}

pub(crate) fn registered_at(path: &std::path::Path) -> Result<Vec<Host>> {
    let state: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut hosts: Vec<Host> = serde_json::from_value(
        state["codex-managed-remote-connections"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into(),
    )?;
    let mut identifiers = std::collections::HashSet::new();
    for host in &mut hosts {
        ensure!(
            !host.host_id.is_empty() && host.host_id != "local",
            "远程主机 ID 无效"
        );
        ensure!(identifiers.insert(&host.host_id), "远程主机 ID 重复");
        host.enabled = state["remote-connection-auto-connect-by-host-id"][&host.host_id] != false;
    }
    Ok(hosts)
}

impl Host {
    pub fn verify_source(&self, config: &Config) -> Result<()> {
        source::verify_existing(self, config)
    }

    pub(crate) fn ssh(&self, config: &Config) -> Result<Vec<String>> {
        self.ssh_with(&config.ssh_command, config.rpc_timeout_seconds)
    }

    pub(crate) fn ssh_with(&self, program: &[String], timeout_seconds: u64) -> Result<Vec<String>> {
        let target = self
            .alias
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or(self.hostname.as_deref().filter(|value| !value.trim().is_empty()))
            .context("主机没有 SSH 连接信息")?;
        ensure!(
            !target.starts_with('-') && !target.chars().any(char::is_control),
            "SSH 目标无效"
        );
        let mut command = program.to_vec();
        ensure!(!command.is_empty() && command.iter().all(|part| !part.is_empty()), "SSH 命令无效");
        command.extend([
            "-T".into(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            format!("ConnectTimeout={}", timeout_seconds.min(10)),
            "-o".into(),
            "ServerAliveInterval=15".into(),
            "-o".into(),
            "ServerAliveCountMax=2".into(),
        ]);
        if let Some(port) = self.ssh_port {
            ensure!(port != 0, "SSH 端口无效");
            command.extend(["-p".into(), port.to_string()]);
        }
        if let Some(identity) = &self.identity {
            ensure!(!identity.is_empty(), "SSH 身份路径为空");
            command.extend(["-i".into(), identity.clone()]);
        }
        command.extend(["--".into(), target.into()]);
        Ok(command)
    }

    pub fn command(&self, config: &Config, operation: &str) -> Result<Vec<String>> {
        let mut command = self.ssh(config)?;
        command.push(crate::rpc::login::wrap(operation)?);
        Ok(command)
    }

    pub fn proxy(&self, config: &Config) -> Result<Vec<String>> {
        self.command(config, r#"if command -v codex >/dev/null 2>&1; then exec codex app-server proxy; else exec "$HOME/.local/bin/codex" app-server proxy; fi"#)
    }

    pub fn directory(&self, config: &Config) -> PathBuf {
        config
            .data_dir
            .join("remote-hosts")
            .join(format!("{:x}", Sha256::digest(self.host_id.as_bytes())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registered_hosts_do_not_depend_on_projects_and_respect_disabled_connections() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("state.json");
        let host = json!({"hostId":"remote:a","displayName":"测试","alias":"example",
            "hostname":null,"sshPort":2222,"identity":null});
        std::fs::write(
            &path,
            json!({"codex-managed-remote-connections":[host],
            "remote-connection-auto-connect-by-host-id":{"remote:a":false}})
            .to_string(),
        )?;
        let config = Config {
            data_dir: directory.path().into(),
            sidebar_state_file: path,
            ..Config::default()
        };
        let hosts = registered(&config)?;
        assert_eq!(hosts.len(), 1);
        assert!(!hosts[0].enabled);
        let command = hosts[0].proxy(&config)?;
        assert_eq!(
            &command[command.len() - 3..],
            ["--", "example", command.last().unwrap()]
        );
        assert!(
            !command
                .iter()
                .any(|part| part.contains("StrictHostKeyChecking=no"))
        );
        Ok(())
    }

    #[test]
    fn host_paths_cannot_escape_and_ssh_options_cannot_be_injected() -> Result<()> {
        let mut host: Host = serde_json::from_value(json!({"hostId":"../outside",
            "displayName":"测试","alias":"-oProxyCommand=bad","hostname":null,
            "sshPort":null,"identity":null}))?;
        let config = Config {
            data_dir: PathBuf::from("/tmp/board"),
            ..Config::default()
        };
        assert!(host.proxy(&config).is_err());
        host.alias = Some("safe host; literal".into());
        let command = host.proxy(&config)?;
        assert_eq!(command[command.len() - 2], "safe host; literal");
        assert_eq!(
            host.directory(&config).parent(),
            Some(config.data_dir.join("remote-hosts").as_path())
        );
        Ok(())
    }
}
