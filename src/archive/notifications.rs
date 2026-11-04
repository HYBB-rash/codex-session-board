use super::executor::{Executor, json_files};
use super::{
    canonical, epoch, mention, merge, page_url, strings, text, timestamp, truthy, value_digest,
};
use crate::clients::Notion;
use crate::common::{command, digest, read_json, write_json};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub(crate) fn desktop_notify(arguments: &Value, title: &str, message: &str) -> Result<Value> {
    if arguments.is_null()
        || arguments
            .as_array()
            .is_some_and(|arguments| arguments.is_empty())
    {
        return Ok(json!({"delivery":"unavailable","read_acknowledged":false}));
    }
    let arguments = command(arguments)?;
    let mut child = Command::new(&arguments[0])
        .args(&arguments[1..])
        .arg(title)
        .arg(message)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(
                json!({"delivery":if status.success() {"submitted"} else {"failed"},
                "returncode":status.code().unwrap_or(-1),"read_acknowledged":false}),
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Archive desktop notification timed out");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

impl<N: Notion> Executor<N> {
    pub fn publish_notifications(&mut self) -> Result<()> {
        let path = self.root.join("notification-health.json");
        let prior = read_json(&path, json!({}))?;
        if !truthy(&self.config["_notification_probe"])
            && epoch() < prior["next_attempt_at"].as_f64().unwrap_or(0.0)
        {
            return Ok(());
        }
        match self.notify() {
            Ok(()) => write_json(&path, &json!({"status":"healthy","at":timestamp()})),
            Err(error) => self.notification_failure(&path, &prior, &error),
        }
    }

    fn blocked_notification(&self, prior: &Value, error: &anyhow::Error) -> (Value, bool, bool) {
        let reason = error.to_string();
        let signature = digest(reason.as_bytes());
        let same = prior["status"] == "blocked" && prior["signature"] == signature;
        let attempts = if same {
            prior["attempts"].as_u64().unwrap_or(0) + 1
        } else {
            1
        };
        let delays = self.delays();
        let exhausted = attempts as usize > delays.len();
        let delay = if exhausted {
            self.seconds("recovery_probe_seconds", 3600.0)
        } else {
            delays[attempts as usize - 1]
        };
        let mut record = prior.clone();
        merge(
            &mut record,
            json!({"status":"blocked","reason":reason,"signature":signature,
                    "checked_at":timestamp(),"attempts":attempts,"next_attempt_at":epoch()+delay}),
        );
        let attention = exhausted || notification_needs_attention(&reason);
        let fresh = attention && (!same || prior["alert_signature"] != signature);
        (record, same, fresh)
    }

    fn notification_failure(
        &self,
        path: &std::path::Path,
        prior: &Value,
        error: &anyhow::Error,
    ) -> Result<()> {
        let (mut record, same, fresh) = self.blocked_notification(prior, error);
        if !same {
            let mut failure = record.clone();
            failure["diagnostic"] = json!(format!("{error:#}"));
            write_json(
                &self
                    .root
                    .join("notification-failures")
                    .join(format!("{}.json", Uuid::new_v4().simple())),
                &failure,
            )?;
        }
        if fresh {
            self.deliver_notification_failure(path, &mut record)?;
        }
        write_json(path, &record)
    }

    fn deliver_notification_failure(
        &self,
        path: &std::path::Path,
        record: &mut Value,
    ) -> Result<()> {
        let signature = record["signature"].clone();
        merge(
            record,
            json!({"at":timestamp(),"delivery":"intent","read_acknowledged":false,"alert_signature":signature}),
        );
        write_json(path, record)?;
        let target = if let Some(parent) = self.config["exception_parent_page_id"].as_str() {
            page_url(parent)?
        } else {
            path.display().to_string()
        };
        match desktop_notify(
            &self.config["notify_command"],
            "Notion 归档异常清单更新受阻",
            &format!("归档队列继续运行，清单写入检查点已保留；运行记录：{target}"),
        ) {
            Ok(delivery) => merge(record, delivery),
            Err(error) => merge(
                record,
                json!({"delivery":"failed","error_type":error.to_string()}),
            ),
        }
        Ok(())
    }

    pub fn notify(&mut self) -> Result<()> {
        let unresolved = self.exceptions()?;
        let notification_path = self.root.join("notification.json");
        let mut prior = read_json(&notification_path, json!({"sent":[]}))?;
        let ids = exception_signatures(&unresolved)?;
        let sent: BTreeSet<_> = strings(prior.get("sent").unwrap_or(&json!([])))?
            .into_iter()
            .collect();
        let fresh = ids.difference(&sent).next().is_some();
        let remote = self.publish_exception_page(&unresolved, &mut prior, &notification_path)?;
        if fresh {
            self.deliver_exceptions(&unresolved, ids, remote, prior, &notification_path)?;
        } else {
            merge(
                &mut prior,
                json!({"sent":ids,"items":unresolved,"at":timestamp()}),
            );
            if unresolved.is_empty() {
                prior["delivery"] = json!("silent");
            }
            write_json(&notification_path, &prior)?;
        }
        Ok(())
    }

    fn exceptions(&self) -> Result<Vec<Value>> {
        let mut unresolved = Vec::new();
        for path in json_files(&self.root.join("tasks"))? {
            let state = read_json(&path, Value::Null)?;
            if truthy(&state["error"])
                && (state["exhausted"] == true || state["category"] == "user_input")
            {
                unresolved.push(json!({"task_page_id":text(&state,"task_page_id")?,"title":text(&state,"title")?,
                    "phase":state["phase"],"category":state["category"],"reason":state["error"],
                    "attempts":state["attempts"].as_u64().unwrap_or(0),
                    "resume":"补齐所属项目或恢复连接/权限后自动巡检续接；不重建未知写入。"}));
            }
        }
        write_json(
            &self.root.join("exceptions.json"),
            &json!({"at":timestamp(),"items":unresolved}),
        )?;
        Ok(unresolved)
    }

    fn publish_exception_page(
        &mut self,
        unresolved: &[Value],
        prior: &mut Value,
        notification_path: &std::path::Path,
    ) -> Result<Option<String>> {
        let remote_digest = value_digest(&json!(unresolved))?;
        let mut remote = prior["page_id"].as_str().map(str::to_string);
        if let Some(parent) = self.config["exception_parent_page_id"]
            .as_str()
            .map(str::to_string)
            && (remote.is_none() || prior["remote_digest"] != remote_digest)
        {
            let (marker, content) = exception_content(&parent, unresolved)?;
            let operation_root = self.root.join("operations");
            let page = self.writer(operation_root.clone()).ensure_child(
                &parent,
                "自动归档异常清单",
                &marker,
                &content,
                "exception-index",
                true,
            )?;
            let remote_id = text(&page, "id")?.to_string();
            remote = Some(remote_id.clone());
            let key = self.exception_operation(&page, &content, prior, notification_path)?;
            self.writer(operation_root.clone()).replace(
                &remote_id,
                &content,
                &key,
                Some(text(&page, "content")?),
            )?;
            let key = verified_exception_operation(&operation_root, key)?;
            merge(
                prior,
                json!({"page_id":remote_id,"remote_digest":remote_digest,"remote_operation":key}),
            );
            prior
                .as_object_mut()
                .context("Invalid notification checkpoint")?
                .remove("pending_remote_key");
            write_json(notification_path, prior)?;
        }
        Ok(remote)
    }

    fn exception_operation(
        &self,
        page: &Value,
        content: &str,
        prior: &mut Value,
        path: &std::path::Path,
    ) -> Result<String> {
        if let Some(key) = prior["pending_remote_key"].as_str() {
            return Ok(key.to_string());
        }
        let root = self.root.join("operations");
        ensure!(
            known_exception_content(&root, page, content)?,
            "Exception list has unrecognized external changes; preserve it"
        );
        let revision = prior["remote_revision"].as_u64().unwrap_or(0) + 1;
        let operation = format!("exceptions-v3-{revision}");
        ensure!(
            !root.join(format!("{operation}.json")).exists(),
            "Exception publication sequence needs reconciliation"
        );
        merge(
            prior,
            json!({"remote_revision":revision,"pending_remote_key":operation,"page_id":page["id"]}),
        );
        write_json(path, prior)?;
        Ok(operation)
    }

    fn deliver_exceptions(
        &self,
        unresolved: &[Value],
        ids: BTreeSet<String>,
        remote: Option<String>,
        prior: Value,
        notification_path: &std::path::Path,
    ) -> Result<()> {
        let target = remote
            .as_deref()
            .map(page_url)
            .transpose()?
            .unwrap_or_else(|| self.root.join("exceptions.json").display().to_string());
        let message = format!("{} 个归档任务需要处理；详情见 {target}", unresolved.len());
        let mut intent = prior;
        merge(
            &mut intent,
            json!({"at":timestamp(),"sent":ids,"items":unresolved,"page_id":remote,"delivery":"intent"}),
        );
        write_json(notification_path, &intent)?;
        let delivery = desktop_notify(
            &self.config["notify_command"],
            "Notion 自动归档需要处理",
            &message,
        )?;
        merge(&mut intent, delivery);
        write_json(notification_path, &intent)
    }
}

fn notification_needs_attention(reason: &str) -> bool {
    [
        "unrecognized external",
        "unverified ownership",
        "sequence needs reconciliation",
    ]
    .iter()
    .any(|fragment| reason.contains(fragment))
}

fn exception_signatures(unresolved: &[Value]) -> Result<BTreeSet<String>> {
    unresolved
        .iter()
        .map(|item| {
            let mut signature = item.clone();
            signature
                .as_object_mut()
                .context("Invalid exception record")?
                .remove("attempts");
            value_digest(&signature)
        })
        .collect()
}

fn exception_content(parent: &str, unresolved: &[Value]) -> Result<(String, String)> {
    let marker = format!("notion-program/archive-exceptions/{parent}");
    let mut content = format!(
        "{marker}\n\n运行记录：{}\n\n## 自动归档异常清单\n\n\
        成功任务静默；仅列无法安全继续的任务。修复原因后自动巡检，未知写入先读回核对。桌面通知提交成功不等于 Herman 已读。\n\n",
        mention(parent)?
    );
    for item in unresolved {
        content.push_str(&format!(
            "### {}\n\n{}\n\n已完成到：`{}`。自动尝试：{} 次。\n\n原因：\n\n```json\n{}\n```\n\n需要与恢复：{}\n\n",
            text(item,"title")?,mention(text(item,"task_page_id")?)?,text(item,"phase")?,
            item["attempts"],serde_json::to_string(&item["reason"])?,text(item,"resume")?
        ));
    }
    if unresolved.is_empty() {
        content.push_str("当前没有需要人工介入的归档异常。\n");
    }
    Ok((marker, content))
}

fn known_exception_content(root: &std::path::Path, page: &Value, content: &str) -> Result<bool> {
    let current = canonical(text(page, "content")?);
    let mut known = current == canonical(content);
    for path in json_files(root)? {
        if !path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("exceptions"))
        {
            continue;
        }
        let operation = read_json(&path, Value::Null)?;
        if operation["page_id"] == page["id"]
            && current == canonical(operation["after"].as_str().unwrap_or(""))
        {
            known = true;
            break;
        }
    }
    Ok(known)
}

fn verified_exception_operation(root: &std::path::Path, mut key: String) -> Result<String> {
    for _ in 0..20 {
        let operation = read_json(&root.join(format!("{key}.json")), json!({}))?;
        if let Some(next) = operation["superseded_by"].as_str() {
            key = next.to_string();
        } else {
            ensure!(
                operation["phase"] == "verified",
                "Exception publication is not verified"
            );
            return Ok(key);
        }
    }
    bail!("Exception publication revision chain is invalid")
}
