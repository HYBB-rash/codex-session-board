use super::{App, remote::remote_identity};
use crate::rpc::{Role, Rpc, latest_turn_page};
use anyhow::{Context, Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
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
        } else {
            self.remove(identifier)?;
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
        let result = if identifier.starts_with("remote:") {
            let (host, thread) = remote_identity(identifier)?;
            self.remote_app(&host)?.read_preview(&thread)
        } else {
            let uuid = uuid::Uuid::parse_str(identifier).context("会话 ID 无效")?;
            ensure!(uuid.to_string() == identifier, "会话 ID 无效");
            self.read_preview(identifier)
        };
        result.context("读取回复失败")
    }

    fn read_preview(&self, thread: &str) -> Result<Value> {
        let _operation = self.operation()?;
        ensure!(self.store.contains(thread)?, "会话不在看板中");
        let mut reader = self.connection(Duration::from_secs(5), Role::PreviewReader)?;
        let answer = read_reply(&mut reader, thread)?;
        ensure!(self.enabled.load(Ordering::SeqCst), "主机登记已失效");
        Ok(answer)
    }
}

fn valid_turn_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}

fn idle(status: &Value) -> bool {
    matches!(status["type"].as_str(), Some("idle" | "notLoaded"))
}

fn thread_running(reader: &mut dyn Rpc, thread: &str) -> Result<bool> {
    let response = reader.call("thread/read", json!({"threadId":thread,"includeTurns":false}))?;
    let actual = &response["thread"];
    ensure!(actual["id"] == thread, "会话身份不匹配");
    match actual["status"]["type"].as_str() {
        Some("active") => Ok(true),
        Some("idle" | "notLoaded") => Ok(false),
        _ => anyhow::bail!("无法核对当前轮次状态"),
    }
}

fn single_turn(page: &Value) -> Result<Option<(&Value, &str, &str)>> {
    let turns = page["data"].as_array().context("缺少轮次列表")?;
    ensure!(turns.len() <= 1, "答复分页与请求数量不匹配");
    let Some(turn) = turns.first() else {
        ensure!(page["nextCursor"].is_null(), "空页包含后续游标");
        return Ok(None);
    };
    let id = turn["id"].as_str().filter(|id| valid_turn_id(id)).context("轮次 ID 无效")?;
    let status = turn["status"].as_str().context("轮次状态缺失")?;
    ensure!(matches!(status, "completed" | "inProgress" | "interrupted" | "failed"), "未知轮次状态");
    Ok(Some((turn, id, status)))
}

fn latest_reply(reader: &mut dyn Rpc, thread: &str, initial: &Value) -> Result<Option<(String, String)>> {
    let mut visited = std::collections::HashSet::new();
    let mut next_page;
    let mut page = initial;
    while let Some((turn, id, status)) = single_turn(page)? {
        if status == "completed" {
            let items = turn["items"].as_array().context("缺少轮次消息")?;
            let last = items.iter().rev().filter(|item| {
                item["type"] == "agentMessage" && item["phase"] == "final_answer"
            }).filter_map(|item| item["text"].as_str()).find(|text| !text.trim().is_empty());
            if let Some(text) = last {
                return Ok(Some((id.to_owned(), render_markdown(text))));
            }
        }
        if page["nextCursor"].is_null() { break; }
        let cursor = page["nextCursor"].as_str().context("历史游标无效")?;
        ensure!(visited.insert(cursor.to_owned()), "历史游标重复");
        next_page = reader.call("thread/turns/list", json!({
            "threadId":thread,"limit":1,"sortDirection":"desc","itemsView":"summary","cursor":cursor
        }))?;
        page = &next_page;
    }
    Ok(None)
}

fn read_reply(reader: &mut dyn Rpc, thread: &str) -> Result<Value> {
    for _ in 0..2 {
        let running = thread_running(reader, thread)?;
        let first = latest_turn_page(reader, thread, "summary")?;
        let revision = single_turn(&first)?.map(|(_, id, status)| (id, status));
        let answer = latest_reply(reader, thread, &first)?;
        let confirmation = latest_turn_page(reader, thread, "notLoaded")?;
        let verified = single_turn(&confirmation)?.map(|(_, id, status)| (id, status));
        if running == thread_running(reader, thread)? && revision == verified {
            let in_progress = running || revision.is_some_and(|(_, status)| status == "inProgress");
            let (turn, rendered) = answer.unzip();
            return Ok(json!({"turnId":turn,"html":rendered,"inProgress":in_progress}));
        }
    }
    anyhow::bail!("读取期间轮次发生变化，请重试")
}

fn render_markdown(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let events = Parser::new_ext(markdown, options).filter_map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Some(Event::Text(text)),
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            let safe = url::Url::parse(&dest_url).ok().filter(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            });
            Some(if let Some(url) = safe {
                Event::Start(Tag::Link { link_type, dest_url: url.to_string().into(), title, id })
            } else {
                Event::Html("<a>".into())
            })
        }
        event => Some(event),
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, events);
    rendered
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
