use super::*;
use crate::store::remote::identity;
use std::collections::BTreeMap;

impl App {
    pub(crate) fn route_remote(&self, path: &str, body: &Value) -> Result<Value> {
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        let fields: Vec<String> = serde_json::from_str(
            identifier.strip_prefix("remote:").context("无效远程 ID")?
        )?;
        ensure!(fields.len() == 3 && fields[0] == "codex", "无效远程来源");
        ensure!(identity(&fields[1], &fields[2]) == identifier, "远程身份格式无效");
        let uuid = uuid::Uuid::parse_str(&fields[2]).context("无效会话 ID")?;
        ensure!(uuid.to_string() == fields[2], "无效会话 ID");
        let remote = self.remote_app(&fields[1])?;
        remote.store.note(&fields[2])?;
        let mut request = body.clone();
        request["id"] = json!(fields[2]);
        remote.request(path, Some(&request))
    }

    pub(crate) fn append_remote(
        &self,
        sessions: &mut Vec<Value>,
        catalog: &mut Value,
    ) -> Result<()> {
        let projects = self.store.remote_projects()?;
        let registered: HashSet<_> = projects
            .iter()
            .map(|project| project["id"].clone().to_string())
            .collect();
        let mut remote = self.store.remote_sessions()?;
        self.append_managed_remote(&mut remote)?;
        let mut hosts = BTreeMap::new();
        for project in &projects {
            hosts.insert(
                project["hostId"].to_string(),
                json!({
                    "hostId":project["hostId"],"hostLabel":project["hostLabel"]
                }),
            );
        }
        for session in &mut remote {
            hosts.insert(
                session["hostId"].to_string(),
                json!({
                    "hostId":session["hostId"],"hostLabel":session["hostLabel"]
                }),
            );
            if !registered.contains(&session["projectId"].to_string()) {
                session["projectId"] = Value::Null;
            }
        }
        catalog["loaded"] = json!(catalog["loaded"] == true || !hosts.is_empty());
        catalog["remoteHosts"] = json!(hosts.into_values().collect::<Vec<_>>());
        catalog["items"]
            .as_array_mut()
            .context("项目目录格式无效")?
            .extend(projects);
        sessions.extend(remote);
        sessions.sort_by(|first, second| {
            second["updatedAt"]
                .as_i64()
                .cmp(&first["updatedAt"].as_i64())
        });
        Ok(())
    }

    fn append_managed_remote(&self, sessions: &mut Vec<Value>) -> Result<()> {
        let remotes: Vec<_> = self.remotes.lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?.values().cloned().collect();
        let legacy = sessions.clone();
        let mut reports = Vec::new();
        for remote in remotes {
            let host = &remote.config.host_id;
            let rows = remote.store.sessions()?;
            let active = remote.active_sessions.lock()
                .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?;
            if active.is_some() {
                sessions.retain(|row| row["hostId"] != *host);
            }
            let mut health = remote.health.lock().map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?.clone();
            for row in rows {
                if active.as_ref().is_some_and(|active| !active.contains(row["id"].as_str().unwrap_or(""))) {
                    continue;
                }
                let row = remote_value(host, &health["hostLabel"], row, &legacy)?;
                sessions.retain(|previous| previous["id"] != row["id"]);
                sessions.push(row);
            }
            health["hostId"] = json!(host);
            reports.push(health);
        }
        self.update_health(json!({"remoteConnections":reports}))
    }

    pub(crate) fn open_remote(&self, identifier: &str) -> Result<Value> {
        let session = self.store.remote_session(identifier)?;
        let host = session["host_id"].as_str().context("远程会话缺少主机")?;
        let thread = session["thread_id"].as_str().context("远程会话缺少 ID")?;
        ensure!(identity(host, thread) == identifier, "远程会话身份不一致");
        let url = open_client_url(conversation_url(thread, host)?, &self.config)?;
        Ok(json!({"ok":true,"url":url}))
    }
}

fn remote_value(host: &str, label: &Value, mut row: Value, legacy: &[Value]) -> Result<Value> {
    let thread = row["id"].as_str().context("远程会话缺少 ID")?.to_owned();
    let identifier = identity(host, &thread);
    let cached = legacy.iter().find(|row| row["id"] == identifier);
    row["threadId"] = json!(thread);
    row["id"] = json!(identifier);
    row["hostId"] = json!(host);
    row["hostLabel"] = label.clone();
    row["remote"] = json!(true);
    row["projectId"] = cached.map(|row| row["projectId"].clone()).unwrap_or(Value::Null);
    Ok(row)
}
