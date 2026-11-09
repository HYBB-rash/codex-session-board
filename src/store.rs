use crate::model::{STATES, now, recommend, runtime_state};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

mod drafts;
mod native;
pub mod remote;
mod runtime_native;
mod sorting;
pub use runtime_native::RuntimeChange;

pub struct Store {
    database: Mutex<Connection>,
    pub sync_titles: bool,
    pub title_epoch: Option<f64>,
    pub native_sections: bool,
    pub legacy_mode: bool,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
 id TEXT PRIMARY KEY, title TEXT NOT NULL, cwd TEXT NOT NULL,
 updated_at INTEGER NOT NULL, seen_at REAL NOT NULL, observed_at REAL,
 attempted_at REAL, observed_revision INTEGER, turn_json TEXT NOT NULL DEFAULT '{}',
 observation_error TEXT, selected_state TEXT, note TEXT NOT NULL DEFAULT '',
 selection_turn TEXT, selection_users TEXT NOT NULL DEFAULT '[]', selected_at REAL,
 runtime_key TEXT, automatic_state TEXT, automatic_reason TEXT,
 needs_review INTEGER NOT NULL DEFAULT 0, input_revision INTEGER NOT NULL DEFAULT 0,
 version INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS changes (
 sequence INTEGER PRIMARY KEY, thread_id TEXT NOT NULL, changed_at REAL NOT NULL,
 state TEXT, note TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS title_updates (
 thread_id TEXT PRIMARY KEY, version INTEGER NOT NULL, state TEXT NOT NULL,
 requested_at REAL NOT NULL, attempted_at REAL, error TEXT);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

fn row_value(row: &Row<'_>) -> rusqlite::Result<Value> {
    let mut value = serde_json::Map::new();
    for (index, name) in row.as_ref().column_names().iter().enumerate() {
        use rusqlite::types::ValueRef;
        let field = match row.get_ref(index)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(number) => json!(number),
            ValueRef::Real(number) => json!(number),
            ValueRef::Text(text) => json!(String::from_utf8_lossy(text)),
            ValueRef::Blob(_) => Value::Null,
        };
        value.insert((*name).to_string(), field);
    }
    Ok(Value::Object(value))
}

fn text<'value>(value: &'value Value, field: &str) -> &'value str {
    value[field].as_str().unwrap_or_default()
}

fn turn_of(row: &Value) -> Value {
    serde_json::from_str(text(row, "turn_json")).unwrap_or(json!({}))
}

fn users(turn: &Value) -> Vec<&str> {
    turn["userItems"]
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn suggestion(row: &Value) -> (&'static str, &'static str) {
    if row["observed_revision"].is_number()
        && row["updated_at"].as_f64() > row["observed_revision"].as_f64()
    {
        return ("unknown", "会话有更新，等待重新读取历史。");
    }
    recommend(&turn_of(row))
}

fn effective(row: &Value, native: bool) -> String {
    if native {
        return row["native_state"].as_str().unwrap_or("unknown").into();
    }
    row["selected_state"]
        .as_str()
        .or_else(|| row["automatic_state"].as_str())
        .unwrap_or_else(|| suggestion(row).0)
        .to_string()
}

fn get_row(database: &Connection, identifier: &str) -> Result<Value> {
    database
        .query_row("SELECT * FROM sessions WHERE id=?", [identifier], row_value)
        .optional()?
        .context("会话不存在")
}

fn validate_selection(
    row: &Value,
    state: Option<&str>,
    note: &str,
    revision: (i64, Option<i64>),
) -> Result<()> {
    ensure!(
        state.is_none_or(|state| STATES.iter().any(|item| item.0 == state)),
        "未知状态"
    );
    ensure!(
        note.chars().count() <= 2000,
        "备注必须为不超过 2000 字的文本"
    );
    ensure!(
        state != Some("blocked") || !note.trim().is_empty(),
        "请填写阻塞原因"
    );
    ensure!(row["archived"] == 0, "会话已归档，请刷新看板");
    ensure!(row["version"] == revision.0, "状态已更新，请刷新后重试");
    ensure!(
        revision
            .1
            .is_none_or(|version| row["input_revision"] == version),
        "会话有新输入，请刷新并核对后重试"
    );
    Ok(())
}

fn archive_expected(row: &Value, revision: (i64, i64), native: bool) -> Result<Value> {
    ensure!(row["archived"] == 0, "会话已归档，请刷新看板");
    ensure!(row["version"] == revision.0, "状态已更新，请刷新后重试");
    ensure!(
        row["input_revision"] == revision.1,
        "会话有新输入，请刷新并核对后重试"
    );
    ensure!(row["needs_review"] == 0, "会话有新输入，请核对后点击卡片上的“确认待归档”");
    ensure!(
        crate::model::board_state(&effective(row, native)) == "to_archive",
        "只能归档待归档会话"
    );
    Ok(if native {
        turn_of(row)
    } else {
        json!({"id":row["selection_turn"],"userItems":serde_json::from_str::<Value>(text(row,"selection_users"))?})
    })
}

fn confirm_archive(database: &Transaction<'_>, identifier: &str) -> Result<()> {
    database.execute(
        "UPDATE sessions SET archived=1,version=version+1 WHERE id=?",
        [identifier],
    )?;
    database.execute("DELETE FROM title_updates WHERE thread_id=?", [identifier])?;
    Ok(())
}

fn queue_title(
    database: &Transaction<'_>,
    identifier: &str,
    enabled: bool,
    native: bool,
) -> Result<()> {
    if !enabled {
        database.execute("DELETE FROM title_updates WHERE thread_id=?", [identifier])?;
        return Ok(());
    }
    let row = get_row(database, identifier)?;
    database.execute(
        "INSERT INTO title_updates(thread_id,version,state,requested_at) VALUES (?,?,?,?)
         ON CONFLICT(thread_id) DO UPDATE SET version=excluded.version,state=excluded.state,
         requested_at=excluded.requested_at,attempted_at=NULL,error=NULL",
        params![
            identifier,
            row["version"].as_i64(),
            effective(&row, native),
            now()
        ],
    )?;
    Ok(())
}

impl Store {
    fn queue_title(&self, database: &Transaction<'_>, identifier: &str) -> Result<()> {
        queue_title(database, identifier, self.sync_titles, self.native_sections)
    }

    pub fn open(directory: &Path, sync_titles: bool) -> Result<Self> {
        crate::config::create_private_dir(directory)?;
        let database = Connection::open(directory.join("status.sqlite3"))?;
        database.busy_timeout(std::time::Duration::from_secs(3))?;
        database.execute_batch(SCHEMA)?;
        Self::upgrade(&database)?;
        Self::upgrade_remote(&database)?;
        let title_epoch = if sync_titles {
            database.execute(
                "INSERT OR IGNORE INTO settings(key,value) VALUES ('title_sync_started_at',?)",
                [now().to_string()],
            )?;
            Some(
                database
                    .query_row(
                        "SELECT value FROM settings WHERE key='title_sync_started_at'",
                        [],
                        |row| row.get::<_, String>(0),
                    )?
                    .parse()?,
            )
        } else {
            None
        };
        Ok(Self {
            database: Mutex::new(database),
            sync_titles,
            title_epoch,
            native_sections: false,
            legacy_mode: false,
        })
    }

    #[cfg(test)]
    pub fn open_memory(sync_titles: bool) -> Result<Self> {
        let database = Connection::open_in_memory()?;
        database.execute_batch(SCHEMA)?;
        Self::upgrade(&database)?;
        Self::upgrade_remote(&database)?;
        Ok(Self {
            database: Mutex::new(database),
            sync_titles,
            title_epoch: None,
            native_sections: false,
            legacy_mode: false,
        })
    }

    fn upgrade(database: &Connection) -> Result<()> {
        let mut statement = database.prepare("PRAGMA table_info(sessions)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (name, declaration) in [
            ("needs_review", "INTEGER NOT NULL DEFAULT 0"),
            ("input_revision", "INTEGER NOT NULL DEFAULT 0"),
            ("runtime_key", "TEXT"),
            ("automatic_state", "TEXT"),
            ("automatic_reason", "TEXT"),
            ("archived", "INTEGER NOT NULL DEFAULT 0"),
            ("native_state", "TEXT"),
            ("native_revision", "TEXT"),
            ("runtime_pending_key", "TEXT"),
            ("native_choice_at", "REAL"),
        ] {
            if !columns.iter().any(|column| column == name) {
                database.execute(
                    &format!("ALTER TABLE sessions ADD COLUMN {name} {declaration}"),
                    [],
                )?;
            }
        }
        Ok(())
    }

    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.database
            .lock()
            .map_err(|_| anyhow::anyhow!("状态数据库锁已损坏"))
    }

    pub fn contains(&self, identifier: &str) -> Result<bool> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT 1 FROM sessions WHERE id=? AND archived=0",
                [identifier],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn note(&self, identifier: &str) -> Result<String> {
        let database = self.connection()?;
        Ok(get_row(&database, identifier)?["note"]
            .as_str()
            .context("会话备注格式无效")?
            .to_string())
    }

    pub fn archive(
        &self,
        identifier: &str,
        version: i64,
        input_version: i64,
        operation: impl FnOnce(&Value) -> Result<()>,
    ) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        let expected = archive_expected(&row, (version, input_version), self.native_sections)?;
        operation(&expected)?;
        confirm_archive(&transaction, identifier)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn discover(&self, threads: &[Value]) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        for thread in threads {
            let identifier = thread["id"].as_str().context("会话缺少 ID")?;
            let title = thread["name"]
                .as_str()
                .filter(|value| !value.is_empty())
                .or_else(|| thread["preview"].as_str())
                .unwrap_or(identifier);
            let title: String = title
                .lines()
                .next()
                .unwrap_or(identifier)
                .chars()
                .take(300)
                .collect();
            transaction.execute(
                "INSERT INTO sessions(id,title,cwd,updated_at,seen_at) VALUES (?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET title=excluded.title,cwd=excluded.cwd,
                 updated_at=excluded.updated_at,seen_at=excluded.seen_at",
                params![
                    identifier,
                    title,
                    text(thread, "cwd"),
                    thread["updatedAt"].as_i64().unwrap_or(0),
                    now()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn observe(&self, identifier: &str, turn: &Value, error: Option<&str>) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        if get_row(&transaction, identifier)?["archived"] == 1 {
            return Ok(());
        }
        if let Some(error) = error {
            transaction.execute(
                "UPDATE sessions SET observation_error=?,attempted_at=? WHERE id=?",
                params![error, now(), identifier],
            )?;
        } else {
            let row = get_row(&transaction, identifier)?;
            let previous = turn_of(&row);
            let new_input = users(turn)
                .iter()
                .any(|user| turn["id"] != previous["id"] || !users(&previous).contains(user));
            let follow_up = row["needs_review"] == 1
                || has_follow_up(&row, turn, self.native_sections)
                || self.native_sections && row["native_state"] == "to_archive" && new_input;
            transaction.execute(
                "UPDATE sessions SET turn_json=?,observed_at=?,attempted_at=?,observed_revision=updated_at,
                 needs_review=?,input_revision=input_revision+?,observation_error=NULL WHERE id=?",
                params![serde_json::to_string(turn)?, now(), now(), follow_up, i64::from(new_input), identifier],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn set_state(
        &self,
        identifier: &str,
        state: Option<&str>,
        note: &str,
        version: i64,
        input_version: Option<i64>,
    ) -> Result<()> {
        self.set_state_inner(identifier, state, note, (version, input_version), None)
    }

    fn set_state_inner(
        &self,
        identifier: &str,
        state: Option<&str>,
        note: &str,
        revision: (i64, Option<i64>),
        native: Option<&crate::native_sections::Observation>,
    ) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        validate_selection(&row, state, note, revision)?;
        let turn = turn_of(&row);
        if let Some(native) = native {
            native::cache(&transaction, identifier, native)?;
            transaction.execute("UPDATE sessions SET native_choice_at=?,runtime_key=COALESCE(runtime_pending_key,runtime_key),runtime_pending_key=NULL WHERE id=?", params![now(),identifier])?;
        }
        transaction.execute(
            "UPDATE sessions SET selected_state=?,note=?,selection_turn=?,selection_users=?,
             selected_at=?,needs_review=0,version=version+1 WHERE id=?",
            params![
                state,
                note.trim(),
                turn["id"].as_str(),
                serde_json::to_string(&users(&turn))?,
                now(),
                identifier
            ],
        )?;
        transaction.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier, now(), state, note.trim()],
        )?;
        self.queue_title(&transaction, identifier)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn observe_runtime(
        &self,
        identifier: &str,
        status: &Value,
        turn: &Value,
        since: Option<f64>,
    ) -> Result<bool> {
        let Some((state, reason)) = runtime_state(status, turn) else {
            return Ok(false);
        };
        let Some(turn_id) = turn["id"]
            .as_str()
            .filter(|identifier| !identifier.is_empty())
        else {
            return Ok(false);
        };
        let mut flags = status["activeFlags"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if status["type"] != "active" {
            flags.clear();
        }
        flags.sort_by_key(Value::to_string);
        let key = json!([turn_id, state, flags]).to_string();
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        if row["archived"] == 1 || row["runtime_key"] == key {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE sessions SET runtime_key=? WHERE id=?",
            params![key, identifier],
        )?;
        if preserve_runtime(&row, status, turn, since, state) {
            transaction.commit()?;
            return Ok(false);
        }
        let queued = self.record_runtime(
            &transaction,
            identifier,
            &row,
            (status, turn, since),
            (state, reason),
        )?;
        transaction.commit()?;
        Ok(queued)
    }

    fn record_runtime(
        &self,
        transaction: &Transaction<'_>,
        identifier: &str,
        row: &Value,
        observation: (&Value, &Value, Option<f64>),
        decision: (&str, &str),
    ) -> Result<bool> {
        let (status, turn, since) = observation;
        let (state, reason) = decision;
        transaction.execute(
            "UPDATE sessions SET automatic_state=?,automatic_reason=?,selected_state=NULL,
             version=version+1 WHERE id=?",
            params![state, reason, identifier],
        )?;
        transaction.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier, now(), state, reason],
        )?;
        let baseline = row["runtime_key"].is_null();
        let mut live = !baseline
            || matches!(status["type"].as_str(), Some("active" | "systemError"))
            || since
                .zip(turn["startedAt"].as_f64())
                .is_some_and(|(since, start)| start >= since);
        if terminal(status, turn)
            && turn["completedAt"]
                .as_f64()
                .or_else(|| turn["startedAt"].as_f64())
                .zip(self.title_epoch)
                .is_some_and(|(end, epoch)| end < epoch)
        {
            live = false;
        }
        let pending = transaction
            .query_row(
                "SELECT 1 FROM title_updates WHERE thread_id=?",
                [identifier],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if live || pending {
            self.queue_title(transaction, identifier)?;
        }
        Ok((live || pending) && self.sync_titles)
    }

    pub fn pending(&self) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut statement = database.prepare(
            "SELECT * FROM sessions WHERE archived=0 ORDER BY attempted_at IS NOT NULL,attempted_at,updated_at DESC",
        )?;
        Ok(statement
            .query_map([], row_value)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn pending_titles(&self, identifier: Option<&str>) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut statement = database.prepare(
            "SELECT * FROM title_updates WHERE (? IS NULL OR thread_id=?)
             AND (?=0 OR EXISTS(SELECT 1 FROM sessions WHERE id=thread_id AND native_revision IS NOT NULL))
             ORDER BY attempted_at IS NOT NULL,attempted_at,requested_at",
        )?;
        Ok(statement
            .query_map(
                params![identifier, identifier, self.native_sections],
                row_value,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn state_revision(&self, identifier: &str) -> Result<(String, i64)> {
        let database = self.connection()?;
        let row = get_row(&database, identifier)?;
        Ok((
            effective(&row, self.native_sections),
            row["version"].as_i64().context("状态版本无效")?,
        ))
    }

    pub fn title_current(&self, identifier: &str, version: i64) -> Result<bool> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT 1 FROM title_updates JOIN sessions ON sessions.id=thread_id
             WHERE thread_id=? AND title_updates.version=? AND sessions.version=?",
                params![identifier, version, version],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn title_synced(&self, identifier: &str, version: i64, title: &str) -> Result<bool> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let removed = transaction.execute(
            "DELETE FROM title_updates WHERE thread_id=? AND version=?",
            params![identifier, version],
        )? > 0;
        if removed {
            let title: String = title.chars().take(300).collect();
            transaction.execute(
                "UPDATE sessions SET title=? WHERE id=?",
                params![title, identifier],
            )?;
        }
        transaction.commit()?;
        Ok(removed)
    }

    pub fn title_failed(&self, identifier: &str, version: i64, error: &str) -> Result<()> {
        let error: String = error.chars().take(500).collect();
        self.connection()?.execute(
            "UPDATE title_updates SET attempted_at=?,error=? WHERE thread_id=? AND version=?",
            params![now(), error, identifier, version],
        )?;
        Ok(())
    }

    pub fn saved_session_ids(&self) -> Result<std::collections::HashSet<String>> {
        let database = self.connection()?;
        let mut query = database.prepare("SELECT id FROM sessions")?;
        Ok(query.query_map([],|row|row.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn has_session(&self, identifier: &str) -> Result<bool> {
        Ok(self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?)",
            [identifier],|row|row.get(0))?)
    }

    pub fn sessions(&self) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut statement = database
            .prepare("SELECT * FROM sessions WHERE archived=0 ORDER BY updated_at DESC,id")?;
        let rows = statement
            .query_map([], row_value)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut result = Vec::new();
        for row in rows {
            let job = database
                .query_row(
                    "SELECT * FROM title_updates WHERE thread_id=?",
                    [text(&row, "id")],
                    row_value,
                )
                .optional()?;
            result.push(session_value(&row, job.as_ref(), self.native_sections));
        }
        Ok(result)
    }
}

fn session_value(row: &Value, job: Option<&Value>, native: bool) -> Value {
    let turn = turn_of(row);
    let (suggestion, reason) = suggestion(row);
    let selected = row["selected_state"].is_string();
    json!({
        "id":row["id"], "title":row["title"], "cwd":row["cwd"], "updatedAt":row["updated_at"],
        "seenAt":row["seen_at"], "observedAt":row["observed_at"], "state":effective(row,native), "manual":selected,
        "sectionVersion":if native {row["native_revision"].clone()} else {Value::Null},
        "suggestion":row["automatic_state"].as_str().unwrap_or(suggestion),
        "reason":if native {"状态来自 Codex 原生分区。"}else{row["automatic_reason"].as_str().unwrap_or(reason)},
        "source":if native {"codex"}else if selected {"manual"} else if row["automatic_state"].is_string() {"runtime"} else {"history"},
        "note":row["note"], "followUp":row["needs_review"]==1, "observationError":row["observation_error"],
        "titleSyncPending":job.is_some(), "titleSyncError":job.map(|job| &job["error"]),
        "turnStatus":turn["status"], "version":row["version"], "inputVersion":row["input_revision"]
    })
}

fn has_follow_up(row: &Value, turn: &Value, native: bool) -> bool {
    if (!native && row["selected_state"].is_null()) || row["selected_at"].is_null() {
        return false;
    }
    if turn["id"] == row["selection_turn"] {
        let selected: Value =
            serde_json::from_str(text(row, "selection_users")).unwrap_or(json!([]));
        return users(turn).iter().any(|user| {
            !selected
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item == user))
        });
    }
    !users(turn).is_empty()
        && turn["startedAt"]
            .as_f64()
            .zip(row["selected_at"].as_f64())
            .is_some_and(|(start, selected)| start > selected)
}

fn terminal(status: &Value, turn: &Value) -> bool {
    !matches!(status["type"].as_str(), Some("active" | "systemError"))
        && matches!(
            turn["status"].as_str(),
            Some("completed" | "failed" | "interrupted")
        )
}

fn preserve_runtime(
    row: &Value,
    status: &Value,
    turn: &Value,
    since: Option<f64>,
    state: &str,
) -> bool {
    let baseline = row["runtime_key"].is_null();
    let terminal = terminal(status, turn);
    let event = turn[if terminal { "completedAt" } else { "startedAt" }].as_f64();
    let selected = row["selected_at"].as_f64();
    let older = event
        .zip(selected)
        .is_some_and(|(event, selected)| event <= selected);
    let manual = row["selected_state"].is_string()
        && (baseline && (turn["id"] == row["selection_turn"] || event.is_none() || older)
            || terminal && older);
    let old_failure = baseline
        && state == "blocked"
        && !matches!(status["type"].as_str(), Some("active" | "systemError"))
        && since
            .zip(turn["startedAt"].as_f64())
            .is_none_or(|(since, start)| start < since);
    manual || old_failure
}
