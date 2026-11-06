use crate::model::STATES;
use crate::rpc::Rpc;
use crate::store::Store;
use anyhow::{Context, Result, ensure};
use serde_json::json;

pub fn projected_title(title: &str, state: &str) -> Result<String> {
    let label = STATES
        .iter()
        .find(|item| item.0 == state)
        .context("未知状态")?
        .1;
    let mut body = title;
    loop {
        let next = STATES.iter().find_map(|(_, label)| {
            [format!("[{label}]"), format!("【{label}】")]
                .iter()
                .find_map(|prefix| body.strip_prefix(prefix).map(str::trim_start))
        });
        match next {
            Some(next) => body = next,
            None => break,
        }
    }
    Ok(format!(
        "[{label}] {}",
        if body.is_empty() {
            "未命名会话"
        } else {
            body
        }
    ))
}

pub fn sync(store: &Store, writer: &mut dyn Rpc, identifier: &str) -> Result<()> {
    let jobs = store.pending_titles(Some(identifier))?;
    let Some(job) = jobs.first() else {
        return Ok(());
    };
    let version = job["version"].as_i64().context("标题任务缺少版本")?;
    let result = sync_job(
        store,
        writer,
        identifier,
        job["state"].as_str().context("未知状态")?,
        version,
    );
    if let Err(error) = result {
        store.title_failed(identifier, version, &error.to_string())?;
    }
    Ok(())
}

fn sync_job(
    store: &Store,
    writer: &mut dyn Rpc,
    identifier: &str,
    state: &str,
    version: i64,
) -> Result<()> {
    let response = writer.call(
        "thread/read",
        json!({"threadId":identifier,"includeTurns":false}),
    )?;
    let thread = &response["thread"];
    validate_title(thread, identifier)?;
    let current = thread["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            thread["preview"]
                .as_str()
                .and_then(|preview| preview.lines().next())
                .unwrap_or(identifier)
        });
    let target = projected_title(current, state)?;
    if !store.title_current(identifier, version)? {
        return Ok(());
    }
    if current != target {
        writer.call(
            "thread/name/set",
            json!({"threadId":identifier,"name":target}),
        )?;
    }
    store.title_synced(identifier, version, &target)?;
    Ok(())
}

fn validate_title(thread: &serde_json::Value, identifier: &str) -> Result<()> {
    ensure!(thread.is_object(), "原生标题响应缺少有效会话");
    ensure!(
        thread.get("id").is_none_or(|value| value == identifier),
        "原生标题响应会话 ID 不匹配"
    );
    for field in ["name", "preview"] {
        ensure!(
            thread[field].is_null() || thread[field].is_string(),
            "原生标题字段格式无效"
        );
    }
    ensure!(
        thread["name"].is_string() || thread["preview"].is_string() || thread["id"] == identifier,
        "原生标题响应缺少完整名称或会话标识"
    );
    Ok(())
}
