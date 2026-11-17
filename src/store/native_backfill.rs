use super::*;
use crate::{config::Config, native_sections::Observation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BackfillChange {
    pub state: String,
    pub target: String,
    pub version: i64,
    pub input: i64,
    pub scope: Value,
}

#[derive(Serialize, Deserialize)]
struct Backfill {
    scope: Value,
    done: bool,
    #[serde(default)]
    baselined: bool,
    #[serde(default)]
    started_at: Option<f64>,
    pending: Option<BackfillChange>,
}

pub(crate) fn default_section(native: &Observation) -> bool {
    ["unsectioned", "chats", "threads"].contains(&native.version.as_str())
}

fn load(database: &Connection, identifier: &str, scope: &Value) -> Result<Backfill> {
    let saved: Option<String> = database.query_row("SELECT value FROM settings WHERE key=?",
        [format!("native_backfill:v1:{identifier}")], |row| row.get(0)).optional()?;
    let record: Option<Backfill> = saved.map(|value| serde_json::from_str(&value)).transpose()?;
    Ok(record.filter(|record| record.done || record.scope == *scope)
        .unwrap_or_else(|| Backfill {scope:scope.clone(),done:false,baselined:false,
            started_at:Some(now()),pending:None}))
}

fn save(database: &Transaction<'_>, identifier: &str, record: &Backfill) -> Result<()> {
    database.execute("INSERT INTO settings(key,value) VALUES (?,?)
        ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![format!("native_backfill:v1:{identifier}"),serde_json::to_string(record)?])?;
    Ok(())
}

fn settle_titles(database: &Transaction<'_>, identifier: &str, native: &Observation) -> Result<()> {
    database.execute("DELETE FROM title_updates WHERE thread_id=? AND state<>?",
        params![identifier,native.state])?;
    database.execute("UPDATE title_updates SET version=(SELECT version FROM sessions WHERE id=?)
        WHERE thread_id=?", params![identifier,identifier])?;
    Ok(())
}

fn candidate(row: &Value, status: &Value) -> String {
    let turn = turn_of(row);
    let live = (status["type"] == "active" && row["observed_revision"] == row["updated_at"])
        .then(|| runtime_state(status, &turn)).flatten();
    let state = row["selected_state"].as_str()
        .or_else(|| live.map(|value| value.0))
        .or_else(|| row["automatic_state"].as_str())
        .unwrap_or_else(|| suggestion(row).0);
    crate::model::board_state(state).into()
}

fn validate(database: &Connection, identifier: &str, change: &BackfillChange) -> Result<()> {
    let record = load(database, identifier, &change.scope)?;
    let row = get_row(database, identifier)?;
    ensure!(!record.done && record.pending.as_ref() == Some(change)
        && row["archived"] == 0 && row["version"] == change.version
        && row["input_revision"] == change.input,
        "远程分区迁移已过期，请重新读取");
    Ok(())
}

impl Store {
    pub(crate) fn native_backfill_since(&self, identifier: &str, fallback: f64) -> Result<f64> {
        Ok(load(&*self.connection()?,identifier,&Value::Null)?.started_at.unwrap_or(fallback))
    }

    pub(crate) fn native_backfill_pending(&self, identifier: &str) -> Result<bool> {
        Ok(!load(&*self.connection()?, identifier, &Value::Null)?.done)
    }

    pub(crate) fn cache_native_backfill(&self, identifier: &str, native: &Observation,
        scope: &Value) -> Result<bool> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let mut record = load(&transaction, identifier, scope)?;
        if record.done { return Ok(false); }
        let row = get_row(&transaction, identifier)?;
        if row["archived"] == 1 { return Ok(true); }
        if row["native_section"] != native.version || row["native_state"] != native.state
            || native::unconfirmed_input(&row,native) && row["needs_review"] != 1 {
            native::write_daily(&transaction, identifier, &native::DailyWrite {
                column:Some(native),bump_version:true,
                needs_review:native::unconfirmed_input(&row,native).then_some(1),
                ..native::DailyWrite::default()})?;
        }
        if !default_section(native) {
            record.done = true;
            record.pending = None;
            settle_titles(&transaction, identifier, native)?;
        }
        save(&transaction, identifier, &record)?;
        transaction.commit()?;
        Ok(true)
    }

    pub(crate) fn prepare_native_backfill(&self, identifier: &str, config: &Config,
        status: &Value, scope: &Value) -> Result<Option<BackfillChange>> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let mut record = load(&transaction, identifier, scope)?;
        if record.done { return Ok(None); }
        let row = get_row(&transaction, identifier)?;
        let section = row["native_section"].as_str().context("等待确认远程原生位置")?;
        ensure!(row["archived"] == 0
            && ["unsectioned","chats","threads"].contains(&section), "远程原生位置已改变");
        let state = candidate(&row, status);
        let change = BackfillChange {
            target:config.section_target(&state)?.into(),state,
            version:row["version"].as_i64().context("状态版本无效")?,
            input:row["input_revision"].as_i64().context("输入版本无效")?,
            scope:scope.clone(),
        };
        record.pending = Some(change.clone());
        save(&transaction, identifier, &record)?;
        transaction.commit()?;
        Ok(Some(change))
    }

    pub(crate) fn validate_native_backfill(&self, identifier: &str,
        change: &BackfillChange) -> Result<()> {
        validate(&*self.connection()?, identifier, change)
    }

    pub(crate) fn finish_native_backfill(&self, identifier: &str,
        change: &BackfillChange, confirmed: &Observation) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        validate(&transaction, identifier, change)?;
        ensure!(confirmed.state == change.state && confirmed.version == change.target,
            "迁移目标尚未在远程原生位置确认");
        native::write_daily(&transaction, identifier, &native::DailyWrite {
            column:Some(confirmed),bump_version:true,expect_version:Some(change.version),
            expect_input:Some(change.input),
            needs_review:native::unconfirmed_input(&get_row(&transaction,identifier)?,confirmed)
                .then_some(1),..native::DailyWrite::default()})?;
        let mut record = load(&transaction,identifier,&change.scope)?;
        record.done = true;
        record.pending = None;
        save(&transaction,identifier,&record)?;
        settle_titles(&transaction, identifier, confirmed)?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn baseline_native_backfill(&self, identifier: &str, status: &Value,
        turn: &Value, since: f64) -> Result<bool> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let mut record = load(&transaction, identifier, &Value::Null)?;
        if !record.done || record.baselined { return Ok(false); }
        let Some((state,_)) = runtime_state(status,turn) else { return Ok(false); };
        let Some(key) = runtime_native::phase_key(state,status,turn) else { return Ok(false); };
        let time = turn[if terminal(status,turn) {"completedAt"} else {"startedAt"}].as_f64();
        let old = time.is_none_or(|time| time < since);
        if old { runtime_native::keep_phase(&transaction,identifier,&key)?; }
        record.baselined = true;
        save(&transaction,identifier,&record)?;
        transaction.commit()?;
        Ok(old)
    }
}

#[cfg(test)]
#[path = "native_backfill_tests.rs"]
mod tests;
