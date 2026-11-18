use super::{App, remote::remote_identity};
use crate::rpc::{Role, Rpc, latest_turn_page};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, atomic::Ordering};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct PreviewTurns(Mutex<HashMap<String, String>>);

impl PreviewTurns {
    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, String>>> {
        self.0.lock().map_err(|_| anyhow::anyhow!("预览状态锁已损坏"))
    }

    pub(crate) fn clear(&self) -> Result<()> {
        self.lock()?.clear();
        Ok(())
    }

    pub(crate) fn remove(&self, identifier: &str) -> Result<()> {
        self.lock()?.remove(identifier);
        Ok(())
    }

    pub(crate) fn observe(&self, identifier: &str, status: &Value, turn: &Value) -> Result<()> {
        if idle(status) && turn["status"] == "completed" && turn["hasFinal"] == true
            && let Some(turn_id) = turn["id"].as_str().filter(|id| valid_turn_id(id))
        {
            self.lock()?.insert(identifier.to_owned(), turn_id.to_owned());
        }
        Ok(())
    }
}

impl App {
    fn previews_live(&self) -> Result<bool> {
        let health = self.health.lock().map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?;
        Ok(self.enabled.load(Ordering::SeqCst) && health["runtimeVerified"] == true)
    }

    pub(super) fn annotate_previews(&self, sessions: &mut [Value]) -> Result<()> {
        if !self.previews_live()? {
            return Ok(());
        }
        let turns = self.preview_turns.lock()?;
        for session in sessions {
            if session["personalDraft"] == true || session["presence"] == "cachedUnknown" {
                continue;
            }
            if let Some(turn) = session["id"].as_str().and_then(|id| turns.get(id)) {
                session["previewTurnId"] = json!(turn);
            }
        }
        Ok(())
    }

    pub(super) fn preview_reply(&self, body: &Value) -> Result<Value> {
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        let turn = body["turnId"].as_str().filter(|id| valid_turn_id(id)).context("轮次 ID 无效")?;
        let result = if identifier.starts_with("remote:") {
            let (host, thread) = remote_identity(identifier)?;
            self.remote_app(&host)?.read_preview(&thread, turn)
        } else {
            let uuid = uuid::Uuid::parse_str(identifier).context("会话 ID 无效")?;
            ensure!(uuid.to_string() == identifier, "会话 ID 无效");
            self.read_preview(identifier, turn)
        };
        result.map_err(|_| anyhow::anyhow!("本轮答复已无法预览，请打开会话核对。"))
    }

    fn read_preview(&self, thread: &str, turn: &str) -> Result<Value> {
        let _operation = self.operation()?;
        ensure!(self.store.contains(thread)?, "会话不在看板中");
        self.require_preview(thread, turn)?;
        let mut reader = self.connection(Duration::from_secs(5), Role::PreviewReader)?;
        let answer = read_reply(&mut reader, thread, turn)?;
        self.require_preview(thread, turn)?;
        Ok(json!({"turnId":turn,"text":answer}))
    }

    fn require_preview(&self, thread: &str, turn: &str) -> Result<()> {
        ensure!(self.previews_live()?, "主机执行状态不可用");
        ensure!(
            self.preview_turns.lock()?.get(thread).is_some_and(|expected| expected == turn),
            "预览轮次已改变"
        );
        Ok(())
    }
}

fn valid_turn_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}

fn idle(status: &Value) -> bool {
    matches!(status["type"].as_str(), Some("idle" | "notLoaded"))
}

fn verify_idle(reader: &mut dyn Rpc, thread: &str) -> Result<()> {
    let response = reader.call("thread/read", json!({"threadId":thread,"includeTurns":false}))?;
    let actual = &response["thread"];
    ensure!(actual["id"] == thread && idle(&actual["status"]), "会话正在执行或无法确认");
    Ok(())
}

fn verify_latest(page: &Value, turn: &str) -> Result<()> {
    let turns = page["data"].as_array().context("缺少轮次列表")?;
    ensure!(turns.len() == 1 && turns[0]["id"] == turn && turns[0]["status"] == "completed",
        "当前轮次发生变化或尚未正常完成");
    Ok(())
}

fn read_reply(reader: &mut dyn Rpc, thread: &str, turn: &str) -> Result<String> {
    verify_idle(reader, thread)?;
    let page = latest_turn_page(reader, thread, "summary")?;
    verify_latest(&page, turn)?;
    let text = page["data"][0]["items"].as_array().context("缺少轮次消息")?
        .iter().rfind(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer")
        .and_then(|item| item["text"].as_str()).filter(|text| !text.trim().is_empty())
        .context("没有可确认的最终答复")?;
    verify_latest(&latest_turn_page(reader, thread, "notLoaded")?, turn)?;
    verify_idle(reader, thread)?;
    Ok(text.to_owned())
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
