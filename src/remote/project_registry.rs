use crate::{config::Config, store::remote::identity};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    id: String,
    host_id: String,
    label: String,
    remote_path: String,
}

impl Project {
    fn value(self, state: &Value) -> Result<Value> {
        ensure!(
            [&self.id, &self.host_id, &self.label]
                .iter()
                .all(|value| !value.trim().is_empty())
                && self.host_id != "local"
                && self.remote_path.starts_with('/')
                && !self.remote_path.contains('\0'),
            "远程项目登记缺少有效身份、名称或绝对目录"
        );
        let label = state["codex-managed-remote-connections"]
            .as_array()
            .and_then(|hosts| hosts.iter().find(|host| host["hostId"] == self.host_id))
            .and_then(|host| host["displayName"].as_str())
            .unwrap_or(&self.host_id);
        Ok(
            json!({"id":identity(&self.host_id,&self.id),"nativeId":self.id,
            "remote":true,"hostId":self.host_id,"hostLabel":label,
            "label":format!("{} · {label}",self.label),"paths":[self.remote_path]}),
        )
    }
}

fn read(config: &Config) -> Result<Option<Vec<Value>>> {
    if config.sidebar_state_file.as_os_str().is_empty() {
        return Ok(None);
    }
    let bytes = match std::fs::read(&config.sidebar_state_file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let state: Value = serde_json::from_slice(&bytes).context("无法读取完整远程项目登记表")?;
    let Some(entries) = state.get("remote-projects") else {
        return Ok(None);
    };
    let projects: Vec<Project> =
        serde_json::from_value(entries.clone()).context("远程项目登记格式无效")?;
    let mut identifiers = HashSet::new();
    projects
        .into_iter()
        .map(|project| {
            let value = project.value(&state)?;
            ensure!(
                identifiers.insert(value["id"].clone().to_string()),
                "远程项目登记身份重复"
            );
            Ok(value)
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

pub(crate) fn catalog(
    config: &Config,
    fallback: impl FnOnce() -> Result<Vec<Value>>,
) -> Result<Vec<Value>> {
    match read(config)? {
        Some(projects) => Ok(projects),
        None => fallback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(directory: &std::path::Path, state: Value) -> Result<Config> {
        let path = directory.join("state.json");
        std::fs::write(&path, serde_json::to_vec(&state)?)?;
        Ok(Config {
            sidebar_state_file: path,
            ..Config::default()
        })
    }

    fn project(host: &str, identifier: &str, path: &str) -> Value {
        json!({"id":identifier,"hostId":host,"label":"项目","remotePath":path})
    }

    #[test]
    fn registered_projects_do_not_depend_on_git_probes_or_native_tool_availability() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let state = json!({"remote-projects":[project("remote:a","same","/remote/不存在/../项目"),
            project("remote:b","same","/another")],
            "codex-managed-remote-connections":[{"hostId":"remote:a","displayName":"主机 A"}]});
        let config = config(directory.path(), state)?;
        let projects = catalog(&config, || panic!("must not invoke the Git-probing tool"))?;
        assert_eq!(projects.len(), 2);
        assert_ne!(projects[0]["id"], projects[1]["id"]);
        assert_eq!(projects[0]["hostLabel"], "主机 A");
        assert_eq!(projects[0]["paths"], json!(["/remote/不存在/../项目"]));
        assert_eq!(projects[1]["hostLabel"], "remote:b");
        Ok(())
    }

    #[test]
    fn registry_removal_is_read_fresh_and_does_not_resurrect_a_tool_result() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = config(
            directory.path(),
            json!({"remote-projects":[project("remote:a","first","/one")]}),
        )?;
        assert_eq!(catalog(&config, || panic!("unexpected fallback"))?.len(), 1);
        std::fs::write(&config.sidebar_state_file, r#"{"remote-projects":[]}"#)?;
        assert!(catalog(&config, || panic!("must not resurrect removed projects"))?.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_or_ambiguous_registries_fail_before_fallback() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for entries in [
            json!({}),
            json!([{}]),
            json!([project("local", "one", "/one")]),
            json!([project("remote:a", "one", "relative")]),
            json!([
                project("remote:a", "one", "/one"),
                project("remote:a", "one", "/two")
            ]),
        ] {
            let config = config(directory.path(), json!({"remote-projects":entries}))?;
            assert!(catalog(&config, || panic!("invalid registry must not fall back")).is_err());
        }
        let config = config(directory.path(), json!({}))?;
        std::fs::write(&config.sidebar_state_file, "{broken")?;
        assert!(catalog(&config, || panic!("broken state must not fall back")).is_err());
        Ok(())
    }

    #[test]
    fn older_state_without_remote_registry_preserves_the_existing_reader() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = config(directory.path(), json!({}))?;
        let expected = vec![json!({"id":"legacy-tool-project"})];
        assert_eq!(catalog(&config, || Ok(expected.clone()))?, expected);
        assert!(catalog(&config, || anyhow::bail!("tool unavailable")).is_err());
        std::fs::remove_file(&config.sidebar_state_file)?;
        assert_eq!(catalog(&config, || Ok(expected.clone()))?, expected);
        Ok(())
    }
}
