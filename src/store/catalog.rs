use super::*;

impl Store {
    pub(crate) fn catalog_drafts(&self, host: &str) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut query =
            database.prepare("SELECT value FROM settings WHERE key GLOB 'draft_request:*'")?;
        let records = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut drafts = Vec::new();
        for record in records {
            let record: Value = serde_json::from_str(&record)?;
            if !matches!(record["stage"].as_str(), Some("created" | "complete")) {
                continue;
            }
            let target = record["project"]
                .as_str()
                .and_then(|project| project.strip_prefix("remote:"))
                .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok());
            if target.is_some_and(|target| {
                target.len() == 3 && target[0] == "codex" && target[1] == host
            }) {
                drafts.push(record);
            }
        }
        Ok(drafts)
    }

    pub(crate) fn catalog_published_ids(&self) -> Result<std::collections::BTreeSet<String>> {
        let database = self.connection()?;
        let mut query = database
            .prepare("SELECT substr(key,19) FROM settings WHERE key GLOB 'catalog_published:*'")?;
        Ok(query
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn catalog_mark_published(&self, identifiers: &[String]) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        for identifier in identifiers {
            ensure!(
                uuid::Uuid::parse_str(identifier)?.to_string() == *identifier,
                "发布会话 UUID 无效"
            );
            transaction.execute(
                "INSERT OR IGNORE INTO settings(key,value) VALUES (?, '1')",
                [format!("catalog_published:{identifier}")],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn catalog_value(&self, key: &str) -> Result<Option<Value>> {
        ensure!(
            matches!(key, "catalog_snapshot_v1" | "catalog_progress_v1"),
            "目录缓存键无效"
        );
        self.connection()?
            .query_row("SELECT value FROM settings WHERE key=?", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .map(|text| serde_json::from_str(&text).map_err(Into::into))
            .transpose()
    }

    pub(crate) fn save_catalog_value(&self, key: &str, value: Option<&Value>) -> Result<()> {
        ensure!(
            matches!(key, "catalog_snapshot_v1" | "catalog_progress_v1"),
            "目录缓存键无效"
        );
        let database = self.connection()?;
        if let Some(value) = value {
            database.execute(
                "INSERT INTO settings(key,value) VALUES (?,?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value.to_string()],
            )?;
        } else {
            database.execute("DELETE FROM settings WHERE key=?", [key])?;
        }
        Ok(())
    }

    pub(crate) fn catalog_sessions(&self) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut statement =
            database.prepare("SELECT * FROM sessions ORDER BY updated_at DESC,id")?;
        let rows = statement
            .query_map([], row_value)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|row| {
                let job = database
                    .query_row(
                        "SELECT * FROM title_updates WHERE thread_id=?",
                        [text(&row, "id")],
                        row_value,
                    )
                    .optional()?;
                let mut value = self.session_value(&row, job.as_ref());
                value["archived"] = json!(row["archived"] == 1);
                Ok(value)
            })
            .collect()
    }
}
