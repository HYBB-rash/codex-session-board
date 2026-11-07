use crate::{config::Config, model, rpc::Rpc};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub state: String,
    pub version: String,
}

pub fn observe(config: &Config, thread: &Value) -> Result<Observation> {
    let section = thread.get("section").context("原生响应缺少分区字段")?;
    ensure!(
        section.is_null() || section["id"].is_string(),
        "原生分区格式无效"
    );
    let state = config
        .status_sections
        .iter()
        .find(|(_, identifier)| section["id"].as_str() == Some(identifier.as_str()))
        .map(|(state, _)| model::board_state(state))
        .unwrap_or("unknown");
    Ok(Observation {
        state: state.into(),
        version: json!([section["id"], thread["sectionEnteredAt"]]).to_string(),
    })
}

pub fn verify_catalog(reader: &mut impl Rpc, config: &Config) -> Result<()> {
    let mut catalog = Vec::new();
    let mut cursor = Value::Null;
    let mut seen = HashSet::new();
    for _ in 0..config.max_pages {
        let result = reader.call(
            "threadSection/list",
            json!({"limit":config.page_size,"cursor":cursor}),
        )?;
        catalog.extend(
            result["data"]
                .as_array()
                .context("原生分区目录无效")?
                .iter()
                .cloned(),
        );
        cursor = result["nextCursor"].clone();
        if cursor.is_null() {
            for (state, identifier) in &config.status_sections {
                let label = model::STATES
                    .iter()
                    .find(|item| item.0 == state)
                    .context("未知状态分区")?
                    .1;
                ensure!(
                    catalog
                        .iter()
                        .any(|section| section["id"] == *identifier && section["name"] == label),
                    "原生分区 ID 或名称不匹配：{label}"
                );
            }
            return Ok(());
        }
        ensure!(
            cursor.is_string() && seen.insert(cursor.to_string()),
            "原生分区分页游标无效"
        );
    }
    anyhow::bail!("原生分区目录未完整读取")
}

fn read(reader: &mut impl Rpc, config: &Config, identifier: &str) -> Result<Observation> {
    let result = reader.call(
        "thread/read",
        json!({"threadId":identifier,"includeTurns":false}),
    )?;
    ensure!(result["thread"]["id"] == identifier, "原生会话身份不匹配");
    observe(config, &result["thread"])
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
    if before.state == state {
        return Ok(before);
    }
    reader.call(
        "thread/section/move",
        json!({"threadId":identifier,"sectionId":section}),
    )?;
    let after = read(reader, config, identifier)?;
    ensure!(after.state == state, "原生分区写入未确认，请刷新核对");
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

    fn thread(section: &str, entered: i64) -> Value {
        json!({"id":"thread","section":{"id":section},"sectionEnteredAt":entered})
    }

    #[test]
    fn unmapped_native_sections_are_unknown_and_missing_fields_are_rejected() -> Result<()> {
        assert_eq!(observe(&config(), &thread("pinned", 1))?.state, "unknown");
        assert_eq!(
            observe(&config(), &json!({"section":null}))?.state,
            "unknown"
        );
        assert!(observe(&config(), &json!({})).is_err());
        Ok(())
    }

    #[test]
    fn changed_native_placement_rejects_the_move_before_any_write() -> Result<()> {
        let mut desktop = Desktop {
            replies: [json!({"thread":thread("review-id",2)})].into(),
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
        assert_eq!(desktop.methods, ["thread/read"]);
        Ok(())
    }

    #[test]
    fn move_reads_back_native_state_and_never_reconciles_other_threads() -> Result<()> {
        let mut desktop = Desktop {
            replies: [
                json!({"thread":thread("review-id",1)}),
                json!({}),
                json!({"thread":thread("archive-id",2)}),
            ]
            .into(),
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
            ["thread/read", "thread/section/move", "thread/read"]
        );
        Ok(())
    }

    #[test]
    fn unconfirmed_moves_fail_instead_of_creating_a_retry_intent() -> Result<()> {
        let mut desktop = Desktop {
            replies: [
                json!({"thread":thread("review-id",1)}),
                json!({}),
                json!({"thread":thread("review-id",1)}),
            ]
            .into(),
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
    fn catalog_rejects_ui_layer_identifiers_and_repeated_cursors() {
        let mut desktop = Desktop {
            replies: [json!({"data":[{"id":"ui-layer-id","name":"待审阅"}]})].into(),
            methods: vec![],
        };
        assert!(verify_catalog(&mut desktop, &config()).is_err());
        desktop.replies = [
            json!({"data":[],"nextCursor":"same"}),
            json!({"data":[],"nextCursor":"same"}),
        ]
        .into();
        assert!(verify_catalog(&mut desktop, &config()).is_err());
    }
}
