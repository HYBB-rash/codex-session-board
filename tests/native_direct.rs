#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::{Result, bail};
use codex_session_board::{config::Config, native_sections::direct, rpc::Rpc};
use serde_json::{Value, json};
use std::collections::VecDeque;

const THREAD: &str = "11111111-1111-4111-8111-111111111111";

struct Reader {
    replies: VecDeque<Value>,
    calls: Vec<String>,
}

impl Rpc for Reader {
    fn call(&mut self, method: &str, _: Value) -> Result<Value> {
        self.calls.push(method.into());
        match method {
            "threadSection/list" | "thread/read" | "thread/list" => self
                .replies
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("unexpected read")),
            _ => bail!("aggregate or write reached the native reader"),
        }
    }
}

fn setup() -> Result<(tempfile::TempDir, Config, Value)> {
    let directory = tempfile::tempdir()?;
    let config = Config {
        sidebar_state_file: directory.path().join("state.json"),
        status_sections: [("review".into(), "logical-review".into())].into(),
        ..Config::default()
    };
    let state = json!({"electron-persisted-atom-state":{"sidebar-custom-sections-v3":{
        "account":{"sections":[{"id":"logical-review","hostSectionIds":{
            "local":"backend-review","mac":"backend-remote"
        }}]}
    }}});
    std::fs::write(&config.sidebar_state_file, state.to_string())?;
    Ok((directory, config, state))
}

fn section() -> Value {
    json!({"id":"backend-review","name":"待审阅","appearance":null})
}

fn reader(section: Value) -> Reader {
    Reader {
        replies: [
            json!({"data":[self::section()],"nextCursor":null}),
            json!({"thread":{"id":THREAD,"section":section}}),
        ]
        .into(),
        calls: vec![],
    }
}

#[test]
fn direct_read_maps_only_the_local_backend_and_never_queries_the_aggregate() -> Result<()> {
    let (_directory, config, _) = setup()?;
    let mut source = reader(section());
    let observed = direct::read(&mut source, &config, THREAD)?;
    assert_eq!(observed.version, "logical-review");
    assert_eq!(observed.state, "review");
    assert_eq!(source.calls, ["threadSection/list", "thread/read"]);
    let mut source = reader(Value::Null);
    assert_eq!(
        direct::read(&mut source, &config, THREAD)?.version,
        "unsectioned"
    );
    let mut source = reader(json!({"id":"backend-remote","name":"待审阅"}));
    assert!(direct::read(&mut source, &config, THREAD).is_err());
    Ok(())
}

#[test]
fn direct_read_rejects_ambiguous_missing_renamed_and_wrong_host_mappings() -> Result<()> {
    let (_directory, config, original) = setup()?;
    let path = "/electron-persisted-atom-state/sidebar-custom-sections-v3";
    for variant in ["duplicate-account", "missing", "duplicate-backend"] {
        let mut state = original.clone();
        let accounts = state.pointer_mut(path).unwrap();
        match variant {
            "duplicate-account" => accounts["other"] = accounts["account"].clone(),
            "missing" => {
                accounts["account"]["sections"][0]["hostSectionIds"]["local"] = Value::Null
            }
            _ => accounts["account"]["sections"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"other","hostSectionIds":{"local":"backend-review"}})),
        }
        std::fs::write(&config.sidebar_state_file, state.to_string())?;
        let mut source = reader(section());
        assert!(
            direct::read(&mut source, &config, THREAD).is_err(),
            "{variant}"
        );
        assert_eq!(source.calls, ["threadSection/list"]);
    }
    std::fs::write(&config.sidebar_state_file, original.to_string())?;
    let mut source = reader(section());
    source.replies[0]["data"][0]["name"] = json!("renamed");
    assert!(direct::read(&mut source, &config, THREAD).is_err());
    let mut remote_config = config;
    remote_config.host_id = "mac".into();
    assert!(direct::read(&mut reader(section()), &remote_config, THREAD).is_err());
    Ok(())
}

#[test]
fn direct_snapshot_rejects_partial_pages_and_malformed_target_results() -> Result<()> {
    let (_directory, config, _) = setup()?;
    for malformed in [
        json!({}),
        json!({"id":THREAD}),
        json!({"id":"wrong","section":section()}),
    ] {
        let mut source = reader(section());
        source.replies[1] = json!({"thread":malformed});
        assert!(direct::read(&mut source, &config, THREAD).is_err());
    }
    let mut source = reader(section());
    source.replies[1] = json!({"data":[{"id":THREAD,"section":section()}]});
    let snapshot = direct::snapshot(&mut source, &config)?;
    assert_eq!(snapshot[THREAD].version, "logical-review");
    assert_eq!(source.calls, ["threadSection/list", "thread/list"]);
    let mut config = config;
    config.max_pages = 1;
    let mut source = reader(section());
    source.replies[0]["nextCursor"] = json!("unfinished");
    assert!(direct::snapshot(&mut source, &config).is_err());
    let mut source = reader(section());
    source.replies[1] = json!({"data":[],"nextCursor":"unfinished"});
    assert!(direct::snapshot(&mut source, &config).is_err());
    Ok(())
}
