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
            page.push(project_fields(entry, &self.method));
        }
        let next = match result.get("nextCursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.is_empty() => None,
            Some(Value::String(value)) => Some(value.clone()),
            _ => anyhow::bail!("目录游标格式无效"),
        };
        if let Some(next) = &next {
            ensure!(!self.cursors.contains(next), "目录分页游标重复");
        }
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
}

fn project_fields(entry: &Value, method: &str) -> Value {
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
        if let Some(value) = entry.get(*field) {
            let value = if matches!(*field, "preview" | "name") {
                value
                    .as_str()
                    .map(|text| json!(text.chars().take(300).collect::<String>()))
                    .unwrap_or_else(|| value.clone())
            } else if *field == "metadata" {
                json!({"localProjectId":value.get("localProjectId")})
            } else {
                value.clone()
            };
            result.insert((*field).into(), value);
        }
    }
    Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pages(Vec<Value>);

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
