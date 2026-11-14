use super::legacy::Projection;
use crate::rpc::Rpc;
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Input {
    pub hash: String,
    pub metadata: Value,
    pub projection: Projection,
}

pub(crate) fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub(crate) fn read(reader: &mut dyn Rpc, codex_home: &str) -> Result<Input> {
    let account = reader.call("account/read", json!({"refreshToken":false}));
    let (account, mut issues) = match account {
        Ok(account) => (account, Vec::new()),
        Err(error) => (Value::Null, vec![format!("accountUnavailable:{error}")]),
    };
    let identity = json!([
        account["userId"],
        account["workspaceRouting"]["chatgptAccountId"]
    ]);
    let path = format!("{codex_home}/.codex-global-state.json");
    let state = state_file(reader, &path);
    let (raw_hash, metadata, projection) = match state {
        Ok((bytes, metadata)) => {
            let raw_hash = digest(&bytes);
            let projection = serde_json::from_slice::<Value>(&bytes)
                .map_err(anyhow::Error::from)
                .and_then(|state| Projection::parse(&state, codex_home, &account));
            match projection {
                Ok(projection) => (raw_hash, metadata, projection),
                Err(error) => {
                    issues.push(format!("legacyInvalid:{error}"));
                    (raw_hash, metadata, Projection::default())
                }
            }
        }
        Err(error) => {
            let message = format!("legacyUnavailable:{error}");
            let hash = digest(message.as_bytes());
            issues.push(message);
            (hash, Value::Null, Projection::default())
        }
    };
    let mut projection = projection;
    projection.issues.extend(issues);
    let hash = digest(&serde_json::to_vec(&json!([raw_hash, identity]))?);
    Ok(Input {
        hash,
        metadata,
        projection,
    })
}

fn state_file(reader: &mut dyn Rpc, path: &str) -> Result<(Vec<u8>, Value)> {
    let metadata = metadata(reader, path)?;
    ensure!(
        metadata["isFile"] == true && metadata["isSymlink"] != true,
        "目标全局状态不是普通文件"
    );
    let response = reader.call("fs/readFile", json!({"path":path}))?;
    let encoded = response["dataBase64"]
        .as_str()
        .context("文件响应缺少 Base64")?;
    ensure!(
        encoded.len() <= MAX_STATE_BYTES.div_ceil(3) * 4,
        "目标全局状态超过 16 MiB"
    );
    let bytes = STANDARD.decode(encoded).context("文件 Base64 无效")?;
    ensure!(bytes.len() <= MAX_STATE_BYTES, "目标全局状态超过 16 MiB");
    Ok((bytes, metadata))
}

pub(crate) fn metadata(reader: &mut dyn Rpc, path: &str) -> Result<Value> {
    let result = reader.call("fs/getMetadata", json!({"path":path}))?;
    ensure!(result.is_object(), "文件元数据格式无效");
    Ok(
        json!({"isFile":result["isFile"],"isSymlink":result["isSymlink"],
        "modifiedAtMs":result["modifiedAtMs"],"createdAtMs":result["createdAtMs"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct File(Value);

    impl Rpc for File {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            if method == "account/read" {
                assert_eq!(params, json!({"refreshToken":false}));
                return Ok(
                    json!({"userId":"user","workspaceRouting":{"chatgptAccountId":"account"},
                    "email":"never-cache@example.invalid"}),
                );
            }
            assert_eq!(params["path"], "/remote/.codex/.codex-global-state.json");
            match method {
                "fs/getMetadata" => Ok(json!({"isFile":true,"modifiedAtMs":1})),
                "fs/readFile" => Ok(self.0.clone()),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn invalid_json_and_base64_are_partial_legacy_evidence() -> Result<()> {
        for response in [
            json!({"dataBase64":"invalid!"}),
            json!({"dataBase64":STANDARD.encode(b"not-json")}),
        ] {
            let input = read(&mut File(response), "/remote/.codex")?;
            assert!(!input.projection.issues.is_empty());
            let saved = serde_json::to_string(&input)?;
            assert!(!saved.contains("never-cache"));
            assert!(!saved.contains("not-json"));
        }
        Ok(())
    }
}
