use super::remote::remote_value;
use super::*;

impl App {
    pub(super) fn append_managed_host(
        &self,
        remote: &App,
        sessions: &mut Vec<Value>,
        legacy: &[Value],
    ) -> Result<Value> {
        let host = &remote.config.host_id;
        let rows = remote.store.sessions()?;
        let known = remote.store.saved_session_ids()?;
        let active = remote
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("会话发现锁已损坏"))?;
        let mut health = remote
            .health
            .lock()
            .map_err(|_| anyhow::anyhow!("健康状态锁已损坏"))?
            .clone();
        let checked = remote
            .config
            .remote_binding
            .as_ref()
            .map(|binding| binding.require_confirmed())
            .transpose();
        let writable = remote.enabled.load(Ordering::SeqCst) && checked.is_ok();
        health["writeError"] = json!(checked.err().map(|error| error.to_string()));
        sessions.retain(|row| {
            row["hostId"] != *host || !known.contains(row["threadId"].as_str().unwrap_or(""))
        });
        for row in rows {
            if active
                .as_ref()
                .is_some_and(|active| !active.contains(row["id"].as_str().unwrap_or("")))
                && row["sectionVersion"].is_string()
            {
                continue;
            }
            let mut row = remote_value(host, &health["hostLabel"], row, legacy)?;
            row["readOnly"] = json!(!writable);
            sessions.retain(|previous| previous["id"] != row["id"]);
            sessions.push(row);
        }
        health["hostId"] = json!(host);
        Ok(health)
    }
}

#[cfg(test)]
#[path = "remote_snapshot_tests.rs"]
mod tests;
