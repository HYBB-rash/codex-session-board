use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

pub(super) struct CatalogGuard {
    codex_home: PathBuf,
    user_agent: Option<String>,
    peer_fingerprint: Option<String>,
}

impl CatalogGuard {
    pub(super) fn initialized(response: &Value) -> Result<Self> {
        let home = response["codexHome"].as_str().context("目录初始化缺少 codexHome")?;
        ensure!(
            home.starts_with('/') && home.len() <= 4096
                && !home.chars().any(char::is_control)
                && Path::new(home).components().all(|part|
                    matches!(part, Component::RootDir | Component::Normal(_))),
            "目录初始化 codexHome 无效"
        );
        Ok(Self {
            codex_home: PathBuf::from(home),
            user_agent: response["userAgent"].as_str().map(str::to_owned),
            peer_fingerprint: None,
        })
    }

    pub(super) fn bind(&mut self, expected_home: &Path, identity: Value) -> Result<()> {
        ensure!(self.codex_home == expected_home, "目录后台 codexHome 与已核验来源不一致");
        self.peer_fingerprint = Some(format!("{:x}", Sha256::digest(serde_json::to_vec(&identity)?)));
        Ok(())
    }

    pub(super) fn bind_local(&mut self, authority: &super::authority::LocalAuthority) -> Result<()> {
        authority.confirm()?;
        let home = std::env::var_os("HOME").context("本机目录读取缺少 HOME")?;
        let override_home = std::env::var_os("CODEX_HOME").map(PathBuf::from);
        let expected = local_home(Path::new(&home), override_home.as_deref())?;
        self.bind(&expected, json!({"localAuthority":authority, "codexHome":expected}))
    }

    pub(super) fn source(&self) -> Result<Value> {
        let fingerprint = self.peer_fingerprint.as_ref().context("目录来源尚未核验")?;
        let version = self.user_agent.as_deref().and_then(|agent|
            agent.split('(').next()?.trim().rsplit_once('/').map(|(_, version)| version.trim())
                .filter(|version| !version.is_empty() && !version.chars().any(char::is_whitespace)));
        Ok(json!({"codexHome":self.codex_home, "userAgent":self.user_agent,
            "version":version, "peerFingerprint":fingerprint}))
    }

    pub(super) fn check(&self, method: &str, params: &Value) -> Result<()> {
        self.source()?;
        ensure!(params.is_object(), "目录 RPC 参数必须是对象");
        match method {
            "project/import" => import_params(params)?,
            "thread/list" => ensure!(params["useStateDbOnly"] == Value::Bool(true),
                "目录读取必须使用 useStateDbOnly=true"),
            "thread/read" => ensure!(params["includeTurns"] == Value::Bool(false),
                "目录读取只允许 includeTurns=false"),
            "account/read" => ensure!(params["refreshToken"] == Value::Bool(false),
                "目录读取只允许 refreshToken=false"),
            "fs/getMetadata" | "fs/readFile" => {
                let expected = self.codex_home.join(".codex-global-state.json");
                ensure!(params["path"].as_str() == expected.to_str(),
                    "目录读取只允许已核验 codexHome 下的全局状态文件");
            }
            _ => {}
        }
        Ok(())
    }
}

fn import_params(params: &Value) -> Result<()> {
    let object = params.as_object().context("导入参数不是对象")?;
    ensure!(object.len() == 4 && ["idempotencyKey","name","roots","metadata"].iter()
        .all(|key| object.contains_key(*key)), "项目准备只允许精确旧项目导入，禁止会话迁移");
    let key = params["idempotencyKey"].as_str().context("导入缺少旧项目 ID")?;
    ensure!(!key.is_empty() && key.len() <= 4096 && !key.starts_with("g-p-")
        && !key.chars().any(char::is_control), "导入旧项目 ID 无效");
    ensure!(params["name"].as_str().is_some_and(|name| !name.trim().is_empty()), "导入名称无效");
    ensure!(params["roots"].as_array().is_some_and(|roots| !roots.is_empty() && roots.iter().all(|root|
        root.as_object().is_some_and(|fields| fields.len() == 1)
        && root["path"].as_str().is_some_and(|path| path.starts_with('/') && !path.chars().any(char::is_control)))),
        "导入根目录无效");
    ensure!(params["metadata"].as_object().is_some_and(|metadata| metadata.values().all(Value::is_string)),
        "导入 metadata 无效");
    Ok(())
}

fn local_home(home: &Path, override_home: Option<&Path>) -> Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    ensure!(home.is_absolute(), "本机 HOME 必须是绝对路径");
    let declared = override_home.map(Path::to_path_buf).unwrap_or_else(|| home.join(".codex"));
    ensure!(declared.is_absolute(), "本机 CODEX_HOME 必须是绝对路径");
    let canonical = declared.canonicalize().context("无法核验本机 CODEX_HOME")?;
    let metadata = canonical.metadata()?;
    ensure!(metadata.is_dir() && metadata.uid() == rustix::process::getuid().as_raw(),
        "本机 CODEX_HOME 所有者或类型无效");
    Ok(canonical)
}

#[cfg(test)]
#[path = "catalog_guard_tests.rs"]
mod tests;
