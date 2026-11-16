use crate::config::Config;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capability {
    protocol: String,
    archive: PathBuf,
    sha256: String,
    open_command: Vec<String>,
}

fn digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..length]);
    }
}

fn launch_archive(command: &[String]) -> Result<PathBuf> {
    let program = command.first().context("客户端启动命令为空")?;
    if matches!(program.as_str(), "open" | "/usr/bin/open") {
        let index = command.iter().position(|part| part == "-a")
            .context("远程原生新建需要显式指定客户端应用")?;
        let app = PathBuf::from(command.get(index + 1).context("客户端应用路径缺失")?);
        ensure!(app.is_absolute() && app.extension().is_some_and(|suffix| suffix == "app"),
            "客户端应用需要绝对路径");
        return Ok(app.join("Contents/Resources/app.asar"));
    }
    let executable = PathBuf::from(program);
    ensure!(executable.is_absolute(), "远程原生新建需要客户端程序的绝对路径");
    let parent = executable.parent().context("客户端程序缺少目录")?;
    if parent.file_name().is_some_and(|name| name == "MacOS") {
        return Ok(parent.parent().context("客户端应用目录无效")?.join("Resources/app.asar"));
    }
    Ok(parent.join("resources/app.asar"))
}

fn capability(config: &Config, target: bool) -> Result<()> {
    let path = config.native_new_capability.as_ref()
        .context("当前 Codex 客户端尚未配置远程原生新建入口，请更新客户端")?;
    let capability: Capability = serde_json::from_reader(std::fs::File::open(path)?)
        .context("无法核对 Codex 远程原生新建能力")?;
    ensure!((capability.protocol == "codex-remote-draft-v2"
        || (!target && capability.protocol == "codex-remote-draft-v1"))
        && capability.open_command == config.open_command
        && capability.archive.is_absolute()
        && launch_archive(&config.open_command)?.canonicalize()? == capability.archive.canonicalize()?
        && digest(&capability.archive)? == capability.sha256,
        "Codex 原生新建入口与当前客户端不匹配，请重新配置客户端");
    Ok(())
}

pub(crate) fn target_url(config: &Config, host: &str, project: &str, operation: &str) -> Result<String> {
    target_capability(config)?;
    target_route(host, project, operation)
}

pub(crate) fn target_capability(config: &Config) -> Result<()> {
    capability(config, true)
}

fn target_route(host: &str, project: &str, operation: &str) -> Result<String> {
    ensure!(!host.trim().is_empty() && host != "local", "目标新建需要远程主机");
    for identifier in [project, operation] {
        ensure!(uuid::Uuid::parse_str(identifier)?.to_string() == identifier, "目标新建 ID 无效");
    }
    let mut url = url::Url::parse("codex://threads/new-target")?;
    url.query_pairs_mut().append_pair("hostId", host).append_pair("projectId", project)
        .append_pair("operationId", operation);
    Ok(url.to_string())
}

pub(crate) fn remote_url(config: &Config, project: &Value) -> Result<String> {
    capability(config, false)?;
    let host = project["hostId"].as_str().context("远程项目缺少主机")?;
    let identifier = project["nativeId"].as_str().context("远程项目缺少原生 ID")?;
    ensure!(!host.trim().is_empty() && host != "local" && !identifier.trim().is_empty(),
        "远程新建目标无效");
    let mut url = url::Url::parse("codex://threads/new-remote")?;
    url.query_pairs_mut().append_pair("hostId", host).append_pair("projectId", identifier);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn target_route_contains_only_bound_identifiers() -> Result<()> {
        let identifier = "01a1d38a-3ab5-7741-a555-3aafd1ecb9a8";
        let operation = "01a1dfd4-904e-77f1-8052-3e2b388002ec";
        let url = url::Url::parse(&target_route("remote:ssh", identifier, operation)?)?;
        assert_eq!(url.path(), "/new-target");
        assert_eq!(url.query_pairs().collect::<Vec<_>>(), vec![
            ("hostId".into(), "remote:ssh".into()), ("projectId".into(), identifier.into()),
            ("operationId".into(), operation.into())]);
        assert!(target_route("local", identifier, operation).is_err());
        assert!(target_route("remote:ssh", "/path", operation).is_err());
        Ok(())
    }

    #[test]
    fn only_v2_capability_can_open_target_projects() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let resources = directory.path().join("resources");
        std::fs::create_dir(&resources)?;
        let archive = resources.join("app.asar");
        std::fs::write(&archive, b"fixture archive")?;
        let capability = directory.path().join("capability.json");
        let config = Config {
            open_command: vec![directory.path().join("codex").to_string_lossy().into_owned()],
            native_new_capability: Some(capability.clone()), ..Config::default()
        };
        let project = json!({"hostId":"remote:target","nativeId":"client-remote-project"});
        for protocol in ["codex-remote-draft-v1","codex-remote-draft-v2"] {
            std::fs::write(&capability, serde_json::to_vec(&json!({"protocol":protocol,
                "archive":archive,"sha256":digest(&archive)?,"openCommand":config.open_command}))?)?;
            assert!(remote_url(&config,&project).is_ok());
            assert_eq!(target_capability(&config).is_ok(),protocol == "codex-remote-draft-v2");
        }
        std::fs::write(&archive,b"updated archive")?;
        assert!(target_capability(&config).is_err());
        Ok(())
    }
}
