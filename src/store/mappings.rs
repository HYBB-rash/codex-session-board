use super::*;
use crate::native_sections::mapping::Record;
use std::collections::HashSet;

impl Store {
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
    fn malformed_probe_cannot_partially_publish_a_mapping() -> Result<()> {
        let store = Store::open_memory(false)?;
        let record = Record {
            scope: json!({"host":"mac"}),
            logical: Default::default(),
            probe: Some(Probe {
                request: uuid::Uuid::new_v4().to_string(),
                thread: Some("invalid".into()),
            }),
        };
        assert!(store.save_native_mapping("mac", &record).is_err());
        assert!(store.native_mapping("mac")?.is_none());
        assert!(store.save_native_mapping("other", &record).is_err());
        assert!(store.native_mapping("other")?.is_none());
        Ok(())
    }
}
