use super::*;
use crate::native_sections::mapping::Record;
use std::collections::HashSet;

impl Store {
    pub(crate) fn is_native_probe(&self, host: &str, identifier: &str) -> Result<bool> {
        Ok(self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
            [format!("native_probe:{host}:{identifier}")], |row| row.get(0))?)
    }

    pub(crate) fn reclaim_native_probe(&self, host: &str, identifier: &str) -> Result<bool> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let saved: Option<String> = transaction.query_row(
            "SELECT value FROM settings WHERE key=?", [format!("native_mapping:v1:{host}")],
            |row| row.get(0)).optional()?;
        let record: Option<Record> = saved.map(|value| serde_json::from_str(&value)).transpose()?;
        if record.and_then(|record| record.probe).and_then(|probe| probe.thread)
            .is_some_and(|thread| thread == identifier) { return Ok(false); }
        let changed = transaction.execute("DELETE FROM settings WHERE key=?",
            [format!("native_probe:{host}:{identifier}")])?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    pub(crate) fn abandon_unknown_native_probe(&self, host: &str, request: &str,
        source: &Value) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let key = format!("native_mapping:v1:{host}");
        let saved: String = transaction.query_row("SELECT value FROM settings WHERE key=?",
            [&key], |row| row.get(0))?;
        let mut record: Record = serde_json::from_str(&saved)?;
        let probe = record.probe.as_ref().context("校准请求已结束，请刷新")?;
        ensure!(record.scope["source"] == *source, "校准请求属于另一个来源");
        ensure!(probe.request == request && probe.thread.is_none(),
            "校准请求已改变或仍有可清理的会话，请刷新");
        transaction.execute("INSERT INTO settings(key,value) VALUES (?,?)",
            params![format!("native_mapping_abandoned:v1:{host}:{request}"),saved])?;
        record.probe = None;
        transaction.execute("UPDATE settings SET value=? WHERE key=?",
            params![serde_json::to_string(&record)?,key])?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn release_native_probe(&self, host: &str, record: &mut Record) -> Result<()> {
        let identifier = record.probe.as_ref().and_then(|probe| probe.thread.as_deref())
            .context("校准会话归属未知")?.to_owned();
        let mut replacement = record.clone();
        replacement.probe = None;
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        transaction.execute("DELETE FROM settings WHERE key=?",
            [format!("native_probe:{host}:{identifier}")])?;
        transaction.execute("UPDATE settings SET value=? WHERE key=?",
            params![serde_json::to_string(&replacement)?,format!("native_mapping:v1:{host}")])?;
        transaction.commit()?;
        *record = replacement;
        Ok(())
    }

    pub(crate) fn native_mapping(&self, host: &str) -> Result<Option<Record>> {
        let database = self.connection()?;
        let record: Option<String> = database
            .query_row(
                "SELECT value FROM settings WHERE key=?",
                [format!("native_mapping:v1:{host}")],
                |row| row.get(0),
            )
            .optional()?;
        record
            .map(|record| serde_json::from_str(&record).map_err(Into::into))
            .transpose()
    }

    pub(crate) fn save_native_mapping(&self, host: &str, record: &Record) -> Result<()> {
        ensure!(record.scope["host"] == host, "分区映射主机不匹配");
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        transaction.execute(
            "INSERT INTO settings(key,value) VALUES (?,?)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![
                format!("native_mapping:v1:{host}"),
                serde_json::to_string(record)?
            ],
        )?;
        if let Some(probe) = &record.probe
            && let Some(thread) = &probe.thread
        {
            ensure!(
                uuid::Uuid::parse_str(thread)?.to_string() == *thread,
                "校准会话 ID 无效"
            );
            transaction.execute(
                "INSERT OR IGNORE INTO settings(key,value) VALUES (?,?)",
                params![
                    format!("native_probe:{host}:{thread}"),
                    record.scope.to_string()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn native_probes(&self, host: &str) -> Result<HashSet<String>> {
        let prefix = format!("native_probe:{host}:");
        let database = self.connection()?;
        let mut query =
            database.prepare("SELECT key FROM settings WHERE substr(key,1,length(?))=?")?;
        let keys = query
            .query_map(params![prefix, prefix], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(keys
            .into_iter()
            .map(|key| key[prefix.len()..].to_string())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_sections::mapping::Probe;

    #[test]
    fn calibration_ownership_and_unknown_creation_survive_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut record = Record {
            scope: json!({"host":"mac","owner":"account"}),
            logical: Default::default(),
            probe: Some(Probe {
                request: uuid::Uuid::new_v4().to_string(),
                thread: None,
                prepared: false,
                initial_name: Value::Null,
                history_mode: Value::Null,
            }),
        };
        let store = Store::open(directory.path(), false)?;
        store.save_native_mapping("mac", &record)?;
        drop(store);
        let store = Store::open(directory.path(), false)?;
        assert!(
            store
                .native_mapping("mac")?
                .unwrap()
                .probe
                .unwrap()
                .thread
                .is_none()
        );
        let identifier = uuid::Uuid::new_v4().to_string();
        record.probe.as_mut().unwrap().thread = Some(identifier.clone());
        store.save_native_mapping("mac", &record)?;
        record.probe = None;
        store.save_native_mapping("mac", &record)?;
        assert!(store.native_probes("mac")?.contains(&identifier));
        assert!(store.native_probes("other")?.is_empty());
        assert!(store.native_mapping("other")?.is_none());
        Ok(())
    }

    #[test]
    fn probe_reclaim_protects_pending_calibration_and_releases_a_completed_owned_thread() -> Result<()> {
        let store = Store::open_memory(false)?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut record = Record {scope:json!({"host":"mac"}),logical:Default::default(),
            probe:Some(Probe {request:uuid::Uuid::new_v4().to_string(),thread:Some(id.clone()),
                prepared:true,initial_name:Value::Null,history_mode:Value::Null})};
        store.save_native_mapping("mac",&record)?;
        assert!(!store.reclaim_native_probe("mac",&id)?);
        assert!(store.is_native_probe("mac",&id)?);
        assert!(!store.reclaim_native_probe("other",&id)?);
        record.probe = None;
        store.save_native_mapping("mac",&record)?;
        assert!(store.reclaim_native_probe("mac",&id)?);
        assert!(!store.is_native_probe("mac",&id)?);
        assert!(!store.reclaim_native_probe("mac",&id)?);
        Ok(())
    }

    #[test]
    fn malformed_probe_cannot_partially_publish_a_mapping() -> Result<()> {
        let store = Store::open_memory(false)?;
        let record = Record {
            scope: json!({"host":"mac"}),
            logical: Default::default(),
            probe: Some(Probe {
                request: uuid::Uuid::new_v4().to_string(),
                thread: Some("invalid".into()),
                prepared: false,
                initial_name: Value::Null,
                history_mode: Value::Null,
            }),
        };
        assert!(store.save_native_mapping("mac", &record).is_err());
        assert!(store.native_mapping("mac")?.is_none());
        assert!(store.save_native_mapping("other", &record).is_err());
        assert!(store.native_mapping("other")?.is_none());
        Ok(())
    }
}
