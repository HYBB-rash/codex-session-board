use super::*;
use crate::native_sections::Observation;

pub struct RuntimeChange {
    pub state: &'static str,
    reason: &'static str,
    key: String,
    pub version: i64,
    pub section_version: String,
}

pub(super) fn pending(row: &Value) -> Result<Option<Value>> {
    Ok(row["runtime_pending_key"]
        .as_str()
        .map(serde_json::from_str::<Value>)
        .transpose()?)
}

fn event(
    row: &Value,
    status: &Value,
    turn: &Value,
    since: f64,
    pending: bool,
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
    Some((
        state,
        reason,
        key,
        old_event_keeps_card(row, status, turn, since, pending),
    ))
}

fn old_event_keeps_card(
    row: &Value,
    status: &Value,
    turn: &Value,
    since: f64,
    pending_turn: bool,
) -> bool {
    let time = turn[if terminal(status, turn) {
        "completedAt"
    } else {
        "startedAt"
    }]
    .as_f64();
    let no_phase = row["runtime_key"].is_null();
    let before_observation = time.is_none_or(|time| time < since);
    let before_placement = terminal(status, turn)
        && row["native_choice_at"]
            .as_f64()
            .is_some_and(|placed| time.is_none_or(|end| end <= placed));
    !pending_turn && (no_phase && before_observation || before_placement)
}

fn keep_phase(database: &rusqlite::Transaction<'_>, identifier: &str, key: &str) -> Result<()> {
    native::write_daily(
        database,
        identifier,
        &native::DailyWrite {
            pending: native::Pending::Clear,
            phase: native::Phase::Set(key),
            ..native::DailyWrite::default()
        },
    )
}

fn queue_move(database: &rusqlite::Transaction<'_>, identifier: &str, key: &str) -> Result<()> {
    native::write_daily(
        database,
        identifier,
        &native::DailyWrite {
            pending: native::Pending::Set(key),
            ..native::DailyWrite::default()
        },
    )
}

impl Store {
    pub fn has_pending_native_runtime(&self, identifier: &str) -> Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT runtime_pending_key IS NOT NULL FROM sessions WHERE id=?",
            [identifier],
            |row| row.get(0),
        )?)
    }

    pub fn consume_native_runtime(&self, identifier: &str, change: &RuntimeChange) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        native::write_daily(
            &transaction,
            identifier,
            &native::DailyWrite {
                pending: native::Pending::Clear,
                phase: native::Phase::Set(&change.key),
                expect_version: Some(change.version),
                expect_pending: Some(&change.key),
                ..native::DailyWrite::default()
            },
        )?;
        transaction.commit()?;
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
        let pending = pending(&row)?.is_some_and(|key| key[0] == turn["id"]);
        let Some((state, reason, key, preserve)) = event(&row, status, turn, since, pending) else {
            return Ok(None);
        };
        if preserve {
            keep_phase(&transaction, identifier, &key)?;
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
        queue_move(&transaction, identifier, &change.key)?;
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
        native::write_daily(
            &transaction,
            identifier,
            &native::DailyWrite {
                column: Some(native),
                pending: native::Pending::Clear,
                choice: native::Choice::Clear,
                phase: native::Phase::Set(&change.key),
                clear_selected: true,
                automatic: Some((change.state, change.reason)),
                bump_version: true,
                ..native::DailyWrite::default()
            },
        )?;
        transaction.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier, now(), change.state, change.reason],
        )?;
        self.queue_title(&transaction, identifier)?;
        transaction.commit()?;
        Ok(())
    }
}
