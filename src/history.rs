use crate::common::canonical_id;
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub const HISTORY_CONTRACT: &str = "per_task_history_page_v1";
pub const ARTICLE_CONTRACT: &str = "per_round_article_v2";

fn identifier(value: &Value, field: &str) -> Result<String> {
    canonical_id(
        value[field]
            .as_str()
            .with_context(|| format!("Missing {field}"))?,
    )
}

pub fn history_prompt(pid: &str, record: &Value) -> Result<String> {
    let owner = canonical_id(pid)?;
    let contract = record["history_contract"]
        .as_str()
        .unwrap_or(HISTORY_CONTRACT);
    ensure!(
        [HISTORY_CONTRACT, ARTICLE_CONTRACT].contains(&contract),
        "Unknown history delivery contract"
    );
    let destination = if let Some(bound) = record["history_page_id"].as_str() {
        format!("已绑定记录页 page_id={}，必须复用。", canonical_id(bound)?)
    } else {
        "尚未绑定记录页：先查本任务已有链接及子页面，确认不存在才创建。".to_string()
    };
    let article_rules = if contract == ARTICLE_CONTRACT {
        let number = record["round"]
            .as_u64()
            .context("Invalid execution round")?;
        let bound = record["round_article_page_ids"][number.to_string()]
            .as_str()
            .map(|value| {
                canonical_id(value)
                    .map(|value| format!("本轮已绑定文章 page_id={value}，必须复用。"))
            })
            .transpose()?
            .unwrap_or_default();
        format!(
            "每个任务独占一个固定记录页作为索引，历轮复用；每轮独立文章保存该轮完整报告、证据、已消费指令及续接阶段记录。\
             禁止以所有轮次全文合并在记录页代替每轮独立文章。任务主页按轮次保留可展开标题、执行人、日期、简短摘要及对应文章的原生链接。\n\
             文章必须是绑定记录页的子页面，含任务回链、记录页回链、文章归属标记 notion-program/article/{owner}/round-{number:03} \
             及派发标记 notion-program/{owner}/round-{number:03}。同轮续接复用同一文章，保留旧阶段；不同轮次使用不同文章。\
             先查本轮收据、链接及子页面，多个候选、归属冲突、读取不完整或创建结果不确定时先核查，无法确认则 blocked。{bound}\n\
             notion_delivery 还须包含 article 对象：\
             {{\"page_id\":\"实际本轮文章UUID\",\"task_page_id\":\"{owner}\",\"history_page_id\":\"实际记录页UUID\",\"round\":{number},\
             \"link_verified\":true,\"report_verified\":true,\"instructions_verified\":true,\"task_summary_verified\":true}}。\
             history.report_verified 表示核验索引指向的本轮完整文章，不在索引重复全文。reference 同时注明文章 URL。\n"
        )
    } else {
        "每个任务独占一个轮次记录页，历轮复用；完整报告、证据和已消费指令写入记录页，任务主页只保留摘要与原生跳转链接。\n".to_string()
    };
    let templates = format!(
        "\n任务页模板：\n{}\n索引模板：\n{}\n轮次文章模板：\n{}",
        include_str!("../execution-templates/task-page.md"),
        include_str!("../execution-templates/history-page.md"),
        include_str!("../execution-templates/round-article.md")
    );
    Ok(format!(
        "轮次记录契约：{contract}。{article_rules}\
         禁止将完整轮次正文继续写入任务主页，禁止多个任务共用记录页。\n\
         {destination}所属任务必须为 {owner}；记录页应是本任务的子页面，包含返回本任务的链接及归属标记 notion-program/history/{owner}。\
         标题相同不足以证明归属；先 fetch 核验父页、归属标记和回链，再写本轮。绑定页不一致、无法读取或疑似串页时 blocked，不另建替代页。\n\
         参考 execution-templates/ 中的正文模板，替换全部占位 ID；默认模板不预填具体任务的记录页。保留任务要求、支持文件、审阅区和按钮。\n\
         创建或写入结果不确定时先核查回执、父页子页面和唯一标记，不盲目重试。按派发标记定位，续接编辑同轮，不重复添加。\
         旧历史先完整备份，复制后逐段核验内容、顺序和双向链接，确认完整才删除原位置；截断、未知或无法验证的历史保留原位并说明。\n\
         审阅区处理：开始时完整读取并保存本轮实际使用的五项审阅结论、决定、下一轮要求、完成标准、执行指令的原文和上下文；空白模板不是指令。\
         只归档实际消费的原文，不归档或清空未消费的新内容。先重新读取归档正文，核验父页、归属标记、回链、轮次及每项原文完整且顺序一致；\
         读取截断、未知内容可能影响审阅区、写入或核验失败时不得清空，记录原因并 blocked。\
         清空前再次读取审阅区，逐项比较当前值与已核验原文；变化或新增内容保留，不使用整页替换。\
         仅以精确旧文本定向替换清空完全相同且已存档的填写内容，保留模板和按钮；不以未证明原子条件写入或并发排除机制为阻塞条件。\
         如实报告读写之间仍可能有并发变化。清空结果不确定先读回，不盲目重试；清空后读回核验存档、模板及新指令。空白审阅区无需清空。\n\
         提交前由执行会话读回核验记录页、本轮文章全文及主页链接，确认主页没有新增完整报告；调度器仅验证收据身份及声明，不读正文。\n\
         notion_delivery 内必须包含 history 对象：\
         {{\"page_id\":\"实际记录页UUID\",\"task_page_id\":\"{owner}\",\"link_verified\":true,\"report_verified\":true,\"task_summary_only\":true}}。\
         未实际核验不得填写 true。reference 注明记录页 URL 及本轮位置。\n{templates}"
    ))
}

pub fn validate_history_delivery(
    pid: &str,
    record: &Value,
    delivery: &Value,
    state: &Value,
) -> Result<Option<String>> {
    let contract = record["history_contract"].as_str();
    ensure!(
        contract.is_none() || matches!(contract, Some(HISTORY_CONTRACT | ARTICLE_CONTRACT)),
        "Unknown history delivery contract"
    );
    if contract.is_none() && delivery["history"].is_null() && delivery["article"].is_null() {
        return Ok(None);
    }
    let history = &delivery["history"];
    ensure!(
        history.is_object(),
        "Missing independent history delivery attestation"
    );
    let owner = canonical_id(pid)?;
    ensure!(
        identifier(history, "task_page_id")? == owner,
        "History belongs to another task"
    );
    let target = identifier(history, "page_id")?;
    ensure!(target != owner, "History page must be separate from task");
    for field in ["link_verified", "report_verified", "task_summary_only"] {
        ensure!(
            history[field] == true,
            "History delivery not verified: {field}"
        );
    }
    if let Some(expected) = record["history_page_id"].as_str() {
        ensure!(
            target == canonical_id(expected)?,
            "Bound history page changed"
        );
    }
    for (other_pid, other) in state.as_object().context("State must be an object")? {
        let other_owner = canonical_id(other_pid)?;
        if other_owner != owner {
            ensure!(other_owner != target, "History target is another task page");
            if let Some(bound) = other["history_page_id"].as_str() {
                ensure!(
                    canonical_id(bound)? != target,
                    "History page belongs to another task"
                );
            }
        }
        if let Some(articles) = other["round_article_page_ids"].as_object() {
            for article in articles.values() {
                ensure!(
                    canonical_id(article.as_str().context("Invalid bound article")?)? != target,
                    "History target is an existing round article"
                );
            }
        }
    }
    if contract == Some(ARTICLE_CONTRACT) || !delivery["article"].is_null() {
        validate_article_delivery(&owner, record, delivery, state, &target)?;
    }
    Ok(Some(target))
}

pub fn validate_article_delivery(
    owner: &str,
    record: &Value,
    delivery: &Value,
    state: &Value,
    history_id: &str,
) -> Result<String> {
    let article = &delivery["article"];
    ensure!(
        article.is_object(),
        "Missing independent round article delivery attestation"
    );
    let number = record["round"]
        .as_u64()
        .filter(|number| *number > 0)
        .context("Invalid execution round")?;
    ensure!(
        identifier(article, "task_page_id")? == owner,
        "Article belongs to another task"
    );
    ensure!(
        identifier(article, "history_page_id")? == history_id,
        "Article belongs to another history"
    );
    ensure!(
        article["round"].as_u64() == Some(number),
        "Article belongs to another round"
    );
    let target = identifier(article, "page_id")?;
    ensure!(
        target != owner && target != history_id,
        "Article must be a separate page"
    );
    for field in [
        "link_verified",
        "report_verified",
        "instructions_verified",
        "task_summary_verified",
    ] {
        ensure!(
            article[field] == true,
            "Article delivery not verified: {field}"
        );
    }
    let round_key = number.to_string();
    if let Some(expected) = record["round_article_page_ids"][&round_key].as_str() {
        ensure!(
            canonical_id(expected)? == target,
            "Bound round article changed"
        );
    }
    let mut records = state.as_object().context("Invalid state")?.clone();
    records.insert(owner.to_string(), record.clone());
    for (other_pid, other) in records {
        let other_owner = canonical_id(&other_pid)?;
        ensure!(other_owner != target, "Article target is another task page");
        if let Some(bound) = other["history_page_id"].as_str() {
            ensure!(
                canonical_id(bound)? != target,
                "Article target is a bound history"
            );
        }
        if let Some(articles) = other["round_article_page_ids"].as_object() {
            for (other_round, other_article) in articles {
                if canonical_id(other_article.as_str().context("Invalid bound article")?)? == target
                {
                    ensure!(
                        other_owner == owner && other_round == &round_key,
                        "Article already belongs to another task or round"
                    );
                }
            }
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const OWNER: &str = "11111111-1111-4111-8111-111111111111";
    const HISTORY: &str = "22222222-2222-4222-8222-222222222222";
    const ARTICLE: &str = "33333333-3333-4333-8333-333333333333";

    fn delivery() -> Value {
        json!({
            "history": {"page_id": HISTORY, "task_page_id": OWNER,
                "link_verified": true, "report_verified": true, "task_summary_only": true},
            "article": {"page_id": ARTICLE, "task_page_id": OWNER, "history_page_id": HISTORY,
                "round": 2, "link_verified": true, "report_verified": true,
                "instructions_verified": true, "task_summary_verified": true}
        })
    }

    #[test]
    fn binds_distinct_round_articles() -> Result<()> {
        let record = json!({"history_contract": ARTICLE_CONTRACT, "round": 2});
        assert_eq!(
            validate_history_delivery(OWNER, &record, &delivery(), &json!({}))?,
            Some(HISTORY.to_string())
        );
        for field in [
            "link_verified",
            "report_verified",
            "instructions_verified",
            "task_summary_verified",
        ] {
            let mut value = delivery();
            value["article"][field] = json!(false);
            assert!(validate_history_delivery(OWNER, &record, &value, &json!({})).is_err());
        }
        Ok(())
    }

    #[test]
    fn rejects_route_collision_and_wrong_round() {
        let record = json!({"history_contract": ARTICLE_CONTRACT, "round": 2});
        let state = json!({"44444444-4444-4444-8444-444444444444": {"round_article_page_ids": {"1": ARTICLE}}});
        assert!(validate_history_delivery(OWNER, &record, &delivery(), &state).is_err());
        let mut value = delivery();
        value["article"]["round"] = json!(3);
        assert!(validate_history_delivery(OWNER, &record, &value, &json!({})).is_err());
    }

    #[test]
    fn legacy_receipts_keep_original_contract() -> Result<()> {
        assert_eq!(
            validate_history_delivery(OWNER, &json!({}), &json!({}), &json!({}))?,
            None
        );
        assert!(
            validate_history_delivery(
                OWNER,
                &json!({"history_contract": HISTORY_CONTRACT}),
                &json!({}),
                &json!({})
            )
            .is_err()
        );
        Ok(())
    }
}
