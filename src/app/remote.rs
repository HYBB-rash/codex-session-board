use super::*;
use crate::store::remote::identity;
use std::collections::BTreeMap;

impl App {
    pub(crate) fn route_remote(&self, path: &str, body: &Value) -> Result<Value> {
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        let (host, thread) = remote_identity(identifier)?;
        let remote = self.remote_app(&host)?;
        remote.store.note(&thread)?;
        let mut request = body.clone();
        request["id"] = json!(thread);
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
        for session in &mut remote {
            if !registered.contains(&session["projectId"].to_string()) {
                session["projectId"] = Value::Null;
            }
        }
        let hosts = self.remote_hosts(&projects, &remote)?;
        catalog["loaded"] = json!(catalog["loaded"] == true || !hosts.is_empty());
        catalog["remoteHosts"] = json!(hosts);
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

    fn remote_hosts(&self, projects: &[Value], sessions: &[Value]) -> Result<Vec<Value>> {
        let health = self
            .health
            .lock()
            .map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?;
        let registered = health["remoteRegistry"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut hosts = BTreeMap::new();
        for record in registered.iter().chain(projects).chain(sessions) {
            hosts.insert(
                record["hostId"].to_string(),
                json!({"hostId":record["hostId"],"hostLabel":record["hostLabel"]}),
            );
        }
        Ok(hosts.into_values().collect())
    }

    pub(crate) fn annotate_connections(&self, sessions: &mut [Value]) {
        for session in sessions {
            if session["readOnly"] == true
                || self.config.native_sections && !session["sectionVersion"].is_string()
            {
                session["readOnly"] = json!(true);
                session["canArchive"] = json!(false);
            }
        }
    }

    fn append_managed_remote(&self, sessions: &mut Vec<Value>) -> Result<()> {
        let legacy = sessions.clone();
        let mut reports = Vec::new();
        for remote in self.remote_instances()? {
            let host = &remote.config.host_id;
            let rows = remote.store.sessions()?;
            let known: HashSet<_> = rows.iter().filter_map(|row| row["id"].as_str()).collect();
            let active = remote
                .active_sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?;
            if active.is_some() {
                sessions.retain(|row| {
                    row["hostId"] != *host
                        || !known.contains(row["threadId"].as_str().unwrap_or(""))
                });
            }
            let mut health = remote
                .health
                .lock()
                .map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?
                .clone();
            for row in rows {
                if active
                    .as_ref()
                    .is_some_and(|active| !active.contains(row["id"].as_str().unwrap_or("")))
                {
                    continue;
                }
                let mut row = remote_value(host, &health["hostLabel"], row, &legacy)?;
                row["readOnly"] = json!(!remote.enabled.load(Ordering::SeqCst));
                sessions.retain(|previous| previous["id"] != row["id"]);
                sessions.push(row);
            }
            health["hostId"] = json!(host);
            reports.push(health);
        }
        self.update_health(json!({"remoteConnections":reports}))
    }

    fn remote_instances(&self) -> Result<Vec<Arc<App>>> {
        Ok(self
            .remotes
            .lock()
            .map_err(|_| anyhow::anyhow!("主机锁已损坏"))?
            .values()
            .cloned()
            .collect())
    }

    pub(crate) fn open_remote(&self, identifier: &str) -> Result<Value> {
        let (host, thread) = remote_identity(identifier)?;
        let remote = if self.config.native_sections {
            self.remote_entry(&host)?.1
        } else {
            None
        };
        let known = remote.is_some_and(|remote| remote.store.note(&thread).is_ok())
            || self.store.remote_session(identifier).is_ok();
        ensure!(known, "远程会话不在看板中");
        let url = open_client_url(conversation_url(&thread, &host)?, &self.config)?;
        Ok(json!({"ok":true,"url":url}))
    }
}

fn remote_identity(identifier: &str) -> Result<(String, String)> {
    let fields: Vec<String> =
        serde_json::from_str(identifier.strip_prefix("remote:").context("无效远程 ID")?)?;
    ensure!(fields.len() == 3 && fields[0] == "codex", "无效远程来源");
    ensure!(
        identity(&fields[1], &fields[2]) == identifier,
        "远程身份格式无效"
    );
    let uuid = uuid::Uuid::parse_str(&fields[2]).context("无效会话 ID")?;
    ensure!(uuid.to_string() == fields[2], "无效会话 ID");
    Ok((fields[1].clone(), fields[2].clone()))
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
    row["projectId"] = cached
        .map(|row| row["projectId"].clone())
        .unwrap_or(Value::Null);
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offline() -> Result<(tempfile::TempDir, App, String)> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("state.json");
        std::fs::write(
            &path,
            json!({"codex-managed-remote-connections":[{
            "hostId":"remote:a","displayName":"测试","alias":"example","hostname":null,
            "sshPort":null,"identity":null}],
            "remote-connection-auto-connect-by-host-id":{"remote:a":false}})
            .to_string(),
        )?;
        let app = App::new(Config {
            data_dir: directory.path().join("data"),
            native_sections: true,
            sync_titles: false,
            sidebar_state_file: path,
            open_command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf '%s\\n' \"$1\" >> \"$0\"".into(),
                directory.path().join("opens").to_string_lossy().into(),
            ],
            ..Config::default()
        })?;
        let thread = uuid::Uuid::new_v4().to_string();
        app.store.discover_remote(
            "remote:a",
            "测试",
            &[json!({"id":thread,
            "hostId":"remote:a","title":"离线会话","updatedAt":1})],
        )?;
        Ok((directory, app, identity("remote:a", &thread)))
    }

    #[test]
    fn opening_a_known_offline_chat_does_not_require_connecting_or_enable_writes() -> Result<()> {
        let (directory, app, identifier) = offline()?;
        let before = app.store.remote_session(&identifier)?;
        let opened = app.request("/api/open", Some(&json!({"id":identifier})))?;
        assert!(
            opened["url"]
                .as_str()
                .unwrap()
                .contains("hostId=remote%3Aa")
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join("opens"))?
                .lines()
                .count(),
            1
        );
        assert!(
            app.request(
                "/api/state",
                Some(&json!({"id":identifier,
            "state":"review","note":"no","version":0,"inputVersion":0}))
            )
            .is_err()
        );
        assert_eq!(app.store.remote_session(&identifier)?, before);
        assert_eq!(app.snapshot()?["sessions"][0]["readOnly"], true);
        assert!(app.remotes.lock().unwrap().is_empty());
        Ok(())
    }

    #[test]
    fn old_chat_navigation_rejects_reassigned_hosts_before_opening_the_client() -> Result<()> {
        let (directory, app, identifier) = offline()?;
        let host = crate::remote::hosts::registered(&app.config)?.remove(0);
        crate::config::write_json(
            &host.directory(&app.config).join("source.json"),
            &json!({"hostId":"remote:a","command":["previous-target"]}),
            false,
        )?;
        assert!(
            app.request("/api/open", Some(&json!({"id":identifier})))
                .is_err()
        );
        assert!(!directory.path().join("opens").exists());
        assert_eq!(app.store.remote_session(&identifier)?["version"], 0);
        Ok(())
    }
}
