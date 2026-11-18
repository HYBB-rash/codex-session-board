use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Write, path::Path, time::Duration};
use crate::{config::Config, remote::hosts::{Host, source::{Binding, Endpoint}}, rpc::{Role, Rpc}};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    pub(super) hostname: String,
    pub(super) user: String,
    pub(super) port: String,
    pub(super) directory: String,
}

pub(super) fn name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 253 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

impl Target {
    pub(super) fn parse(value: &Value) -> Result<Self> {
        let mut target: Self = serde_json::from_value(value.clone()).context("连接目标格式无效")?;
        ensure!(name(&target.hostname), "请输入明确的 SSH 主机名或 IPv4 地址");
        ensure!(name(&target.user) && target.user.len() <= 128, "SSH 用户无效");
        let port = target.port.parse::<u16>().context("SSH 端口无效")?;
        ensure!(port != 0, "SSH 端口无效");
        ensure!(target.directory.starts_with('/') && target.directory.len() <= 4096
            && !target.directory.starts_with("//") && !target.directory.chars().any(char::is_control)
            && Path::new(&target.directory).components().all(|part| matches!(part,
                std::path::Component::RootDir | std::path::Component::Normal(_))),
            "请输入项目绝对目录");
        target.port = port.to_string();
        target.hostname.make_ascii_lowercase();
        Ok(target)
    }

    pub(super) fn key(&self) -> Result<String> {
        Ok(super::digest(&serde_json::to_vec(
            &(&self.hostname, &self.user, &self.port))?))
    }

    pub(super) fn direct(&self) -> String {
        format!("{}@{}", self.user, self.hostname)
    }

    pub(super) fn alias(&self, reserved: &BTreeSet<String>) -> Result<String> {
        let readable: String = self.hostname.split('.').next().unwrap_or_default()
            .chars().take(30).map(|character| if character == '_' { '-' } else { character }).collect();
        let short = format!("csb-{readable}");
        let key = self.key()?;
        [short.clone(), format!("{short}-{}", &key[..8]), format!("{short}-{}", &key[..16])]
            .into_iter().find(|name| !reserved.iter().any(|existing| existing.eq_ignore_ascii_case(name)))
            .context("无法分配无冲突的 SSH 别名")
    }

    pub(super) fn block(&self, alias: &str) -> String {
        format!("\nHost {alias}\n    HostName {}\n    User {}\n    Port {}\n",
            self.hostname, self.user, self.port)
    }

    pub(super) fn summary(&self, alias: &str) -> String {
        format!("{}:{} · {} · {alias}", self.direct(), self.port, self.directory)
    }

    pub(super) fn endpoint(&self) -> Result<Endpoint> {
        Ok(Endpoint { hostname: self.hostname.clone(), user: self.user.clone(), port: self.port.parse()? })
    }
}

pub(super) struct Probe {
    temporary: tempfile::TempDir,
    config: Config,
}

impl Probe {
    pub(super) fn new(config: &Config, contents: &str) -> Result<Self> {
        Self::configured(config, Some(contents))
    }

    pub(super) fn installed(config: &Config) -> Result<Self> {
        Self::configured(config, None)
    }

    fn configured(config: &Config, contents: Option<&str>) -> Result<Self> {
        let temporary = tempfile::tempdir()?;
        let mut command = config.ssh_command.clone();
        if let Some(contents) = contents {
            let saved = temporary.path().join("config");
            let mut file = tempfile::NamedTempFile::new_in(temporary.path())?;
            file.write_all(contents.as_bytes())?;
            file.persist(&saved)?;
            command.extend(["-F".into(), saved.to_string_lossy().into_owned()]);
        }
        command.extend([
            "-o".into(), "StrictHostKeyChecking=yes".into(), "-o".into(), "UpdateHostKeys=no".into(),
            "-o".into(), "ControlMaster=no".into(), "-o".into(), "ControlPath=none".into()]);
        let config = Config {
            ssh_command: command,
            data_dir: temporary.path().join("probe"),
            sidebar_state_file: std::path::PathBuf::new(),
            ..config.clone()
        };
        Ok(Self { temporary, config })
    }

    pub(super) fn settings(&self, host: &Host) -> Result<Value> {
        let mut command = host.ssh(&self.config)?;
        command.insert(self.config.ssh_command.len(), "-G".into());
        let deadline = crate::rpc::deadline_after(Duration::from_secs(self.config.rpc_timeout_seconds))?;
        let output = crate::rpc::capture::output(&command, deadline)?;
        let mut fields = serde_json::Map::new();
        for line in std::str::from_utf8(&output)?.lines() {
            if let Some((name, value)) = line.split_once(' ') && name != "host" {
                let values = fields.entry(name.to_owned()).or_insert_with(|| json!([]));
                if let Some(values) = values.as_array_mut() {
                    values.push(json!(value));
                }
            }
        }
        Ok(Value::Object(fields))
    }

    pub(super) fn proof(&self, host: &Host, target: &Target) -> Result<Value> {
        let mut config = self.config.clone();
        config.host_id = host.host_id.clone();
        config.data_dir = self.temporary.path().join(uuid::Uuid::new_v4().to_string());
        config.proxy_command = host.proxy(&self.config)?;
        config.remote_binding = Some(Binding::prepare(host, &config)?);
        let binding = config.remote_binding.clone().context("缺少 SSH 来源绑定")?;
        let temporary_app = crate::app::App::new(config)?;
        let timeout = Duration::from_secs(self.config.rpc_timeout_seconds);
        let mut reader = temporary_app.connection(timeout, Role::SetupReader)?;
        let source = reader.catalog_source()?;
        let account = reader.call("account/read", json!({"refreshToken":false}))?;
        let account = setup_account(&account)?;
        let project = verify_directory(&mut reader, target)?;
        let identity = binding.mapping_identity()?;
        ensure!(identity["ssh"] == serde_json::to_value(target.endpoint()?)?,
            "SSH 生效主机、用户或端口与输入不同");
        Ok(json!({"source":source["peerFingerprint"],"peer":identity["peer"],"account":account,"project":project}))
    }
}

fn setup_account(response: &Value) -> Result<Value> {
    if let Some(fingerprint) = crate::catalog::account_fingerprint(response)? {
        return Ok(json!({"fingerprint":fingerprint}));
    }
    ensure!(response["requiresOpenaiAuth"] == false && response["account"].is_null()
        && response["userId"].is_null() && response["workspaceRouting"].is_null(),
        "无法确认远端 Codex 账号");
    Ok(json!({"provider":"no-openai-auth"}))
}

fn verify_directory(reader: &mut impl Rpc, target: &Target) -> Result<String> {
    let mut cursor = Value::Null;
    let mut visited = BTreeSet::new();
    let mut matched = None;
    loop {
        let result = reader.call("project/list", json!({"limit":100,"cursor":cursor}))?;
        let projects = result["data"].as_array().context("无法确认远端项目目录")?;
        for project in projects.iter().filter(|project| {
            project["roots"].as_array().is_some_and(|roots| roots.len() == 1
                && roots[0]["path"] == target.directory)
        }) {
            ensure!(matched.is_none(), "远端目录对应多个项目，请在 Codex 核对项目归属");
            matched = Some(project["id"].as_str().filter(|id| !id.is_empty())
                .context("远端项目缺少身份")?.to_owned());
        }
        cursor = result["nextCursor"].clone();
        if cursor.is_null() {
            return matched.context("远端项目列表未找到该单目录项目");
        }
        ensure!(cursor.as_str().is_some_and(|value| visited.len() < 100 && visited.insert(value.to_owned())),
            "远端项目列表未能验证完整");
    }
}

pub(super) fn host(target: &Target, alias: Option<&str>) -> Result<Host> {
    Ok(Host {
        host_id: alias.map_or_else(|| "remote:setup-probe".into(), |alias| format!("remote-ssh-discovered:{alias}")),
        display_name: target.hostname.clone(), alias: alias.map(str::to_owned),
        hostname: Some(target.direct()), ssh_port: alias.is_none().then(|| target.port.parse()).transpose()?,
        identity: None, enabled: true, auto_connect: true,
    })
}
