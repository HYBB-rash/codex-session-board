use super::*;
use crate::native_sections::Observation;

pub(super) fn cache(
    database: &Transaction<'_>,
    identifier: &str,
    native: &Observation,
) -> Result<()> {
    ensure!(
        crate::model::board_states().any(|item| item.0 == native.state),
        "原生状态无效"
    );
    database.execute(
        "UPDATE sessions SET native_state=?,native_revision=? WHERE id=?",
        params![native.state, native.version, identifier],
    )?;
    Ok(())
}

fn record_observation(
    database: &Transaction<'_>,
    identifier: &str,
    row: &Value,
    native: &Observation,
) -> Result<()> {
    let confirmed = super::runtime_native::pending(row)?
        .is_some_and(|key| key[1] == native.state);
    let unconfirmed_input = native.state == "to_archive"
        && row["selected_at"].is_null()
        && !users(&turn_of(row)).is_empty();
    cache(database, identifier, native)?;
    database.execute(
        "UPDATE sessions SET native_choice_at=CASE WHEN ? THEN NULL ELSE native_choice_at END,
         needs_review=CASE WHEN ? THEN 1 ELSE needs_review END,
         version=version+1,runtime_key=COALESCE(runtime_pending_key,runtime_key),runtime_pending_key=NULL,
         selected_state=CASE WHEN ? THEN NULL ELSE selected_state END,
         automatic_state=CASE WHEN ? THEN ? ELSE automatic_state END,
         automatic_reason=CASE WHEN ? THEN '自动阶段目标已在原生位置确认。' ELSE automatic_reason END WHERE id=?",
        params![confirmed,unconfirmed_input,confirmed,confirmed,native.state,confirmed,identifier],
    )?;
    if confirmed {
        database.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier,now(),native.state,"自动阶段目标已在原生位置确认。"],
        )?;
    }
    Ok(())
}

impl Store {
    pub fn prepare_archive(&self, identifier: &str, revision: (i64, i64)) -> Result<Value> {
        let database = self.connection()?;
        archive_expected(
            &get_row(&database, identifier)?,
            revision,
            self.native_sections,
        )
    }

    pub fn confirm_archive(&self, identifier: &str, revision: (i64, i64)) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        archive_expected(
            &get_row(&transaction, identifier)?,
            revision,
            self.native_sections,
        )?;
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
        self.set_state_inner(
            identifier,
            state,
            note,
            (revision.0, Some(revision.1)),
            Some(native),
        )
    }
}
