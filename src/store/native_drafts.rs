use super::*;

impl Store {
    pub(crate) fn native_draft_begin(&self, project: &str, operation: &str) -> Result<Value> {
        ensure!(
            uuid::Uuid::parse_str(operation)?.to_string() == operation,
            "原生新建操作 ID 无效"
        );
        ensure!(project.starts_with("target:"), "原生新建项目来源无效");
        self.change_native_draft(operation, |previous| {
            if previous.is_null() {
                *previous = json!({"projectId":project,"operationId":operation,
                    "stage":"preparing","source":null,"target":null});
            }
            ensure!(previous["projectId"] == project, "新建操作已用于另一个项目");
            Ok(())
        })
    }

    pub(crate) fn native_draft_bound(&self, operation: &str, source: &Value) -> Result<Value> {
        self.change_native_draft(operation, |record| {
            ensure!(record.is_object(), "原生新建操作不存在");
            immutable_draft_field(record, "source", source)
        })
    }

    pub(crate) fn native_draft_prepared(&self, operation: &str, target: &Value) -> Result<Value> {
        self.change_native_draft(operation, |record| {
            ensure!(record["source"].is_object(), "原生新建来源尚未核实");
            immutable_draft_field(record, "target", target)?;
            if record["stage"] == "preparing" {
                record["stage"] = json!("prepared");
            }
            Ok(())
        })
    }

    pub(crate) fn native_draft_opened(&self, operation: &str) -> Result<Value> {
        self.change_native_draft(operation, |record| {
            ensure!(record["target"].is_object(), "原生新建目标尚未准备");
            ensure!(
                matches!(record["stage"].as_str(), Some("prepared" | "opened")),
                "原生新建阶段无效"
            );
            record["stage"] = json!("opened");
            Ok(())
        })
    }

    fn change_native_draft(
        &self,
        operation: &str,
        change: impl FnOnce(&mut Value) -> Result<()>,
    ) -> Result<Value> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let key = format!("native_draft_v1:{operation}");
        let previous: Option<String> = transaction
            .query_row("SELECT value FROM settings WHERE key=?", [&key], |row| {
                row.get(0)
            })
            .optional()?;
        let mut record = previous
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or(Value::Null);
        change(&mut record)?;
        transaction.execute(
            "INSERT INTO settings(key,value) VALUES (?,?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, record.to_string()],
        )?;
        transaction.commit()?;
        Ok(record)
    }
}

fn immutable_draft_field(record: &mut Value, field: &str, value: &Value) -> Result<()> {
    ensure!(
        value.as_object().is_some_and(|value| !value.is_empty()),
        "原生新建核验信息不完整"
    );
    ensure!(
        record[field].is_null() || record[field] == *value,
        "原生新建的{field}已变化，请重新选择目标"
    );
    record[field] = value.clone();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_draft_retries_preserve_source_target_and_operation_after_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let operation = uuid::Uuid::new_v4().to_string();
        let project = "target:[\"remote:a\",\"targetLegacy\",\"old\"]";
        let source = json!({"hostId":"remote:a","codexHome":"/target","account":"first"});
        let target = json!({"projectId":"canonical","roots":["/target/project"]});
        let store = Store::open(directory.path(), false)?;
        assert_eq!(
            store.native_draft_begin(project, &operation)?["stage"],
            "preparing"
        );
        store.native_draft_bound(&operation, &source)?;
        drop(store);
        let store = Store::open(directory.path(), false)?;
        assert_eq!(
            store.native_draft_begin(project, &operation)?["source"],
            source
        );
        assert!(
            store
                .native_draft_bound(&operation, &json!({"account":"other"}))
                .is_err()
        );
        store.native_draft_prepared(&operation, &target)?;
        store.native_draft_opened(&operation)?;
        assert_opened_target_is_immutable(&store, project, &operation, &target)
    }

    fn assert_opened_target_is_immutable(
        store: &Store, project: &str, operation: &str, target: &Value,
    ) -> Result<()> {
        assert_eq!(
            store.native_draft_begin(project, operation)?["stage"],
            "opened"
        );
        assert_eq!(
            store.native_draft_prepared(operation, target)?["stage"],
            "opened"
        );
        assert!(
            store
                .native_draft_prepared(operation, &json!({"projectId":"other"}))
                .is_err()
        );
        assert!(
            store
                .native_draft_begin("target:other", operation)
                .is_err()
        );
        assert_eq!(
            store.native_draft_begin(project, operation)?["target"],
            *target
        );
        Ok(())
    }

    #[test]
    fn native_drafts_allow_distinct_operations_without_reusing_legacy_receipts() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = Store::open(directory.path(), false)?;
        let first = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        let project = "target:[\"remote:a\",\"targetServer\",\"canonical\"]";
        store.native_draft_begin(project, &first)?;
        store.native_draft_begin(project, &second)?;
        assert!(store.pending_drafts()?.is_empty());
        assert!(store.draft(&first).is_err());
        assert!(
            store
                .native_draft_prepared(&first, &json!({"projectId":"canonical"}))
                .is_err()
        );
        assert!(store.native_draft_opened(&first).is_err());
        assert!(store.native_draft_bound(&first, &json!({})).is_err());
        assert!(
            store
                .native_draft_bound("missing", &json!({"hostId":"remote:a"}))
                .is_err()
        );
        assert!(store.native_draft_begin(project, "invalid").is_err());
        assert!(
            store
                .native_draft_begin("remote:legacy", &uuid::Uuid::new_v4().to_string())
                .is_err()
        );
        Ok(())
    }
}
