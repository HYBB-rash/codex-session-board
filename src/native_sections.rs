use crate::{config::Config, rpc::Rpc};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub mod direct;

fn client_atoms(state: &Value) -> Result<&serde_json::Map<String, Value>> {
    let atoms = if let Some(owner) = state.get("electron-renderer-content-owner") {
        let owner = owner.as_str().filter(|value| !value.is_empty())
            .context("客户端当前账号标识无效")?;
        &state[format!("electron-owner-atoms:{owner}")]
    } else {
        &state["electron-persisted-atom-state"]
    };
    atoms.as_object().context("客户端当前账号数据无效")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub state: String,
    pub version: String,
}

#[derive(Debug)]
pub struct UnconfirmedMove;

impl std::fmt::Display for UnconfirmedMove {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("原生移动回执未确认")
    }
}

impl std::error::Error for UnconfirmedMove {}

pub fn observe(config: &Config, section: &Value) -> Result<Observation> {
    ensure!(
        section.is_null() || section["sectionId"].is_string(),
        "原生分区格式无效"
    );
    let state = config
        .section_bindings()
        .find(|(_, _, identifier)| section["sectionId"].as_str() == Some(*identifier))
        .map(|(state, _, _)| state)
        .unwrap_or("unknown");
    Ok(Observation {
        state: state.into(),
        version: section["sectionId"]
            .as_str()
            .unwrap_or("unsectioned")
            .into(),
    })
}

pub fn snapshot(reader: &mut impl Rpc, config: &Config) -> Result<HashMap<String, Observation>> {
    let result = reader.call("list_threads", json!({"limit":1}))?;
    ensure!(
        result["unavailableHosts"]
            .as_array()
            .is_some_and(|hosts| !hosts.iter().any(|host| host == &config.host_id)),
        "原生分区主机不可用"
    );
    let catalog = result["sections"].as_array().context("原生分区目录无效")?;
    validate_catalog(config, catalog)?;
    collect(config, catalog)
}

fn validate_catalog(config: &Config, catalog: &[Value]) -> Result<()> {
    for (_, label, identifier) in config.section_bindings() {
        let matches: Vec<_> = catalog
            .iter()
            .filter(|section| section["sectionId"] == *identifier)
            .collect();
        ensure!(
            matches.len() == 1 && matches[0]["name"] == label,
            "原生分区 ID 或名称不匹配：{label}"
        );
    }
    Ok(())
}

fn collect(config: &Config, catalog: &[Value]) -> Result<HashMap<String, Observation>> {
    let aliases = aliases(config)?;
    let prefix = format!("codex:thread:{}:", config.host_id);
    let mut observed = HashMap::new();
    let mut seen = HashSet::new();
    for section in catalog {
        let observation = observe(config, section)?;
        ensure!(seen.insert(observation.version.clone()), "原生分区 ID 重复");
        for item in section["itemKeys"].as_array().context("原生分区成员无效")? {
            let key = item.as_str().context("原生成员标识无效")?;
            let Some(identifier) = key.strip_prefix(&prefix) else {
                continue;
            };
            let canonical = aliases
                .get(identifier)
                .map(String::as_str)
                .unwrap_or(identifier);
            ensure!(
                observed
                    .insert(canonical.into(), observation.clone())
                    .is_none(),
                "会话出现在多个原生分区"
            );
        }
    }
    Ok(observed)
}

fn aliases(config: &Config) -> Result<HashMap<String, String>> {
    if config.sidebar_state_file.as_os_str().is_empty() {
        return Ok(HashMap::new());
    }
    let state: Value = serde_json::from_reader(std::fs::File::open(&config.sidebar_state_file)?)?;
    let atoms = client_atoms(&state)?;
    let mut aliases = HashMap::new();
    for (key, value) in atoms {
        if let Some(identifier) = key.strip_prefix("thread-client-id-v1:local%3A") {
            uuid::Uuid::parse_str(identifier).context("客户端会话标识无效")?;
            let alias = value.as_str().context("客户端会话别名无效")?;
            ensure!(
                aliases.insert(alias.into(), identifier.into()).is_none(),
                "客户端别名存在冲突"
            );
        }
    }
    Ok(aliases)
}

pub fn read(reader: &mut impl Rpc, config: &Config, identifier: &str) -> Result<Observation> {
    lookup(config, &snapshot(reader, config)?, identifier)
}

pub fn lookup(
    config: &Config,
    snapshot: &HashMap<String, Observation>,
    identifier: &str,
) -> Result<Observation> {
    if let Some(observed) = snapshot.get(identifier) {
        return Ok(observed.clone());
    }
    ensure!(
        !snapshot
            .keys()
            .any(|key| key.starts_with("client-new-thread:")),
        "客户端别名尚未确认，保留上次原生位置"
    );
    observe(config, &Value::Null)
}

pub fn move_checked(
    reader: &mut impl Rpc,
    config: &Config,
    identifier: &str,
    expected: &str,
    state: Option<&str>,
) -> Result<Observation> {
    let before = read(reader, config, identifier)?;
    ensure!(before.version == expected, "原生分区已更新，请刷新后重试");
    let Some(state) = state else {
        return Ok(before);
    };
    let section = config.section_target(state)?;
    if before.version == *section {
        return Ok(before);
    }
    reader.call(
        "move_thread_to_sidebar_section",
        json!({"threadId":identifier,"sectionId":section,"source":"codex","hostId":config.host_id}),
    ).context(UnconfirmedMove)?;
    let after = read(reader, config, identifier).context(UnconfirmedMove)?;
    ensure!(after.version == *section, "原生分区写入未确认，请刷新核对");
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};

    struct Desktop {
        replies: VecDeque<Value>,
        methods: Vec<String>,
    }

    impl Rpc for Desktop {
        fn call(&mut self, method: &str, _: Value) -> Result<Value> {
            self.methods.push(method.into());
            self.replies.pop_front().context("unexpected RPC")
        }
    }

    fn config() -> Config {
        Config {
            status_sections: BTreeMap::from([
                ("review".into(), "review-id".into()),
                ("to_archive".into(), "archive-id".into()),
            ]),
            ..Config::default()
        }
    }

    fn thread(section: &str, _: i64) -> Value {
        json!({"sectionId":section})
    }

    fn response(section: &str) -> Value {
        json!({"unavailableHosts":[],"sections":[
            {"sectionId":"review-id","name":"待审阅","itemKeys":if section=="review-id" {vec!["codex:thread:local:thread"]}else{vec![]}},
            {"sectionId":"archive-id","name":"待归档","itemKeys":if section=="archive-id" {vec!["codex:thread:local:thread"]}else{vec![]}}
        ]})
    }

    #[test]
    fn unmapped_native_sections_are_unknown_and_missing_fields_are_rejected() -> Result<()> {
        assert_eq!(observe(&config(), &thread("pinned", 1))?.state, "unknown");
        assert_eq!(observe(&config(), &Value::Null)?.state, "unknown");
        assert!(observe(&config(), &json!({})).is_err());
        Ok(())
    }

    #[test]
    fn changed_native_placement_rejects_the_move_before_any_write() -> Result<()> {
        let mut desktop = Desktop {
            replies: [response("archive-id")].into(),
            methods: vec![],
        };
        let expected = observe(&config(), &thread("review-id", 1))?;
        assert!(
            move_checked(
                &mut desktop,
                &config(),
                "thread",
                &expected.version,
                Some("to_archive")
            )
            .is_err()
        );
        assert_eq!(desktop.methods, ["list_threads"]);
        Ok(())
    }

    #[test]
    fn move_reads_back_native_state_and_never_reconciles_other_threads() -> Result<()> {
        let mut desktop = Desktop {
            replies: [response("review-id"), json!({}), response("archive-id")].into(),
            methods: vec![],
        };
        let expected = observe(&config(), &thread("review-id", 1))?;
        assert_eq!(
            move_checked(
                &mut desktop,
                &config(),
                "thread",
                &expected.version,
                Some("to_archive")
            )?
            .state,
            "to_archive"
        );
        assert_eq!(
            desktop.methods,
            [
                "list_threads",
                "move_thread_to_sidebar_section",
                "list_threads"
            ]
        );
        Ok(())
    }

    #[test]
    fn unconfirmed_moves_fail_instead_of_creating_a_retry_intent() -> Result<()> {
        let mut desktop = Desktop {
            replies: [response("review-id"), json!({}), response("review-id")].into(),
            methods: vec![],
        };
        let expected = observe(&config(), &thread("review-id", 1))?;
        assert!(
            move_checked(
                &mut desktop,
                &config(),
                "thread",
                &expected.version,
                Some("to_archive")
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn catalog_rejects_missing_or_duplicated_section_identifiers() {
        let mut desktop = Desktop {
            replies: [json!({"unavailableHosts":[],"sections":[]})].into(),
            methods: vec![],
        };
        assert!(snapshot(&mut desktop, &config()).is_err());
        let mut duplicate = response("review-id");
        let repeated = duplicate["sections"][0].clone();
        duplicate["sections"].as_array_mut().unwrap().push(repeated);
        desktop.replies = [duplicate].into();
        assert!(snapshot(&mut desktop, &config()).is_err());
        let mut renamed = response("review-id");
        renamed["sections"][0]["name"] = json!("其他名称");
        desktop.replies = [renamed].into();
        assert!(snapshot(&mut desktop, &config()).is_err());
    }

    fn aliased(section: &str) -> Value {
        let mut result = response(section);
        for entry in result["sections"].as_array_mut().unwrap() {
            for member in entry["itemKeys"].as_array_mut().unwrap() {
                *member = json!("codex:thread:local:client-new-thread:alias");
            }
        }
        result
    }

    fn alias_config(identifier: &str) -> Result<(tempfile::TempDir, Config)> {
        let directory = tempfile::tempdir()?;
        let mut config = config();
        config.sidebar_state_file = directory.path().join("aliases.json");
        let mut atoms = serde_json::Map::new();
        atoms.insert(
            format!("thread-client-id-v1:local%3A{identifier}"),
            json!("client-new-thread:alias"),
        );
        std::fs::write(
            &config.sidebar_state_file,
            json!({"electron-persisted-atom-state":atoms}).to_string(),
        )?;
        Ok((directory, config))
    }

    #[test]
    fn aliases_follow_the_current_owner_without_legacy_fallback() -> Result<()> {
        let identifier = "11111111-1111-4111-8111-111111111111";
        let (_directory, config) = alias_config(identifier)?;
        let original: Value = serde_json::from_slice(&std::fs::read(&config.sidebar_state_file)?)?;
        let mut state = json!({
            "electron-renderer-content-owner":"current",
            "electron-owner-atoms:current":original["electron-persisted-atom-state"],
            "electron-persisted-atom-state":{}
        });
        std::fs::write(&config.sidebar_state_file, state.to_string())?;
        assert_eq!(aliases(&config)?.get("client-new-thread:alias").map(String::as_str), Some(identifier));
        state["electron-renderer-content-owner"] = json!("missing");
        state["electron-persisted-atom-state"] = original["electron-persisted-atom-state"].clone();
        std::fs::write(&config.sidebar_state_file, state.to_string())?;
        assert!(aliases(&config).is_err());
        Ok(())
    }

    #[test]
    fn native_aliases_confirm_canonical_ids_without_repeating_moves() -> Result<()> {
        let identifier = "11111111-1111-4111-8111-111111111111";
        let (_directory, config) = alias_config(identifier)?;
        let mut desktop = Desktop {
            replies: [
                aliased("review-id"),
                json!({}),
                aliased("archive-id"),
                aliased("archive-id"),
            ]
            .into(),
            methods: vec![],
        };
        let after = move_checked(
            &mut desktop,
            &config,
            identifier,
            "review-id",
            Some("to_archive"),
        )?;
        move_checked(
            &mut desktop,
            &config,
            identifier,
            &after.version,
            Some("to_archive"),
        )?;
        assert_eq!(
            desktop.methods,
            [
                "list_threads",
                "move_thread_to_sidebar_section",
                "list_threads",
                "list_threads"
            ]
        );
        Ok(())
    }

    #[test]
    fn unresolved_aliases_block_only_unmatched_ids_without_guessing_a_default() -> Result<()> {
        let mut reply = aliased("review-id");
        reply["sections"][0]["itemKeys"]
            .as_array_mut()
            .unwrap()
            .push(json!("codex:thread:local:known"));
        let mut desktop = Desktop {
            replies: [reply.clone(), reply].into(),
            methods: vec![],
        };
        assert!(read(&mut desktop, &config(), "unmatched").is_err());
        assert_eq!(read(&mut desktop, &config(), "known")?.state, "review");
        assert_eq!(desktop.methods, ["list_threads", "list_threads"]);
        Ok(())
    }
}
