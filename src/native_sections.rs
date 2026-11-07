use crate::{config::Config, model, rpc::Rpc};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

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
        .status_sections
        .iter()
        .find(|(_, identifier)| section["sectionId"].as_str() == Some(identifier.as_str()))
        .map(|(state, _)| model::board_state(state))
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
    for (state, identifier) in &config.status_sections {
        let label = model::STATES
            .iter()
            .find(|item| item.0 == state)
            .context("未知状态分区")?
            .1;
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
    let atoms = state["electron-persisted-atom-state"]
        .as_object()
        .context("客户端别名数据无效")?;
    let mut aliases = HashMap::new();
    for (key, value) in atoms {
        if let Some(identifier) = key.strip_prefix("thread-client-id-v1:local%3A") {
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
    Ok(snapshot(reader, config)?
        .remove(identifier)
        .unwrap_or(observe(config, &Value::Null)?))
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
    let state = model::board_state(state);
    let section = config
        .status_sections
        .get(state)
        .context("目标原生分区未配置")?;
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
    }
}
