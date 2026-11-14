use crate::rpc::Rpc;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const ALL_SOURCES: [&str; 10] = [
    "cli",
    "vscode",
    "appServer",
    "exec",
    "subAgent",
    "subAgentReview",
    "subAgentCompact",
    "subAgentThreadSpawn",
    "subAgentOther",
    "unknown",
];

pub fn ordinary_params(limit: usize, archived: bool) -> Value {
    json!({"limit":limit,"sortKey":"created_at","sortDirection":"asc",
        "sourceKinds":[],"modelProviders":[],"currentAccountOnly":false,
        "archived":archived,"useStateDbOnly":true})
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageScan {
    pub method: String,
    pub params: Value,
    pub cursor: Option<String>,
    pub cursors: BTreeSet<String>,
    pub items: Vec<Value>,
    pub complete: bool,
}

impl PageScan {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            method: method.into(),
            params,
            cursor: None,
            cursors: BTreeSet::new(),
            items: Vec::new(),
            complete: false,
        }
    }

    pub fn step(&mut self, reader: &mut dyn Rpc, host: &str) -> Result<()> {
        ensure!(!self.complete, "分页已经完成");
        ensure!(
            matches!(
                self.method.as_str(),
                "project/list" | "thread/list" | "threadSection/list"
            ),
            "不允许的目录分页方法"
        );
        let mut params = self.params.clone();
        params["cursor"] = json!(self.cursor);
        let result = reader.call(&self.method, params)?;
        let page = self.page(&result, host)?;
        let next = self.successor(&result)?;
        for entry in page {
            if let Some(previous) = self.items.iter_mut().find(|old| old["id"] == entry["id"]) {
                *previous = entry;
            } else {
                self.items.push(entry);
            }
        }
        if let Some(next) = &next {
            self.cursors.insert(next.clone());
        }
        self.cursor = next;
        self.complete = self.cursor.is_none();
        Ok(())
    }

    fn page(&self, result: &Value, host: &str) -> Result<Vec<Value>> {
        let entries = result["data"].as_array().context("目录分页缺少 data")?;
        let mut page = Vec::new();
        for entry in entries {
            let identifier = entry["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .context("目录条目缺少身份")?;
            ensure!(
                entry.get("hostId").is_none_or(|value| value == host),
                "目录回包主机不匹配"
            );
            if self.method == "thread/list" {
                ensure!(
                    uuid::Uuid::parse_str(identifier)?.to_string() == identifier,
                    "目录会话 UUID 非规范格式"
                );
            }
            page.push(project_fields(entry, &self.method)?);
        }
        Ok(page)
    }

    fn successor(&self, result: &Value) -> Result<Option<String>> {
        let next = match result.get("nextCursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.is_empty() => None,
            Some(Value::String(value)) => Some(value.clone()),
            _ => anyhow::bail!("目录游标格式无效"),
        };
        if let Some(next) = &next {
            ensure!(!self.cursors.contains(next), "目录分页游标重复");
        }
        Ok(next)
    }
}

pub(crate) fn project_fields(entry: &Value, method: &str) -> Result<Value> {
    let fields: &[&str] = match method {
        "project/list" => &["id", "name", "roots", "position", "metadata"],
        "threadSection/list" => &["id", "name"],
        _ => &[
            "id",
            "name",
            "preview",
            "cwd",
            "updatedAt",
            "createdAt",
            "projectId",
            "ephemeral",
            "threadSource",
            "attachedToThreadId",
            "source",
        ],
    };
    let mut result = serde_json::Map::new();
    for field in fields {
        if let Some(value) = entry.get(*field)
            && let Some(projected) = field_value(field, value)?
        {
            result.insert((*field).into(), projected);
        }
    }
    Ok(Value::Object(result))
}

fn field_value(field: &str, value: &Value) -> Result<Option<Value>> {
    Ok(match field {
        "name" | "preview" => value.as_str().map(|text| json!(text.chars().take(300).collect::<String>())),
        "id" | "cwd" => value.as_str().map(|text| json!(text)),
        "projectId" | "attachedToThreadId" | "threadSource" => {
            ensure!(value.is_null() || value.is_string(), "目录 {field} 类型无效");
            Some(value.clone())
        }
        "ephemeral" => {
            ensure!(value.is_boolean(), "目录 ephemeral 类型无效");
            Some(value.clone())
        }
        "updatedAt" | "createdAt" | "position" => value.as_number().map(|number| json!(number)),
        "roots" => value.as_array().map(|roots| json!(roots.iter()
            .filter_map(|root| root.as_str().or_else(|| root["path"].as_str()))
            .map(|path| json!({"path":path})).collect::<Vec<_>>())),
        "metadata" => Some(value["codex.localProjectId"].as_str()
            .map_or_else(|| json!({}), |identifier| json!({"codex.localProjectId":identifier}))),
        "source" => source_projection(value),
        _ => None,
    })
}

pub(super) fn source_projection(value: &Value) -> Option<Value> {
    if let Some(source) = value.as_str() {
        return matches!(source, "cli" | "vscode" | "exec" | "mcp" | "appServer" | "unknown")
            .then(|| json!(source));
    }
    if let Some(custom) = value["custom"].as_str() {
        return matches!(custom, "atlas" | "chatgpt").then(|| json!({"custom":custom}));
    }
    value.as_object().filter(|fields| fields.contains_key("subAgent"))
        .map(|_| json!({"subAgent":null}))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pages(Vec<Value>);

    #[test]
    fn metadata_projection_never_caches_nested_payloads() -> Result<()> {
        let thread = project_fields(&json!({"id":"id","name":{"secret":"body"},
            "preview":["body"],"source":{"subAgent":{"thread_spawn":{"secret":"body"}}},
            "cwd":"/remote","projectId":null,"turns":[{"text":"body"}]}), "thread/list")?;
        assert_eq!(thread, json!({"id":"id","cwd":"/remote","projectId":null,
            "source":{"subAgent":null}}));
        let project = project_fields(&json!({"id":"project","roots":[{"path":"/remote","secret":"body"}],
            "metadata":{"codex.localProjectId":{"secret":"body"},"other":"body"}}), "project/list")?;
        assert_eq!(project, json!({"id":"project","roots":[{"path":"/remote"}],"metadata":{}}));
        assert!(project_fields(&json!({"projectId":{"secret":"body"}}), "thread/list").is_err());
        assert!(project_fields(&json!({"attachedToThreadId":false}), "thread/list").is_err());
        Ok(())
    }

    impl Rpc for Pages {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            assert_eq!(method, "thread/list");
            let expected = self.0.remove(0);
            assert_eq!(params["cursor"], expected["cursor"]);
            assert_eq!(params["sourceKinds"], json!([]));
            assert_eq!(params["modelProviders"], json!([]));
            assert_eq!(params["currentAccountOnly"], false);
            assert!(params.get("projectId").is_none());
            Ok(expected["response"].clone())
        }
    }

    #[test]
    fn budgeted_pages_resume_without_restarting_or_claiming_completeness() -> Result<()> {
        let first = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        let mut reader = Pages(vec![
            json!({"cursor":null,"response":{"data":[{"id":first,"projectId":null}],"nextCursor":"second"}}),
            json!({"cursor":"second","response":{"data":[{"id":second}],"nextCursor":null}}),
        ]);
        let mut scan = PageScan::new("thread/list", ordinary_params(1, true));
        scan.step(&mut reader, "remote:a")?;
        assert!(!scan.complete);
        let saved = serde_json::to_value(&scan)?;
        let mut scan: PageScan = serde_json::from_value(saved)?;
        scan.step(&mut reader, "remote:a")?;
        assert!(scan.complete);
        assert_eq!(scan.items.len(), 2);
        assert!(scan.items[0].get("projectId").is_some());
        assert!(scan.items[1].get("projectId").is_none());
        Ok(())
    }

    #[test]
    fn invalid_successor_and_foreign_host_do_not_publish_partial_pages() -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut scan = PageScan::new("thread/list", ordinary_params(1, false));
        let mut reader = Pages(vec![
            json!({"cursor":null,"response":{"data":[{"id":id}],"nextCursor":"same"}}),
            json!({"cursor":"same","response":{"data":[{"id":id}],"nextCursor":"same"}}),
        ]);
        scan.step(&mut reader, "remote:a")?;
        let saved = serde_json::to_value(&scan)?;
        assert!(scan.step(&mut reader, "remote:a").is_err());
        assert_eq!(serde_json::to_value(&scan)?, saved);
        let mut reader = Pages(vec![json!({"cursor":"same","response":{"data":[
            {"id":id,"hostId":"remote:b"}]}})]);
        assert!(scan.step(&mut reader, "remote:a").is_err());
        assert_eq!(serde_json::to_value(&scan)?, saved);
        Ok(())
    }
}
