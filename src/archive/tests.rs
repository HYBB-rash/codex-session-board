use super::*;
use crate::clients::Notion;
use crate::common::{digest, read_json, write_json};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Default)]
struct FakeNotion {
    pages: BTreeMap<String, Value>,
    creates: usize,
    writes: usize,
    status_writes: usize,
    lose_create: bool,
    lose_update: bool,
    drop_update: bool,
}

impl FakeNotion {
    fn add(
        &mut self,
        content: &str,
        parent: Option<&str>,
        title: &str,
        properties: Value,
        source: Option<&str>,
    ) -> String {
        let identifier = Uuid::new_v4().to_string();
        self.pages.insert(identifier.clone(),json!({"id":identifier,"content":content,
            "parent_page_id":parent,"parent_data_source_id":source,"title":title,"properties":properties}));
        identifier
    }
}

impl Notion for FakeNotion {
    fn query_tasks(&mut self) -> Result<Vec<Value>> {
        self.pages.iter().filter(|(_,page)|page["properties"]["状态"]=="已完成待归档")
            .map(|(identifier,page)| Ok(json!({"url":page_url(identifier)?,
                "任务名称":page["properties"]["任务名称"],"所属项目":page["properties"]["所属项目"]}))).collect()
    }

    fn get_task_metadata(&mut self, identifier: &str) -> Result<Value> {
        let page = self.pages.get(identifier).context("Missing fake page")?;
        Ok(json!({"page_id":identifier,"properties":page["properties"]}))
    }

    fn update_properties(&mut self, identifier: &str, properties: &Value) -> Result<Value> {
        self.call(
            "update",
            &json!({"page_id":identifier,"command":"update_properties","properties":properties}),
        )
    }

    fn fetch(&mut self, identifier: &str) -> Result<Value> {
        if identifier.starts_with("notion://") {
            return Ok(json!({"text":"Notion-flavored Markdown specification"}));
        }
        let page = self.pages.get(identifier).context("Missing fake page")?;
        let mut content = page["content"].as_str().unwrap_or("").to_string();
        for (child_id, child) in &self.pages {
            if child["parent_page_id"] == identifier {
                let child_url = page_url(child_id)?;
                if !content.contains(&format!("<page url=\"{child_url}\"")) {
                    content.push_str(&format!(
                        "\n<page url=\"{child_url}\">{}</page>\n",
                        child["title"].as_str().unwrap_or("page")
                    ));
                }
            }
        }
        let parent = if let Some(parent) = page["parent_page_id"].as_str() {
            format!("<parent-page url=\"{}\" />", page_url(parent)?)
        } else if let Some(source) = page["parent_data_source_id"].as_str() {
            format!("<parent-data-source url=\"collection://{source}\" />")
        } else {
            String::new()
        };
        let mut properties = page["properties"].clone();
        properties["url"] = json!(page_url(identifier)?);
        Ok(
            json!({"id":identifier,"page_last_edited_at":"2026-11-04T00:00:00Z",
            "text":format!("<page url=\"{}\">\n<ancestor-path>\n{parent}\n</ancestor-path>\n<properties>{}</properties>\n<content>\n{content}\n</content>\n</page>\n",page_url(identifier)?,serde_json::to_string(&properties)?)}),
        )
    }

    fn call(&mut self, role: &str, arguments: &Value) -> Result<Value> {
        match role {
            "query" => {
                let mut rows = self.query_tasks()?;
                let cursor = arguments["data"]["params"][1].as_str().unwrap_or("");
                rows.retain(|row| row["url"].as_str().is_some_and(|value| value > cursor));
                rows.truncate(50);
                Ok(json!({"results":rows}))
            }
            "create" => {
                self.creates += 1;
                let page = &arguments["pages"][0];
                let identifier = self.add(
                    page["content"].as_str().unwrap_or(""),
                    arguments["parent"]["page_id"].as_str(),
                    page["properties"]["title"].as_str().unwrap_or("page"),
                    json!({}),
                    None,
                );
                if self.lose_create {
                    bail!("response lost");
                }
                Ok(json!({"pages":[{"id":identifier}]}))
            }
            "update" => {
                let identifier = arguments["page_id"]
                    .as_str()
                    .context("Missing fake page ID")?;
                let page = self
                    .pages
                    .get_mut(identifier)
                    .context("Missing fake page")?;
                if arguments["command"] == "update_properties" {
                    self.status_writes += 1;
                    merge(&mut page["properties"], arguments["properties"].clone());
                    return Ok(json!({}));
                }
                self.writes += 1;
                if !self.drop_update {
                    let before = page["content"].as_str().unwrap_or("");
                    let content = match arguments["command"].as_str() {
                        Some("replace_content") => arguments["new_str"]
                            .as_str()
                            .context("Missing fake replacement")?
                            .to_string(),
                        Some("insert_content") => format!(
                            "{before}\n\n{}",
                            arguments["content"].as_str().unwrap_or("")
                        ),
                        Some("update_content") => before.replacen(
                            arguments["content_updates"][0]["old_str"]
                                .as_str()
                                .context("Missing old text")?,
                            arguments["content_updates"][0]["new_str"]
                                .as_str()
                                .context("Missing new text")?,
                            1,
                        ),
                        _ => bail!("Unsupported fake update"),
                    };
                    page["content"] = json!(content);
                }
                if self.lose_update {
                    bail!("response lost");
                }
                Ok(json!({}))
            }
            _ => bail!("Unsupported fake operation {role}"),
        }
    }
}

#[test]
fn unknown_creation_recovers_without_replay() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut notion = FakeNotion::default();
    let parent = notion.add("", None, "parent", json!({}), None);
    notion.lose_create = true;
    assert!(
        SafeWriter::new(&mut notion, directory.path())
            .ensure_child(
                &parent,
                "child",
                "marker",
                "marker\ncomplete",
                "child",
                false
            )
            .is_err()
    );
    notion.lose_create = false;
    let child = SafeWriter::new(&mut notion, directory.path()).ensure_child(
        &parent,
        "child",
        "marker",
        "marker\ncomplete",
        "child",
        false,
    )?;
    assert_eq!(child["parent_page_id"], parent);
    assert_eq!(notion.creates, 1);
    Ok(())
}

#[test]
fn unresolved_creation_is_not_repeated() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut notion = FakeNotion::default();
    let parent = notion.add("", None, "parent", json!({}), None);
    write_json(
        &directory.path().join("child.json"),
        &json!({"phase":"creating"}),
    )?;
    assert!(
        SafeWriter::new(&mut notion, directory.path())
            .ensure_child(&parent, "child", "marker", "marker", "child", false)
            .is_err()
    );
    assert_eq!(notion.creates, 0);
    Ok(())
}

#[test]
fn lost_update_is_read_back_and_human_changes_are_preserved() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut notion = FakeNotion::default();
    let identifier = notion.add("before", None, "page", json!({}), None);
    notion.lose_update = true;
    assert!(
        SafeWriter::new(&mut notion, directory.path())
            .replace(&identifier, "after", "write", Some("before"))
            .is_err()
    );
    SafeWriter::new(&mut notion, directory.path()).replace(
        &identifier,
        "after",
        "write",
        Some("before"),
    )?;
    assert_eq!(notion.writes, 1);
    notion
        .pages
        .get_mut(&identifier)
        .context("Missing fake page")?["content"] = json!("human edit");
    assert!(
        SafeWriter::new(&mut notion, directory.path())
            .replace(&identifier, "after", "write", Some("before"))
            .is_err()
    );
    assert_eq!(notion.pages[&identifier]["content"], "human edit");
    Ok(())
}

#[test]
fn unchanged_readback_has_a_bounded_write_budget() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut notion = FakeNotion::default();
    let identifier = notion.add("before", None, "page", json!({}), None);
    notion.drop_update = true;
    for _ in 0..3 {
        assert!(
            SafeWriter::new(&mut notion, directory.path())
                .replace(&identifier, "after", "write", Some("before"))
                .is_err()
        );
    }
    assert_eq!(notion.writes, 2);
    Ok(())
}

#[test]
fn canonical_keeps_literal_code_and_normalizes_mentions() -> Result<()> {
    let identifier = Uuid::new_v4().to_string();
    assert_eq!(canonical("first\nsecond"), canonical("first<br />second"));
    assert_ne!(canonical("`a<br />b`"), canonical("`a b`"));
    assert_ne!(
        canonical("```html\n<br />\n```"),
        canonical("```html\n\n```")
    );
    assert_eq!(
        canonical("```text\nsame\n```"),
        canonical("```txt\nsame\n```")
    );
    assert_eq!(
        canonical(&mention(&identifier)?),
        canonical(&format!(
            "<mention url=\"{}\">title</mention>",
            page_url(&identifier)?
        ))
    );
    Ok(())
}

fn fixture(directory: &std::path::Path) -> Result<(Executor<FakeNotion>, String, String)> {
    let task_source = Uuid::new_v4().to_string();
    let project_source = Uuid::new_v4().to_string();
    let mut notion = FakeNotion::default();
    let project = notion.add("", None, "project", json!({}), Some(&project_source));
    let task = notion.add(
        "Final task report.",
        None,
        "task",
        json!({"任务名称":"task","状态":"已完成待归档","所属项目":[project]}),
        Some(&task_source),
    );
    let config = json!({"archive_data_dir":directory,"data_source_id":task_source,
        "project_data_source_id":project_source,"retry_delays_seconds":[1,2,3],"notify_command":[]});
    Ok((Executor::new(config, notion, None)?, task, project))
}

#[test]
fn no_new_knowledge_creates_verified_receipt_before_status() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (mut executor, identifier, project) = fixture(directory.path())?;
    let mut state = json!({"task_page_id":identifier,"project_id":project,"generation":1,"phase":"queued","title":"task"});
    executor.prepare(&identifier, &mut state)?;
    let original_source = executor.notion.pages[&identifier]["content"].clone();
    state["phase"] = json!("planned");
    state["plan"] = json!({"summary":"","topics":[],"no_update_reason":"Reviewed; no new reusable knowledge.","conflicts":[]});
    executor.apply_plan(&identifier, &mut state)?;
    assert_eq!(state["phase"], "knowledge_verified");
    assert_eq!(executor.notion.status_writes, 0);
    executor.receipt(&identifier, &mut state)?;
    assert_eq!(state["phase"], "receipt_verified");
    assert_eq!(executor.notion.status_writes, 0);
    executor.commit(&identifier, &mut state)?;
    assert_eq!(state["phase"], "archived");
    assert_eq!(executor.notion.status_writes, 1);
    assert_eq!(
        executor.notion.pages[&identifier]["content"],
        original_source
    );
    assert_eq!(
        read_json(&executor.task_path(&identifier), Value::Null)?["phase"],
        "archived"
    );
    Ok(())
}

#[test]
fn changed_source_blocks_commit_and_unknown_status_is_reconciled() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (mut executor, identifier, project) = fixture(directory.path())?;
    let mut state = json!({"task_page_id":identifier,"project_id":project,"generation":1,"phase":"queued","title":"task"});
    executor.prepare(&identifier, &mut state)?;
    state["phase"] = json!("planned");
    state["plan"] =
        json!({"summary":"","topics":[],"no_update_reason":"No change.","conflicts":[]});
    executor.apply_plan(&identifier, &mut state)?;
    executor.receipt(&identifier, &mut state)?;
    executor
        .notion
        .pages
        .get_mut(&identifier)
        .context("Missing fake page")?["content"] = json!("new source");
    assert!(executor.commit(&identifier, &mut state).is_err());
    assert_eq!(executor.notion.status_writes, 0);
    state["phase"] = json!("committing");
    executor
        .notion
        .pages
        .get_mut(&identifier)
        .context("Missing fake page")?["properties"]["状态"] = json!("已归档");
    executor.commit(&identifier, &mut state)?;
    assert_eq!(state["phase"], "archived");
    assert_eq!(executor.notion.status_writes, 0);
    Ok(())
}

#[test]
fn pending_exception_update_reuses_its_intent_after_lost_reply() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (mut executor, identifier, project) = fixture(directory.path())?;
    executor.config["exception_parent_page_id"] = json!(project);
    let mut state = json!({"task_page_id":identifier,"title":"task","phase":"queued","category":"user_input","error":"missing project","attempts":1});
    write_json(&executor.task_path(&identifier), &state)?;
    executor.notify()?;
    state["attempts"] = json!(2);
    write_json(&executor.task_path(&identifier), &state)?;
    executor.notion.lose_update = true;
    assert!(executor.notify().is_err());
    let path = directory.path().join("notification.json");
    let pending = read_json(&path, Value::Null)?;
    assert!(pending["pending_remote_key"].is_string());
    let writes = executor.notion.writes;
    executor.notify()?;
    assert_eq!(executor.notion.writes, writes);
    let recovered = read_json(&path, Value::Null)?;
    assert!(recovered["pending_remote_key"].is_null());
    assert_eq!(recovered["remote_revision"], pending["remote_revision"]);
    Ok(())
}

#[test]
fn signatures_match_python_sorted_json_spacing() -> Result<()> {
    let value = json!({"z":["中",1],"a":true});
    assert_eq!(
        value_digest(&value)?,
        digest("{\"a\": true, \"z\": [\"中\", 1]}".as_bytes())
    );
    Ok(())
}

fn mcp_envelope(payload: Value) -> Value {
    json!({"content":[{"type":"text","text":payload.to_string()}],
        "structuredContent":null,"isError":false})
}

#[test]
fn native_mcp_null_structured_content_and_nested_results() -> Result<()> {
    for payload in [
        json!({"id":"collection://source","text":"<data-source>schema</data-source>"}),
        json!({"results":[]}),
        json!({"pages":[{"id":Uuid::new_v4().to_string()}]}),
        json!({"success":true}),
    ] {
        let envelope = mcp_envelope(payload.clone());
        for wrapped in [
            envelope.clone(),
            json!({"result":envelope}),
            json!({"result":null,"structuredContent":payload}),
        ] {
            assert_eq!(unwrap_tool(wrapped)?, payload);
        }
    }
    assert_eq!(
        unwrap_tool(json!({"structuredContent":null,"content":[
            {"type":"text","text":"{\"results\":"},
            {"type":"text","text":"[]}"}
        ]}))?,
        json!({"results":[]})
    );
    assert_eq!(
        unwrap_tool(json!({"structuredContent":null,"content":[
            {"type":"text","text":"Markdown specification"}
        ]}))?,
        json!({"text":"Markdown specification"})
    );
    for invalid in [
        json!({"isError":true,"content":[{"type":"text","text":"{}"}]}),
        mcp_envelope(json!({"isError":true})),
        mcp_envelope(json!({"type":"async_task","id":"unknown-write"})),
        mcp_envelope(json!({"async_task":null})),
        json!({"structuredContent":null,"content":[]}),
        json!({"structuredContent":null,"content":[{"type":"image","data":"x"}]}),
        json!({"structuredContent":null,"content":[{"type":"text","text":"{\"results\":"}]}),
    ] {
        assert!(unwrap_tool(invalid).is_err());
    }
    Ok(())
}

struct EnvelopeNotion(FakeNotion);

impl Notion for EnvelopeNotion {
    fn query_tasks(&mut self) -> Result<Vec<Value>> {
        self.0.query_tasks()
    }

    fn get_task_metadata(&mut self, identifier: &str) -> Result<Value> {
        self.0.get_task_metadata(identifier)
    }

    fn update_properties(&mut self, identifier: &str, properties: &Value) -> Result<Value> {
        Ok(mcp_envelope(
            self.0.update_properties(identifier, properties)?,
        ))
    }

    fn fetch(&mut self, identifier: &str) -> Result<Value> {
        Ok(mcp_envelope(self.0.fetch(identifier)?))
    }

    fn call(&mut self, role: &str, arguments: &Value) -> Result<Value> {
        Ok(mcp_envelope(self.0.call(role, arguments)?))
    }
}

#[test]
fn native_envelopes_support_context_receipt_and_final_status() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (executor, identifier, project) = fixture(directory.path())?;
    let mut executor = Executor::new(executor.config, EnvelopeNotion(executor.notion), None)?;
    let context = executor.context_for_task(&identifier)?;
    assert_eq!(context["status"], "read");
    assert_eq!(context["task_page_id"], identifier);
    let mut state = json!({"task_page_id":identifier,"project_id":project,
        "generation":1,"phase":"queued","title":"task"});
    executor.prepare(&identifier, &mut state)?;
    state["phase"] = json!("planned");
    state["plan"] = json!({"summary":"","topics":[],
        "no_update_reason":"Reviewed; no new reusable knowledge.","conflicts":[]});
    executor.apply_plan(&identifier, &mut state)?;
    executor.receipt(&identifier, &mut state)?;
    assert_eq!(executor.notion.0.status_writes, 0);
    executor.commit(&identifier, &mut state)?;
    assert_eq!(state["phase"], "archived");
    assert_eq!(executor.notion.0.status_writes, 1);
    let raw = executor.notion.fetch(&identifier)?;
    let mut outer_truncated = raw.clone();
    outer_truncated["truncated"] = json!(true);
    assert!(parse_page(&outer_truncated).is_err());
    let mut part_truncated = raw;
    part_truncated["content"][0]["truncated"] = json!(true);
    assert!(parse_page(&part_truncated).is_err());
    Ok(())
}

#[test]
fn native_archive_empty_queue_uses_sql_and_nullable_mcp_envelopes() -> Result<()> {
    use std::os::unix::net::UnixListener;
    use std::time::Duration;
    use tungstenite::Message;

    let directory = tempfile::tempdir()?;
    let socket_path = directory.path().join("offline-daemon.sock");
    let listener = UnixListener::bind(&socket_path)?;
    let daemon = std::thread::spawn(move || -> Result<Vec<Value>> {
        let (stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        let mut socket = tungstenite::accept(stream)?;
        let mut calls = Vec::new();
        loop {
            let frame = match socket.read() {
                Ok(Message::Text(frame)) => frame,
                Ok(Message::Close(_)) | Err(_) => break,
                _ => continue,
            };
            let request: Value = serde_json::from_str(&frame)?;
            if request.get("id").is_none() {
                continue;
            }
            let response = match request["method"].as_str() {
                Some("initialize") => json!({"userAgent":"offline-archive-daemon"}),
                Some("thread/start") => json!({"thread":{"id":Uuid::new_v4().to_string()}}),
                Some("mcpServer/tool/call") => {
                    let params = &request["params"];
                    calls.push(params.clone());
                    match params["tool"].as_str() {
                        Some("fetch") => mcp_envelope(json!({
                            "id":params["arguments"]["id"],"text":"<data-source>schema</data-source>"
                        })),
                        Some("query") => {
                            ensure!(params["arguments"]["data"]["mode"] == "sql");
                            mcp_envelope(json!({"results":[],"has_more":false}))
                        }
                        _ => bail!("Unexpected offline archive write"),
                    }
                }
                _ => bail!("Unexpected offline archive RPC"),
            };
            socket.send(Message::Text(
                json!({"id":request["id"],"result":response})
                    .to_string()
                    .into(),
            ))?;
        }
        Ok(calls)
    });
    let root = directory.path().join("archive");
    let config = json!({"enabled":true,"daemon_socket":socket_path,
        "request_timeout_seconds":2,"worker_cwd":directory.path(),
        "archive_data_dir":root,"data_source_id":Uuid::new_v4().to_string(),
        "project_data_source_id":Uuid::new_v4().to_string(),
        "mcp_server":"offline","tools":{"fetch":"fetch","query":"query"},
        "notify_command":[]});
    let result = run(&config);
    let calls = daemon.join().expect("Offline archive daemon panicked")?;
    assert_eq!(result?, json!({}));
    assert_eq!(calls.len(), 3);
    assert_eq!(
        read_json(&root.join("runner-health.json"), Value::Null)?["status"],
        "healthy"
    );
    assert_eq!(
        read_json(&root.join("discovery.json"), Value::Null)?["rows"],
        json!([])
    );
    Ok(())
}

#[test]
fn changed_knowledge_hash_and_receipt_prevent_status_commit() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (mut executor, identifier, project) = fixture(directory.path())?;
    let mut state = json!({"task_page_id":identifier,"project_id":project,"generation":1,"phase":"queued","title":"task"});
    executor.prepare(&identifier, &mut state)?;
    state["phase"] = json!("planned");
    state["plan"] =
        json!({"summary":"","topics":[],"no_update_reason":"No change.","conflicts":[]});
    executor.apply_plan(&identifier, &mut state)?;
    executor.receipt(&identifier, &mut state)?;
    let context = state["context_id"]
        .as_str()
        .context("Missing context ID")?
        .to_string();
    let original = executor.notion.pages[&context]["content"].clone();
    executor
        .notion
        .pages
        .get_mut(&context)
        .context("Missing context page")?["content"] = json!("Human knowledge correction.");
    assert!(executor.commit(&identifier, &mut state).is_err());
    assert_eq!(executor.notion.status_writes, 0);
    executor
        .notion
        .pages
        .get_mut(&context)
        .context("Missing context page")?["content"] = original;
    let receipt = state["receipt_page_id"]
        .as_str()
        .context("Missing receipt ID")?
        .to_string();
    executor
        .notion
        .pages
        .get_mut(&receipt)
        .context("Missing receipt page")?["content"] = json!("Changed receipt.");
    assert!(executor.commit(&identifier, &mut state).is_err());
    assert_eq!(executor.notion.status_writes, 0);
    Ok(())
}

#[test]
fn knowledge_update_creates_topic_and_preserves_conflict_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (mut executor, identifier, project) = fixture(directory.path())?;
    let old_source = executor.notion.add(
        "The deployed mode uses push.",
        Some(&identifier),
        "Deployment report",
        json!({}),
        None,
    );
    let new_source = executor.notion.add(
        "The trial recommends pull.",
        Some(&identifier),
        "Trial report",
        json!({}),
        None,
    );
    let conflict = format!(
        "Deployed mode uses push {}; trial recommends pull {}. No deployment replacement is proven.",
        mention(&old_source)?,
        mention(&new_source)?
    );
    let summary = SUMMARY_SECTIONS
        .iter()
        .map(|section| format!("## {section}\n\n{conflict}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut state = json!({"task_page_id":identifier,"project_id":project,"generation":1,"phase":"queued","title":"task"});
    executor.prepare(&identifier, &mut state)?;
    let plan = json!({"summary":summary,"topics":[{"key":"synchronization","title":"Synchronization policy",
        "when_to_read":"Before changing synchronization.","content":conflict,
        "source_ids":[old_source,new_source]}],"no_update_reason":"","conflicts":[conflict]});
    let plan = validate_plan(&plan, &[old_source.clone(), new_source.clone()], &[])?;
    state["phase"] = json!("planned");
    state["plan"] = plan;
    executor.apply_plan(&identifier, &mut state)?;
    assert_eq!(executor.notion.status_writes, 0);
    assert_eq!(
        state["topic_ids"]
            .as_array()
            .context("Missing topics")?
            .len(),
        1
    );
    executor.receipt(&identifier, &mut state)?;
    let topic = state["topic_ids"][0].as_str().context("Missing topic ID")?;
    let topic_content = executor.notion.pages[topic]["content"]
        .as_str()
        .context("Missing topic content")?;
    assert!(topic_content.contains(&old_source.replace('-', "")));
    assert!(topic_content.contains(&new_source.replace('-', "")));
    assert!(topic_content.contains("No deployment replacement is proven"));
    executor.commit(&identifier, &mut state)?;
    assert_eq!(state["phase"], "archived");
    assert_eq!(executor.notion.status_writes, 1);
    Ok(())
}

#[test]
fn source_tree_change_prevents_archive_and_rework_withdraws_unwritten_plan() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (mut executor, identifier, project) = fixture(directory.path())?;
    let mut state = json!({"task_page_id":identifier,"project_id":project,"generation":1,"phase":"queued","title":"task"});
    executor.prepare(&identifier, &mut state)?;
    state["phase"] = json!("planned");
    state["plan"] =
        json!({"summary":"","topics":[],"no_update_reason":"No change.","conflicts":[]});
    executor.apply_plan(&identifier, &mut state)?;
    executor.receipt(&identifier, &mut state)?;
    executor.notion.add(
        "New report.",
        Some(&identifier),
        "New report",
        json!({}),
        None,
    );
    assert!(executor.commit(&identifier, &mut state).is_err());
    assert_eq!(executor.notion.status_writes, 0);
    state["phase"] = json!("planned");
    executor
        .notion
        .pages
        .get_mut(&identifier)
        .context("Missing task page")?["properties"]["状态"] = json!("待办事项");
    executor.advance(&identifier, &mut state)?;
    assert_eq!(state["phase"], "withdrawn");
    assert_eq!(executor.notion.status_writes, 0);
    Ok(())
}
