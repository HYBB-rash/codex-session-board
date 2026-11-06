use crate::{
    app::App,
    rpc::{DesktopConnection, Role, Rpc, summarize_turn},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub fn request(app: &App, body: &Value) -> Result<Value> {
    let identifier = body["id"].as_str().context("缺少会话 ID")?;
    let version = body["version"].as_i64().context("缺少状态版本")?;
    let input = body["inputVersion"].as_i64().context("缺少输入版本")?;
    uuid::Uuid::parse_str(identifier).context("无效会话 ID")?;
    let _titles = app
        .title_lock
        .try_lock()
        .map_err(|_| anyhow::anyhow!("标题同步进行中，请稍后重试归档"))?;
    let _sections = app
        .section_lock
        .try_lock()
        .map_err(|_| anyhow::anyhow!("分区同步进行中，请稍后重试归档"))?;
    let result = app.store.archive(identifier, version, input, |expected| {
        let timeout = Duration::from_secs(app.config.rpc_timeout_seconds.min(3));
        let mut writer =
            DesktopConnection::connect(&app.config.desktop_socket, timeout, Role::ArchiveWriter)?;
        archive(&mut writer, identifier, app.config.max_pages, expected)
    });
    app.refresh.store(true, Ordering::Relaxed);
    result?;
    Ok(json!({"ok":true,"archived":true}))
}

pub fn archive(
    writer: &mut dyn Rpc,
    identifier: &str,
    max_pages: usize,
    expected: &Value,
) -> Result<()> {
    if archived(writer, identifier, max_pages)? {
        return Ok(());
    }
    let response = writer.call(
        "thread/read",
        json!({"threadId":identifier,"includeTurns":false}),
    )?;
    let thread = response.get("thread").context("归档前读取会话失败")?;
    ensure!(thread["id"] == identifier, "归档前读回的会话 ID 不匹配");
    ensure!(
        matches!(
            thread["status"]["type"].as_str(),
            Some("idle" | "notLoaded")
        ),
        "会话仍在执行或状态不明确，请刷新后重试"
    );
    verify_input(writer, identifier, expected)?;
    let response = writer
        .call("thread/archive", json!({"threadId":identifier}))
        .context("归档结果待确认，请核对原会话后重试")?;
    ensure!(response.is_object(), "归档结果待确认，回执格式无效");
    Ok(())
}

fn verify_input(writer: &mut dyn Rpc, identifier: &str, expected: &Value) -> Result<()> {
    let result = writer.call(
        "thread/turns/list",
        json!({
            "threadId":identifier,"limit":1,"sortDirection":"desc","itemsView":"summary"
        }),
    )?;
    let turns = result["data"].as_array().context("无法核对原生最新输入")?;
    if let Some(turn) = turns.first() {
        ensure!(
            turn["id"]
                .as_str()
                .is_some_and(|identifier| !identifier.is_empty())
                && input_items_known(turn),
            "无法核对原生最新输入"
        );
        ensure!(
            matches!(
                turn["status"].as_str(),
                Some("completed" | "failed" | "interrupted")
            ),
            "原生轮次仍在执行或状态不明确，请刷新后重试"
        );
    }
    let latest = turns.first().map(summarize_turn).unwrap_or(json!({}));
    let users = |turn: &Value| turn["userItems"].as_array().cloned().unwrap_or_default();
    ensure!(
        latest["id"] == expected["id"] && users(&latest) == users(expected),
        "原生会话有新输入或轮次尚未核对，请刷新并重新确认待归档状态"
    );
    Ok(())
}

fn input_items_known(turn: &Value) -> bool {
    turn["items"].as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item["type"].is_string()
                && (item["type"] != "userMessage"
                    || item["id"]
                        .as_str()
                        .is_some_and(|identifier| !identifier.is_empty()))
        })
    })
}

fn archived(writer: &mut dyn Rpc, identifier: &str, max_pages: usize) -> Result<bool> {
    let mut cursor = Value::Null;
    let mut cursors = HashSet::new();
    for _ in 0..max_pages {
        let result = writer.call(
            "thread/list",
            json!({
                "archived":true,"useStateDbOnly":true,"limit":100,"cursor":cursor,
                "sortKey":"updated_at","sortDirection":"desc","modelProviders":[],
                "sourceKinds":["cli","vscode","appServer"]
            }),
        )?;
        let threads = result["data"].as_array().context("归档列表格式无效")?;
        if threads.iter().any(|thread| thread["id"] == identifier) {
            return Ok(true);
        }
        cursor = result.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() || cursor == "" {
            return Ok(false);
        }
        let next = cursor.as_str().context("归档分页游标无效")?;
        ensure!(cursors.insert(next.to_string()), "归档分页游标重复");
    }
    anyhow::bail!("归档列表读取未完成，请调整扫描上限后重试")
}
