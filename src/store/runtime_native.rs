use super::*;
use crate::native_sections::Observation;

pub struct RuntimeChange {
    pub state: &'static str,
    reason: &'static str,
    key: String,
    pub version: i64,
    pub section_version: String,
}

fn event(
    row: &Value,
    status: &Value,
    turn: &Value,
    since: f64,
) -> Option<(&'static str, &'static str, String, bool)> {
    let (state, reason) = runtime_state(status, turn)?;
    let identifier = turn["id"].as_str().filter(|value| !value.is_empty())?;
    let mut flags = status["activeFlags"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if status["type"] != "active" {
        flags.clear();
    }
    flags.sort_by_key(Value::to_string);
    let key = json!([identifier, state, flags]).to_string();
    if row["archived"] == 1 || row["runtime_key"] == key {
        return None;
    }
    let time = turn[if terminal(status, turn) {
        "completedAt"
    } else {
        "startedAt"
    }]
    .as_f64();
    let old_baseline = row["runtime_key"].is_null() && time.is_none_or(|time| time < since);
    Some((
        state,
        reason,
        key,
        old_baseline || preserve_runtime(row, status, turn, Some(since), state),
    ))
}

impl Store {
    pub fn consume_native_runtime(&self, identifier: &str, change: &RuntimeChange) -> Result<()> {
        self.connection()?.execute(
            "UPDATE sessions SET runtime_key=?,runtime_pending_key=NULL WHERE id=? AND version=? AND runtime_pending_key=?",
            params![change.key,identifier,change.version,change.key])?;
        Ok(())
    }

    pub fn prepare_native_runtime(
        &self,
        identifier: &str,
        status: &Value,
        turn: &Value,
        since: f64,
    ) -> Result<Option<RuntimeChange>> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        let Some((state, reason, key, preserve)) = event(&row, status, turn, since) else {
            return Ok(None);
        };
        if preserve {
            transaction.execute(
                "UPDATE sessions SET runtime_key=?,runtime_pending_key=NULL WHERE id=?",
                params![key, identifier],
            )?;
            transaction.commit()?;
            return Ok(None);
        }
        let change = RuntimeChange {
            state,
            reason,
            key,
            version: row["version"].as_i64().context("状态版本无效")?,
            section_version: row["native_revision"]
                .as_str()
                .context("等待确认原生分区")?
                .into(),
        };
        transaction.execute(
            "UPDATE sessions SET runtime_pending_key=? WHERE id=?",
            params![change.key, identifier],
        )?;
        transaction.commit()?;
        Ok(Some(change))
    }

    pub fn commit_native_runtime(
        &self,
        identifier: &str,
        change: &RuntimeChange,
        native: &Observation,
    ) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        ensure!(
            row["archived"] == 0
                && row["version"] == change.version
                && row["runtime_pending_key"] == change.key,
            "自动阶段已过期，请重新读取原生状态"
        );
        ensure!(native.state == change.state, "自动阶段原生结果不匹配");
        native::cache(&transaction, identifier, native)?;
        transaction.execute(
            "UPDATE sessions SET runtime_key=?,runtime_pending_key=NULL,automatic_state=?,automatic_reason=?,selected_state=NULL,version=version+1 WHERE id=?",
            params![change.key,change.state,change.reason,identifier])?;
        transaction.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier, now(), change.state, change.reason],
        )?;
        self.queue_title(&transaction, identifier)?;
        transaction.commit()?;
        Ok(())
    }
}
