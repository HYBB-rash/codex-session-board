use super::*;
use crate::native_sections::Observation;
use rusqlite::types::Value as SqlValue;

pub(super) enum Pending<'a> {
    Keep,
    Clear,
    Set(&'a str),
}

pub(super) enum Choice {
    Keep,
    Clear,
    Stamp(f64),
}

pub(super) enum Phase<'a> {
    Keep,
    Set(&'a str),
    Coalesce,
}

pub(super) struct Placement<'a> {
    pub state: Option<&'a str>,
    pub note: &'a str,
    pub turn: Option<&'a str>,
    pub users: &'a str,
    pub at: f64,
}

pub(super) struct DailyWrite<'a> {
    pub column: Option<&'a Observation>,
    pub pending: Pending<'a>,
    pub choice: Choice,
    pub phase: Phase<'a>,
    pub clear_selected: bool,
    pub automatic: Option<(&'a str, &'a str)>,
    pub needs_review: Option<i64>,
    pub archive: bool,
    pub unarchive: bool,
    pub bump_version: bool,
    pub placement: Option<Placement<'a>>,
    pub expect_version: Option<i64>,
    pub expect_pending: Option<&'a str>,
}

impl<'a> Default for DailyWrite<'a> {
    fn default() -> Self {
        Self {
            column: None,
            pending: Pending::Keep,
            choice: Choice::Keep,
            phase: Phase::Keep,
            clear_selected: false,
            automatic: None,
            needs_review: None,
            archive: false,
            unarchive: false,
            bump_version: false,
            placement: None,
            expect_version: None,
            expect_pending: None,
        }
    }
}

pub(super) fn write_daily(
    database: &Transaction<'_>,
    identifier: &str,
    write: &DailyWrite<'_>,
) -> Result<()> {
    if let Some(column) = write.column {
        ensure!(
            crate::model::board_states().any(|item| item.0 == column.state),
            "原生状态无效"
        );
    }
    let (assignments, mut values) = assignments(write);
    if assignments.is_empty() {
        return Ok(());
    }
    let guard = guard_sql(write);
    values.push(SqlValue::Text(identifier.into()));
    push_guard(&mut values, write);
    database.execute(
        &format!("UPDATE sessions SET {} WHERE id=?{guard}", assignments.join(",")),
        rusqlite::params_from_iter(values.iter()),
    )?;
    Ok(())
}

fn assignments(write: &DailyWrite<'_>) -> (Vec<String>, Vec<SqlValue>) {
    let mut sql = Vec::new();
    let mut values = Vec::new();
    if let Some(column) = write.column {
        push(&mut sql, &mut values, "native_state", SqlValue::Text(column.state.clone()));
        push(&mut sql, &mut values, "native_revision", SqlValue::Text(column.version.clone()));
    }
    match &write.pending {
        Pending::Keep => {}
        Pending::Clear => sql.push("runtime_pending_key=NULL".into()),
        Pending::Set(key) => push(&mut sql, &mut values, "runtime_pending_key", SqlValue::Text((*key).into())),
    }
    match &write.choice {
        Choice::Keep => {}
        Choice::Clear => sql.push("native_choice_at=NULL".into()),
        Choice::Stamp(time) => push(&mut sql, &mut values, "native_choice_at", SqlValue::Real(*time)),
    }
    match &write.phase {
        Phase::Keep => {}
        Phase::Set(key) => push(&mut sql, &mut values, "runtime_key", SqlValue::Text((*key).into())),
        Phase::Coalesce => sql.push("runtime_key=COALESCE(runtime_pending_key,runtime_key)".into()),
    }
    selection(&mut sql, &mut values, write);
    if write.archive {
        sql.push("archived=1".into());
    }
    if write.unarchive {
        sql.push("archived=0".into());
    }
    if write.bump_version {
        sql.push("version=version+1".into());
    }
    (sql, values)
}

fn selection(sql: &mut Vec<String>, values: &mut Vec<SqlValue>, write: &DailyWrite<'_>) {
    if let Some(place) = &write.placement {
        push(sql, values, "selected_state", text_or_null(place.state));
        push(sql, values, "note", SqlValue::Text(place.note.into()));
        push(sql, values, "selection_turn", text_or_null(place.turn));
        push(sql, values, "selection_users", SqlValue::Text(place.users.into()));
        push(sql, values, "selected_at", SqlValue::Real(place.at));
        sql.push("needs_review=0".into());
        return;
    }
    if write.clear_selected {
        sql.push("selected_state=NULL".into());
    }
    if let Some((state, reason)) = write.automatic {
        push(sql, values, "automatic_state", SqlValue::Text(state.into()));
        push(sql, values, "automatic_reason", SqlValue::Text(reason.into()));
    }
    if let Some(review) = write.needs_review {
        push(sql, values, "needs_review", SqlValue::Integer(review));
    }
}

fn push(sql: &mut Vec<String>, values: &mut Vec<SqlValue>, column: &str, value: SqlValue) {
    sql.push(format!("{column}=?"));
    values.push(value);
}

fn text_or_null(value: Option<&str>) -> SqlValue {
    match value {
        Some(text) => SqlValue::Text(text.into()),
        None => SqlValue::Null,
    }
}

fn guard_sql(write: &DailyWrite<'_>) -> String {
    let mut guard = String::new();
    if write.expect_version.is_some() {
        guard.push_str(" AND version=?");
    }
    if write.expect_pending.is_some() {
        guard.push_str(" AND runtime_pending_key=?");
    }
    guard
}

fn push_guard(values: &mut Vec<SqlValue>, write: &DailyWrite<'_>) {
    if let Some(version) = write.expect_version {
        values.push(SqlValue::Integer(version));
    }
    if let Some(pending) = write.expect_pending {
        values.push(SqlValue::Text(pending.into()));
    }
}

fn observed_write<'a>(row: &Value, native: &'a Observation) -> Result<DailyWrite<'a>> {
    let confirmed = super::runtime_native::pending(row)?.is_some_and(|key| key[1] == native.state);
    let unconfirmed_input = native.state == "to_archive"
        && row["selected_at"].is_null()
        && !users(&turn_of(row)).is_empty();
    Ok(DailyWrite {
        column: Some(native),
        pending: Pending::Clear,
        choice: if confirmed { Choice::Clear } else { Choice::Keep },
        phase: Phase::Coalesce,
        clear_selected: confirmed,
        automatic: confirmed.then_some((native.state.as_str(), "自动阶段目标已在原生位置确认。")),
        needs_review: unconfirmed_input.then_some(1),
        bump_version: true,
        ..DailyWrite::default()
    })
}

fn record_observation(
    database: &Transaction<'_>,
    identifier: &str,
    row: &Value,
    native: &Observation,
) -> Result<()> {
    apply_observation(database, identifier, row, native, false)
}

fn apply_observation(
    database: &Transaction<'_>,
    identifier: &str,
    row: &Value,
    native: &Observation,
    unarchive: bool,
) -> Result<()> {
    let mut write = observed_write(row, native)?;
    write.unarchive = unarchive;
    let confirmed = write.clear_selected;
    write_daily(database, identifier, &write)?;
    if confirmed {
        database.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier, now(), native.state, "自动阶段目标已在原生位置确认。"],
        )?;
    }
    Ok(())
}

impl Store {
    pub fn prepare_archive(&self, identifier: &str, revision: (i64, i64)) -> Result<Value> {
        let database = self.connection()?;
        archive_expected(&get_row(&database, identifier)?, revision, self.native_sections)
    }

    pub fn confirm_archive(&self, identifier: &str, revision: (i64, i64)) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        archive_expected(&get_row(&transaction, identifier)?, revision, self.native_sections)?;
        confirm_archive(&transaction, identifier)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn native_version(&self, identifier: &str) -> Result<String> {
        let database = self.connection()?;
        get_row(&database, identifier)?["native_revision"]
            .as_str()
            .map(str::to_owned)
            .context("尚未确认原生分区，请稍后刷新")
    }

    pub fn archived_ids(&self) -> Result<Vec<String>> {
        let database = self.connection()?;
        let mut query = database.prepare("SELECT id FROM sessions WHERE archived=1")?;
        Ok(query
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn restore_native(&self, identifier: &str, native: &Observation) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        if row["archived"] != 1 {
            return Ok(());
        }
        apply_observation(&transaction, identifier, &row, native, true)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn cache_native(&self, identifier: &str, native: &Observation) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = get_row(&transaction, identifier)?;
        if row["archived"] == 1
            || row["native_revision"] == native.version && row["native_state"] == native.state
        {
            return Ok(());
        }
        let pending: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM title_updates WHERE thread_id=?)",
            [identifier],
            |row| row.get(0),
        )?;
        record_observation(&transaction, identifier, &row, native)?;
        if row["native_revision"].is_string() || pending {
            queue_title(&transaction, identifier, self.sync_titles, true)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn validate_state(
        &self,
        identifier: &str,
        state: Option<&str>,
        note: &str,
        revision: (i64, i64),
    ) -> Result<()> {
        let database = self.connection()?;
        let row = get_row(&database, identifier)?;
        validate_selection(&row, state, note, (revision.0, Some(revision.1)))
    }

    pub fn set_native_state(
        &self,
        identifier: &str,
        state: Option<&str>,
        note: &str,
        revision: (i64, i64),
        native: &Observation,
    ) -> Result<()> {
        self.set_state_inner(identifier, state, note, (revision.0, Some(revision.1)), Some(native))
    }
}
