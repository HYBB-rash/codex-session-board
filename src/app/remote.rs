use super::*;
use crate::store::remote::identity;
use std::collections::BTreeMap;

impl App {
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

    pub(crate) fn open_remote(&self, identifier: &str) -> Result<Value> {
        let session = self.store.remote_session(identifier)?;
        let host = session["host_id"].as_str().context("远程会话缺少主机")?;
        let thread = session["thread_id"].as_str().context("远程会话缺少 ID")?;
        ensure!(identity(host, thread) == identifier, "远程会话身份不一致");
        let url = open_client_url(conversation_url(thread, host)?, &self.config)?;
        Ok(json!({"ok":true,"url":url}))
    }
}
