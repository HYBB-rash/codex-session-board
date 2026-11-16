use super::remote::remote_value;
use super::*;

impl App {
    fn calibration_health(&self, host: &str) -> Result<Value> {
        Ok(self.mapping_store.native_mapping(host)?.and_then(|record| record.probe)
            .map(|probe| json!({"requestId":probe.request,
                "resetAllowed":probe.thread.is_none(),"cleanupPending":probe.thread.is_some()}))
            .unwrap_or(Value::Null))
    }

    pub(super) fn append_managed_host(
        &self,
        remote: &App,
        sessions: &mut Vec<Value>,
        legacy: &[Value],
        projects: &mut Vec<Value>,
    ) -> Result<Value> {
        let host = &remote.config.host_id;
        let view = crate::catalog::view::read(remote, &self.store.catalog_drafts(host)?)?;
        let rows = view.rows;
        let known = remote.store.saved_session_ids()?;
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
        health["nativeCalibration"] = self.calibration_health(host)?;
        health["writeError"] = json!(checked.err().map(|error| error.to_string()));
        health["catalog"] = view.health;
        health["connected"] = json!(writable);
        append_projects(projects, view.projects, &health, writable, host);
        sessions.retain(|row| {
            row["hostId"] != *host || !known.contains(row["threadId"].as_str().unwrap_or(""))
        });
        for row in rows {
            let project = row.get("catalogProjectId").cloned();
            let read_only = row["readOnly"] == true;
            let mut row = remote_value(host, &health["hostLabel"], row, legacy)?;
            if let Some(project) = project {
                row["projectId"] = project;
            }
            row["readOnly"] = json!(!writable || read_only
                || remote.config.native_sections && !row["sectionVersion"].is_string());
            sessions.retain(|previous| previous["id"] != row["id"]);
            sessions.push(row);
        }
        health["hostId"] = json!(host);
        Ok(health)
    }
}

fn append_projects(projects: &mut Vec<Value>, targets: Vec<Value>, health: &Value,
    writable: bool, host: &str) {
    for mut project in targets {
        project["hostLabel"] = health["hostLabel"].clone();
        project["canCreate"] = json!(writable && health["catalog"]["cached"] == false
            && project["pendingDeletion"] != true && host != "local");
        projects.push(project);
    }
}

#[cfg(test)]
#[path = "remote_snapshot_tests.rs"]
mod tests;
