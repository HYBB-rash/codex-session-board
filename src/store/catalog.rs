use super::*;

impl Store {
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
                let mut value = session_value(&row, job.as_ref(), self.native_sections);
                value["archived"] = json!(row["archived"] == 1);
                Ok(value)
            })
            .collect()
    }
}
