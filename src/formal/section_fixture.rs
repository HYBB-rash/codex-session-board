use super::section_model::{Configuration, KEYS, STATES};
use crate::{config::Config, rpc::Rpc};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{path::Path, sync::{Arc, Mutex}};

pub const THREAD: &str = "11111111-1111-4111-8111-111111111111";

pub fn identifier(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012}")
}

pub fn configuration(root: &Path, model: Configuration) -> Config {
    Config {
        data_dir: root.join("data"),
        native_sections: true,
        sidebar_state_file: root.join("state.json"),
        sidebar_socket: root.join("sidebar.sock"),
        sidebar_caller: THREAD.into(),
        status_sections: KEYS.into_iter().enumerate()
            .filter(|(index, _)| model.0 & (1 << index) != 0)
            .map(|(index, key)| (key.into(), identifier(index))).collect(),
        sync_titles: false,
        ..Config::default()
    }
}

pub fn registry(memberships: [u8; 2], retired: u8) -> Value {
    let accounts: serde_json::Map<String, Value> = memberships.into_iter().enumerate()
        .map(|(account, mask)| {
            let sections: Vec<_> = (0..6)
                .filter(|index| if *index == 5 { retired & (1 << account) != 0 }
                    else { mask & (1 << index) != 0 })
                .map(|index| json!({"id":identifier(index),
                    "hostSectionIds":{"local":format!("backend-{index}")}})).collect();
            (format!("account-{account}"), json!({"sections":sections}))
        }).collect();
    json!({"electron-persisted-atom-state":{"sidebar-custom-sections-v3":accounts}})
}

pub fn catalog(config: &Config) -> Vec<Value> {
    STATES.into_iter().map(|(state, label)| {
        let logical = &config.status_sections[state];
        json!({"id":format!("backend-{}",logical.trim_start_matches("00000000-0000-4000-8000-").parse::<usize>().unwrap()),
            "name":label})
    }).collect()
}

pub struct World {
    pub sections: Vec<Value>,
    pub current: usize,
    pub writes: Vec<Value>,
}

pub struct Reader(pub Arc<Mutex<World>>);
pub struct Writer(pub Arc<Mutex<World>>);

impl Rpc for Reader {
    fn call(&mut self, method: &str, _: Value) -> Result<Value> {
        let world = self.0.lock().unwrap();
        match method {
            "threadSection/list" => Ok(json!({"data":world.sections,"nextCursor":null})),
            "thread/read" => Ok(json!({"thread":{"id":THREAD,"section":world.sections[world.current]}})),
            _ => bail!("只读模型端点收到非读取方法：{method}"),
        }
    }
}

impl Rpc for Writer {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let mut world = self.0.lock().unwrap();
        if method != "move_thread_to_sidebar_section" {
            bail!("模型写入方法无效");
        }
        let logical = params["sectionId"].as_str().unwrap();
        let backend = format!("backend-{}", logical.rsplit('-').next().unwrap().parse::<usize>()?);
        world.current = world.sections.iter().position(|section| section["id"] == backend).unwrap();
        world.writes.push(params);
        Ok(json!({}))
    }
}
