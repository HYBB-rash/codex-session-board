use super::*;
use serde::{Deserialize, Serialize};

pub(crate) struct Seed {
    pub scope: Value,
    pub logical: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Probe {
    pub request: String,
    pub thread: Option<String>,
    #[serde(default)]
    pub prepared: bool,
    #[serde(default)]
    pub initial_name: Value,
    #[serde(default)]
    pub history_mode: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Record {
    pub scope: Value,
    pub logical: HashMap<String, String>,
    pub probe: Option<Probe>,
}

pub(crate) fn usable(
    config: &Config,
    sections: &HashMap<String, Value>,
    logical: HashMap<String, String>,
) -> HashMap<String, String> {
    let mut usable: HashMap<_, _> = logical.into_iter().filter(|(backend, logical)| {
        sections.get(backend).is_some_and(|section| {
            config.section_bindings().find(|(_, _, target)| *target == logical)
                .is_none_or(|(_, label, _)| section["name"] == label)
        })
    }).collect();
    let mut counts = HashMap::new();
    for logical in usable.values() {
        *counts.entry(logical.clone()).or_insert(0) += 1;
    }
    usable.retain(|_, logical| counts[logical.as_str()] == 1);
    usable
}

pub(crate) fn merge_saved(
    mut saved: HashMap<String, String>,
    disk: HashMap<String, String>,
) -> HashMap<String, String> {
    let conflicts: HashSet<_> = disk.iter().filter(|(backend, logical)| {
        saved.get(*backend).is_some_and(|previous| previous != *logical)
            || saved.iter().any(|(other, previous)| previous == *logical && other != *backend)
    }).flat_map(|(backend, logical)| {
        std::iter::once(logical.clone()).chain(saved.get(backend).cloned())
    }).collect();
    saved.extend(disk);
    saved.retain(|_, logical| !conflicts.contains(logical));
    saved
}

pub(crate) fn complete(config: &Config, record: &Record) -> bool {
    config.section_bindings().all(|(_, _, target)|
        record.logical.values().any(|logical| logical == target))
}

pub(crate) fn seed(config: &Config) -> Result<Seed> {
    let state: Value = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(
        &config.sidebar_state_file,
    )?))?;
    let accounts = client_atoms(&state)?["sidebar-custom-sections-v3"]
        .as_object()
        .context("客户端分区映射无效")?;
    let (account, entries) = super::direct::account_sections(config, accounts)?;
    let scope = json!({"owner":state["electron-renderer-content-owner"],
        "account":account,"host":config.host_id,"bindings":config.status_sections});
    let mut logical = HashMap::new();
    let mut identifiers = HashSet::new();
    for entry in entries {
        let identifier = entry["id"].as_str().context("逻辑分区缺少 ID")?;
        ensure!(identifiers.insert(identifier), "逻辑分区 ID 重复");
        if let Some(backend) = entry["hostSectionIds"][&config.host_id].as_str() {
            ensure!(
                logical.insert(backend.into(), identifier.into()).is_none(),
                "主机分区映射重复"
            );
        }
    }
    Ok(Seed { scope, logical })
}

pub(crate) fn live_catalog(reader: &mut (impl Rpc + ?Sized), config: &Config) -> Result<Value> {
    let response = reader.call("list_threads", json!({"limit":1}))?;
    let unavailable = response["unavailableHosts"]
        .as_array()
        .context("原生主机状态无效")?;
    ensure!(
        !unavailable
            .iter()
            .any(|host| host == &config.host_id || host["hostId"] == config.host_id),
        "原生分区主机不可用"
    );
    let entries = response["sections"]
        .as_array()
        .context("原生分区目录无效")?;
    super::validate_catalog(config, entries)?;
    let mut identifiers = HashSet::new();
    for entry in entries {
        ensure!(
            identifiers.insert(entry["sectionId"].as_str().context("逻辑分区 ID 无效")?),
            "原生逻辑分区 ID 重复"
        );
    }
    Ok(response)
}

pub(crate) fn confirm_membership(response: &Value, target: &str, thread: &str) -> Result<()> {
    let suffix = format!(":{thread}");
    let mut placements = Vec::new();
    for entry in response["sections"]
        .as_array()
        .context("原生逻辑目录无效")?
    {
        for item in entry["itemKeys"].as_array().context("原生分区成员无效")? {
            let item = item.as_str().context("原生分区成员标识无效")?;
            if item.starts_with("codex:thread:") && item.ends_with(&suffix) {
                placements.push(entry["sectionId"].as_str().context("逻辑分区 ID 无效")?);
            }
        }
    }
    ensure!(placements == [target], "校准会话的实时逻辑位置尚未确认");
    Ok(())
}
