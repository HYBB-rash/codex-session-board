use anyhow::{Context, Result, bail, ensure};
use codex_session_board::{config::Config, model::board_states};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub struct Surface {
    root: PathBuf,
    threads: Value,
}

pub fn section(state: &str) -> String {
    let index = board_states().position(|(candidate, _)| candidate == state).unwrap_or(0);
    format!("00000000-0000-4000-8000-{:012}", index + 1)
}

fn catalog() -> Vec<Value> {
    board_states().map(|(state, label)| json!({
        "id":format!("backend:{}",section(state)),"name":label,"appearance":null
    })).collect()
}

impl Surface {
    pub fn load(root: &Path) -> Result<Self> {
        Ok(Self { root: root.into(), threads: serde_json::from_slice(
            &std::fs::read(root.join("fixture.json"))?)? })
    }

    fn rows(&self) -> Result<Vec<Value>> {
        self.threads.as_array().context("fixture 会话无效")?.iter().map(|thread| {
            let mut row = thread.clone();
            row["section"] = catalog().into_iter().find(|entry|
                entry["id"] == format!("backend:{}",section(thread["state"].as_str().unwrap_or(""))))
                .context("fixture 分区无效")?;
            Ok(row)
        }).collect()
    }

    fn mutate(&mut self, identifier: &str, field: &str, value: Value) -> Result<Value> {
        let thread = self.threads.as_array_mut().context("fixture 会话无效")?
            .iter_mut().find(|thread| thread["id"] == identifier).context("不是本任务测试会话")?;
        thread[field] = value;
        let temporary = self.root.join("fixture.pending");
        std::fs::write(&temporary, serde_json::to_vec(&self.threads)?)?;
        std::fs::rename(temporary, self.root.join("fixture.json"))?;
        Ok(json!({}))
    }

    pub fn answer(&mut self, method: &str, params: &Value) -> Result<Value> {
        let identifier = params["threadId"].as_str().unwrap_or("");
        match method {
            "initialize" => Ok(json!({})),
            "thread/list" => Ok(json!({"data":self.rows()?.into_iter().filter(|thread|
                (thread["archived"] == true) == (params["archived"] == true)).collect::<Vec<_>>(),"nextCursor":null})),
            "thread/read" => Ok(json!({"thread":self.rows()?.into_iter()
                .find(|thread| thread["id"] == identifier).context("不是本任务测试会话")?})),
            "thread/turns/list" => Ok(json!({"data":[],"nextCursor":null})),
            "threadSection/list" => Ok(json!({"data":catalog(),"nextCursor":null})),
            "thread/name/set" => self.mutate(identifier, "name", params["name"].clone()),
            "thread/archive" => self.mutate(identifier, "archived", json!(true)),
            "move_thread_to_sidebar_section" => {
                ensure!(params["hostId"] == "local" && params["source"] == "codex", "禁止外部宿主");
                let state = board_states().find(|(state, _)| params["sectionId"] == section(state))
                    .context("不是本任务测试分区")?.0;
                self.mutate(identifier, "state", json!(state))
            }
            "list_projects" => Ok(json!({"projects":[]})),
            _ => bail!("开发 fixture 不允许方法 {method}"),
        }
    }
}

pub fn prepare(root: &Path, port: u16) -> Result<()> {
    for name in ["desktop", "sidebar"] {
        std::fs::create_dir_all(root.join(name))?;
        std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o700))?;
    }
    let source = std::env::current_dir()?;
    let binary = std::env::current_exe()?;
    let command = |mode: &str| vec![binary.to_string_lossy().into_owned(), mode.into(), root.to_string_lossy().into_owned()];
    let config = Config {
        data_dir: root.join("data"), port, native_sections: true, sync_titles: false,
        codex_command: command("history"), open_command: command("open"), ssh_command: command("deny"),
        desktop_socket: root.join("desktop/rpc.sock"), sidebar_socket: root.join("sidebar/rpc.sock"),
        sidebar_state_file: root.join("projects.json"),
        sidebar_caller: "11111111-1111-4111-8111-111111111111".into(),
        status_sections: board_states().map(|(state, _)| (state.into(), section(state))).collect(),
        poll_seconds: 2, runtime_poll_seconds: 1, rpc_timeout_seconds: 3,
        ..Config::default()
    };
    config.validate()?;
    std::fs::write(root.join("config.json"), serde_json::to_vec_pretty(&config)?)?;
    let sections: Vec<_> = board_states().map(|(state, _)| json!({
        "id":section(state),"hostSectionIds":{"local":format!("backend:{}",section(state))}})).collect();
    std::fs::write(root.join("projects.json"), serde_json::to_vec(&json!({
        "local-projects":{"dev":{"id":"dev","name":"隔离开发","rootPaths":[source]}},
        "project-order":["dev"],"remote-projects":[],"codex-managed-remote-connections":[],
        "electron-persisted-atom-state":{
            "sidebar-custom-sections-v3":{"dev":{"sections":sections}}}
    }))?)?;
    if !root.join("fixture.json").exists() {
        let threads: Vec<_> = ["unknown","review","to_archive"].iter().enumerate().map(|(index,state)| json!({
            "id":format!("11111111-1111-4111-8111-{:012}",index+1),
            "name":format!("开发会话 {}",index+1),"preview":"隔离数据","cwd":source,
            "updatedAt":1,"createdAt":1,"source":"appServer","status":{"type":"idle"},
            "state":state,"archived":false
        })).collect();
        std::fs::write(root.join("fixture.json"), serde_json::to_vec(&threads)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_rejects_external_hosts_and_unknown_sessions() -> Result<()> {
        let root = tempfile::tempdir()?;
        prepare(root.path(), 65400)?;
        let mut fixture = Surface::load(root.path())?;
        assert!(fixture.answer("thread/read", &json!({"threadId":"foreign"})).is_err());
        assert!(fixture.answer("move_thread_to_sidebar_section", &json!({
            "threadId":"11111111-1111-4111-8111-000000000001","hostId":"foreign",
            "source":"codex","sectionId":section("review")})).is_err());
        assert!(fixture.answer("thread/start", &json!({})).is_err());
        Ok(())
    }

    #[test]
    fn matching_session_ids_remain_private_and_survive_prepare() -> Result<()> {
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        prepare(first.path(), 65400)?;
        prepare(second.path(), 65401)?;
        let mut fixture = Surface::load(first.path())?;
        fixture.mutate("11111111-1111-4111-8111-000000000001", "state", json!("blocked"))?;
        prepare(first.path(), 65400)?;
        assert_eq!(Surface::load(first.path())?.rows()?[0]["state"], "blocked");
        assert_eq!(Surface::load(second.path())?.rows()?[0]["state"], "unknown");
        Ok(())
    }
}
