use super::*;

impl Store {
    pub fn draft(&self, request: &str) -> Result<Value> {
        let database = self.connection()?;
        draft_record(&database, request)?.context("新建请求不存在")
    }

    pub fn pending_drafts(&self) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut query = database.prepare(
            "SELECT requests.value FROM settings pending JOIN settings requests
             ON requests.key='draft_request:'||pending.value
             WHERE pending.key GLOB 'draft_pending:*' ORDER BY pending.key",
        )?;
        let rows = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|row| serde_json::from_str(&row).map_err(Into::into))
            .collect()
    }

    pub fn draft_begin(&self, project: &str, request: &str) -> Result<(Value, bool)> {
        ensure!(
            uuid::Uuid::parse_str(request)?.to_string() == request,
            "新建请求 ID 无效"
        );
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        if let Some(previous) = draft_record(&transaction, request)? {
            ensure!(previous["project"] == project, "新建请求已用于另一个项目");
            return Ok((previous, false));
        }
        let pending: Option<String> = transaction
            .query_row(
                "SELECT value FROM settings WHERE key=?",
                [format!("draft_pending:{project}")],
                |row| row.get(0),
            )
            .optional()?;
        ensure!(
            pending.is_none(),
            "该项目有结果待核对的新建请求：{}；不会重复创建",
            pending.unwrap_or_default()
        );
        let record =
            json!({"project":project,"requestId":request,"stage":"starting","threadId":null});
        transaction.execute(
            "INSERT INTO settings(key,value) VALUES (?,?)",
            params![format!("draft_request:{request}"), record.to_string()],
        )?;
        transaction.execute(
            "INSERT INTO settings(key,value) VALUES (?,?)",
            params![format!("draft_pending:{project}"), request],
        )?;
        transaction.commit()?;
        Ok((record, true))
    }

    pub fn draft_update(&self, request: &str, thread: Option<&str>, stage: &str) -> Result<Value> {
        ensure!(
            matches!(stage, "created" | "complete" | "abandoned"),
            "新建阶段无效"
        );
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let mut record = draft_record(&transaction, request)?.context("新建请求不存在")?;
        ensure!(
            matches!(record["stage"].as_str(), Some("starting" | "created")),
            "新建请求已经结束"
        );
        if let Some(thread) = thread {
            draft_thread(&mut record, thread)?;
        }
        ensure!(
            stage != "complete" || record["threadId"].is_string(),
            "新建会话尚未确认"
        );
        record["stage"] = json!(stage);
        transaction.execute(
            "UPDATE settings SET value=? WHERE key=?",
            params![record.to_string(), format!("draft_request:{request}")],
        )?;
        if stage != "created" {
            transaction.execute(
                "DELETE FROM settings WHERE key=? AND value=?",
                params![
                    format!("draft_pending:{}", text(&record, "project")),
                    request
                ],
            )?;
        }
        transaction.commit()?;
        Ok(record)
    }
}

fn draft_thread(record: &mut Value, thread: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(thread)?.to_string() == thread,
        "新建会话 ID 无效"
    );
    ensure!(
        record["threadId"].is_null() || record["threadId"] == thread,
        "新建会话 ID 不一致"
    );
    record["threadId"] = json!(thread);
    Ok(())
}

fn draft_record(database: &Connection, request: &str) -> Result<Option<Value>> {
    let record: Option<String> = database
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("draft_request:{request}")],
            |row| row.get(0),
        )
        .optional()?;
    record
        .map(|record| serde_json::from_str(&record).map_err(Into::into))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertain_drafts_survive_restart_and_cannot_be_retried_as_new_requests() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let request = uuid::Uuid::new_v4().to_string();
        let next = uuid::Uuid::new_v4().to_string();
        let store = Store::open(directory.path(), false)?;
        assert!(store.draft_begin("remote:project", &request)?.1);
        drop(store);
        let store = Store::open(directory.path(), false)?;
        assert!(!store.draft_begin("remote:project", &request)?.1);
        assert!(store.draft_begin("remote:project", &next).is_err());
        assert!(store.draft_begin("different", &request).is_err());
        assert!(store.draft_update(&request, None, "complete").is_err());
        assert!(store.draft_begin("different", &next)?.1);
        store.draft_update(&request, None, "abandoned")?;
        let replacement = uuid::Uuid::new_v4().to_string();
        assert!(store.draft_begin("remote:project", &replacement)?.1);
        Ok(())
    }

    #[test]
    fn acknowledged_draft_retries_reuse_the_same_thread_without_overwriting_new_requests()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = Store::open(directory.path(), false)?;
        let request = uuid::Uuid::new_v4().to_string();
        let thread = uuid::Uuid::new_v4().to_string();
        store.draft_begin("remote:project", &request)?;
        store.draft_update(&request, Some(&thread), "created")?;
        assert!(
            store
                .draft_update(&request, Some(&uuid::Uuid::new_v4().to_string()), "created")
                .is_err()
        );
        store.draft_update(&request, None, "complete")?;
        let record = store.draft_begin("remote:project", &request)?.0;
        assert_eq!(record["stage"], "complete");
        assert_eq!(record["threadId"], thread);
        let next = uuid::Uuid::new_v4().to_string();
        store.draft_begin("remote:project", &next)?;
        assert!(store.draft_update(&request, None, "abandoned").is_err());
        assert!(
            store
                .draft_begin("remote:project", &uuid::Uuid::new_v4().to_string())
                .is_err()
        );
        Ok(())
    }
}
