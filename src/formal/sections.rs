use super::{section_fixture as fixture, section_model::{self as model, Configuration}};
use crate::{app::App, config, mcp, native_sections::direct};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub fn configurations() -> Result<usize> {
    let directory = tempfile::tempdir()?;
    let mut edges = 0;
    for mask in 0..128 {
        let reference = Configuration(mask);
        let actual = fixture::configuration(directory.path(), reference);
        ensure!(actual.validate().is_ok() == reference.valid(), "配置集合 {mask}");
        for index in 0..7 {
            let mut changed = actual.clone();
            let key = model::KEYS[index];
            if changed.status_sections.remove(key).is_none() {
                changed.status_sections.insert(key.into(), fixture::identifier(index));
            }
            ensure!(changed.validate().is_ok() == reference.edit(index).valid(), "编辑 {mask}/{index}");
            edges += 1;
        }
        migration(&actual, reference, directory.path())?;
        edges += 1;
        let path = directory.path().join("source.json");
        ensure!(config::load(Some(&path)).is_ok() == reference.valid(), "重读 {mask}");
        edges += 1;
    }
    Ok(edges)
}

fn migration(actual: &config::Config, reference: Configuration, root: &std::path::Path) -> Result<()> {
    let source = root.join("source.json");
    let target = root.join(format!("target-{}.json", reference.0));
    let mut before = serde_json::to_value(actual)?;
    before["extension"] = json!({"untouched":null});
    std::fs::write(&source, before.to_string())?;
    let bytes = std::fs::read(&source)?;
    let outcome = config::migrate_sections(&source, &target);
    ensure!(outcome.is_ok() == reference.migrated().valid(), "迁移结果 {}", reference.0);
    ensure!(std::fs::read(&source)? == bytes, "迁移修改了源配置");
    if reference.migrated().valid() {
        before["status_sections"].as_object_mut().unwrap().remove("completed");
        let published: Value = serde_json::from_slice(&std::fs::read(&target)?)?;
        ensure!(published == before, "迁移改变了非退役字段");
        ensure!(config::migrate_sections(&source, &target).is_err(), "覆盖既有目标");
        ensure!(serde_json::from_slice::<Value>(&std::fs::read(&target)?)? == before, "拒绝后目标变化");
    } else {
        ensure!(!target.exists(), "拒绝迁移仍发布了文件");
    }
    ensure!(!actual.data_dir.exists(), "迁移触碰了运行数据");
    Ok(())
}

pub fn accounts() -> Result<usize> {
    let directory = tempfile::tempdir()?;
    let config = fixture::configuration(directory.path(), Configuration(31));
    let world = Arc::new(Mutex::new(fixture::World {
        sections: fixture::catalog(&config), current: 0, writes: vec![],
    }));
    let mut cases = 0;
    for first in 0..32 {
        for second in 0..32 {
            for retired in 0..4 {
                std::fs::write(&config.sidebar_state_file, fixture::registry([first, second], retired).to_string())?;
                let outcome = direct::Catalog::load(&mut fixture::Reader(Arc::clone(&world)), &config);
                ensure!(outcome.is_ok() == model::account([first, second]).is_some(),
                    "账号 {first}/{second} 退役分区 {retired}");
                if outcome.is_err() {
                    let rejected = direct::move_checked(
                        &mut fixture::Reader(Arc::clone(&world)), &mut fixture::Writer(Arc::clone(&world)),
                        &config, fixture::THREAD, &fixture::identifier(0), Some("review")
                    );
                    ensure!(rejected.is_err() && world.lock().unwrap().writes.is_empty(),
                        "无法确定账号仍发送了移动");
                }
                cases += 1;
            }
        }
    }
    ensure!(world.lock().unwrap().writes.is_empty(), "读取发出了写入");
    Ok(cases)
}

pub fn permutations() -> Result<usize> {
    let directory = tempfile::tempdir()?;
    let mut cases = 0;
    for order in model::permutations() {
        let mut config = fixture::configuration(directory.path(), Configuration(31));
        for (index, (state, _)) in model::STATES.iter().enumerate() {
            config.status_sections.insert((*state).into(), fixture::identifier(order[index]));
        }
        config.validate()?;
        ensure!(config.section_target("completed")? == config.status_sections["to_archive"],
            "旧输入别名引入了第六个原生目标");
        std::fs::write(&config.sidebar_state_file, fixture::registry([31, 0], 0).to_string())?;
        let bindings: Vec<_> = config.section_bindings().map(|(state, label, id)| (state, label, id.to_string())).collect();
        let expected: Vec<_> = model::STATES.iter().enumerate()
            .map(|(index, (state, label))| (*state, *label, fixture::identifier(order[index]))).collect();
        ensure!(bindings == expected, "完整绑定序列与独立契约不同");
        for index in 0..model::STATES.len() {
            move_target(&config, index)?;
            cases += 1;
        }
    }
    Ok(cases)
}

fn move_target(config: &config::Config, target: usize) -> Result<()> {
    let current = (target + 1) % 5;
    let world = Arc::new(Mutex::new(fixture::World {
        sections: fixture::catalog(config), current, writes: vec![],
    }));
    let state = model::STATES[target].0;
    let expected = &config.status_sections[model::STATES[current].0];
    let mut reader = fixture::Reader(Arc::clone(&world));
    let mut writer = fixture::Writer(Arc::clone(&world));
    ensure!(direct::move_checked(&mut reader, &mut writer, config, fixture::THREAD, "stale", Some(state)).is_err(),
        "接受了过期位置");
    ensure!(world.lock().unwrap().writes.is_empty(), "过期位置仍触发移动");
    let after = direct::move_checked(&mut reader, &mut writer, config, fixture::THREAD, expected, Some(state))?;
    ensure!(after.state == state && after.version == config.status_sections[state], "移动读回错误");
    let world = world.lock().unwrap();
    ensure!(world.writes == [json!({"threadId":fixture::THREAD,"sectionId":config.status_sections[state],
        "source":"codex","hostId":"local"})], "移动使用了错误的逻辑 ID 或主机");
    Ok(())
}

pub fn surfaces() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = fixture::configuration(directory.path(), Configuration(31));
    let app = App::new(config)?;
    let snapshot = app.snapshot()?;
    let actual: Vec<_> = snapshot["states"].as_object().unwrap().iter()
        .map(|(key, value)| (key.as_str(), value.as_str().unwrap())).collect();
    ensure!(actual == model::STATES, "页面状态投影与独立契约不同");
    let tools = mcp::tools_for(false);
    for name in ["board_sort", "board_update"] {
        let tool = tools["tools"].as_array().unwrap().iter().find(|tool| tool["name"] == name).unwrap();
        let mut expected: Vec<_> = model::STATES.iter().map(|item| json!(item.0)).collect();
        if name == "board_update" { expected.push(Value::Null); }
        ensure!(tool["inputSchema"]["properties"]["state"]["enum"] == json!(expected), "工具状态投影 {name}");
    }
    ensure!(app.store.column_sorts()?.as_object().unwrap().len() == 5, "排序集合未联动");
    Ok(())
}

pub fn faults() -> Result<usize> {
    let directory = tempfile::tempdir()?;
    let original = fixture::configuration(directory.path(), Configuration(31));
    let mut cases = 0;
    for first in 0..5 {
        let state = model::STATES[first].0;
        let mut invalid = original.clone();
        invalid.status_sections.insert(state.into(), "invalid".into());
        ensure!(invalid.validate().is_err(), "非法 UUID 被接受");
        cases += 1;
        for second in first + 1..5 {
            let mut duplicate = original.clone();
            duplicate.status_sections.insert(state.into(), fixture::identifier(second));
            ensure!(duplicate.validate().is_err(), "重复 UUID 被接受");
            cases += 1;
        }
        for fault in ["missing", "renamed", "wrong-host"] {
            broken_catalog(&original, first, fault)?;
            cases += 1;
        }
    }
    Ok(cases)
}

fn broken_catalog(config: &config::Config, target: usize, fault: &str) -> Result<()> {
    let mut registry = fixture::registry([31, 0], 0);
    let mut sections = fixture::catalog(config);
    match fault {
        "missing" => { sections.remove(target); }
        "renamed" => { sections[target]["name"] = json!("其他分区"); }
        _ => {
            registry["electron-persisted-atom-state"]["sidebar-custom-sections-v3"]
                ["account-0"]["sections"][target]["hostSectionIds"]["local"] = Value::Null;
        }
    }
    std::fs::write(&config.sidebar_state_file, registry.to_string())?;
    let world = Arc::new(Mutex::new(fixture::World { sections, current: 0, writes: vec![] }));
    ensure!(direct::Catalog::load(&mut fixture::Reader(Arc::clone(&world)), config).is_err(),
        "目录故障未被目录验证拒绝");
    let rejected = direct::move_checked(
        &mut fixture::Reader(Arc::clone(&world)), &mut fixture::Writer(Arc::clone(&world)),
        config, fixture::THREAD, &fixture::identifier(0), Some("review")
    );
    ensure!(rejected.is_err() && world.lock().unwrap().writes.is_empty(), "目录故障仍发送移动");
    Ok(())
}
