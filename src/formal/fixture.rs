use super::session::{Column, Phase, Seat, Session, Title};
use crate::store::Store;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Row};
use serde_json::{Value, json};

pub(super) const ID: &str = "session-1";

pub(super) struct Stored {
    pub(super) native_state: Option<String>,
    pub(super) native_revision: Option<String>,
    pub(super) runtime_key: Option<String>,
    pub(super) pending_key: Option<String>,
    pub(super) selected_state: Option<String>,
    pub(super) selected_at: Option<f64>,
    pub(super) choice_at: Option<f64>,
    pub(super) note: String,
    pub(super) review: i64,
    pub(super) archived: i64,
    pub(super) version: i64,
    pub(super) input_revision: i64,
    pub(super) turn_json: String,
}

pub(super) fn install(store: &Store, session: Session) -> Result<()> {
    clear(store)?;
    store.discover(&[json!({"id":ID,"name":"会话","cwd":"/work","updatedAt":1})])?;
    write_session(store, session)?;
    write_title(store, session)?;
    ensure!(
        project(store)? == session,
        "夹具没有还原 {session:?}，实际 {:?}",
        project(store)?
    );
    Ok(())
}

fn clear(store: &Store) -> Result<()> {
    let database = store.connection()?;
    database.execute("DELETE FROM title_updates", [])?;
    database.execute("DELETE FROM sessions", [])?;
    Ok(())
}

fn write_session(store: &Store, session: Session) -> Result<()> {
    store.connection()?.execute(
        "UPDATE sessions SET native_state=?2,native_revision=?3,runtime_key=?4,runtime_pending_key=?5,
         selected_state=?6,selected_at=?7,native_choice_at=?8,note=?9,needs_review=?10,archived=?11,
         version=1,input_revision=0,turn_json=?12,selection_turn=?13,selection_users=?14 WHERE id=?1",
        rusqlite::params![
            ID,
            native_state(session),
            revision(session.seat),
            session.phase.map(|phase| key("held", phase)),
            session.pending.map(|phase| key("pend", phase)),
            session.manual.then_some(session.column.as_str()),
            session.anchor.then_some(50.0),
            session.choice.then_some(80.0),
            if session.note { "记" } else { "" },
            i64::from(session.review),
            i64::from(session.archived),
            anchor_turn(session.anchor),
            session.anchor.then_some("turn-anchor"),
            if session.anchor { "[\"u\"]" } else { "[]" },
        ],
    )?;
    Ok(())
}

fn native_state(session: Session) -> Option<&'static str> {
    match session.seat {
        Seat::Unread => None,
        Seat::Movable | Seat::Custom => Some(session.column.as_str()),
    }
}

fn write_title(store: &Store, session: Session) -> Result<()> {
    let error = match session.title {
        Title::Idle => return Ok(()),
        Title::Pending => None,
        Title::Failed => Some("失败"),
    };
    store.connection()?.execute(
        "INSERT INTO title_updates(thread_id,version,state,requested_at,error) VALUES (?,?,?,?,?)",
        rusqlite::params![ID, 1, session.column.as_str(), 1.0, error],
    )?;
    Ok(())
}

fn anchor_turn(anchor: bool) -> String {
    if anchor {
        json!({"id":"turn-anchor","status":"completed","startedAt":50.0,"userItems":["u"]})
            .to_string()
    } else {
        "{}".into()
    }
}

pub(super) fn revision(seat: Seat) -> Option<&'static str> {
    match seat {
        Seat::Unread => None,
        Seat::Movable => Some("open"),
        Seat::Custom => Some("custom"),
    }
}

fn key(prefix: &str, phase: Phase) -> String {
    json!([
        format!("{prefix}-{}", phase.label()),
        phase.column().as_str(),
        flags(phase)
    ])
    .to_string()
}

pub(super) fn flags(phase: Phase) -> Value {
    match phase {
        Phase::Approval => json!(["waitingOnApproval"]),
        Phase::Input => json!(["waitingOnUserInput"]),
        Phase::Running | Phase::Error | Phase::Finished => json!([]),
    }
}

pub(super) fn project(store: &Store) -> Result<Session> {
    let stored = load(store)?;
    Ok(Session {
        column: stored
            .native_state
            .as_deref()
            .map(Column::parse)
            .transpose()?
            .unwrap_or(Column::Unknown),
        phase: phase_of(stored.runtime_key.as_deref())?,
        pending: phase_of(stored.pending_key.as_deref())?,
        manual: stored.selected_state.is_some(),
        seat: seat_of(stored.native_revision.as_deref())?,
        title: title_of(store)?,
        review: stored.review == 1,
        anchor: stored.selected_at.is_some(),
        choice: stored.choice_at.is_some(),
        note: !stored.note.trim().is_empty(),
        archived: stored.archived == 1,
    })
}

pub(super) fn load(store: &Store) -> Result<Stored> {
    store
        .connection()?
        .query_row(
            "SELECT native_state,native_revision,runtime_key,runtime_pending_key,selected_state,selected_at,native_choice_at,note,needs_review,archived,version,input_revision,turn_json FROM sessions WHERE id=?1",
            [ID],
            read_stored,
        )
        .map_err(Into::into)
}

fn read_stored(row: &Row<'_>) -> rusqlite::Result<Stored> {
    Ok(Stored {
        native_state: row.get(0)?,
        native_revision: row.get(1)?,
        runtime_key: row.get(2)?,
        pending_key: row.get(3)?,
        selected_state: row.get(4)?,
        selected_at: row.get(5)?,
        choice_at: row.get(6)?,
        note: row.get(7)?,
        review: row.get(8)?,
        archived: row.get(9)?,
        version: row.get(10)?,
        input_revision: row.get(11)?,
        turn_json: row.get(12)?,
    })
}

pub(super) fn seat_of(revision: Option<&str>) -> Result<Seat> {
    Ok(match revision {
        None => Seat::Unread,
        Some("open") => Seat::Movable,
        Some("custom") => Seat::Custom,
        Some(revision) => anyhow::bail!("未知原生分区 {revision}"),
    })
}

fn phase_of(key: Option<&str>) -> Result<Option<Phase>> {
    let Some(key) = key else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(key)?;
    let state = value[1].as_str().context("阶段缺少列")?;
    let flags = value[2].as_array().context("阶段缺少标记")?;
    if flags.iter().any(|flag| flag == "waitingOnApproval") {
        return Ok(Some(Phase::Approval));
    }
    if flags.iter().any(|flag| flag == "waitingOnUserInput") {
        return Ok(Some(Phase::Input));
    }
    Ok(Some(state_phase(state)?))
}

fn state_phase(state: &str) -> Result<Phase> {
    Ok(match state {
        "in_progress" => Phase::Running,
        "review" => Phase::Finished,
        "blocked" => Phase::Error,
        _ => anyhow::bail!("无法还原阶段 {state}"),
    })
}

fn title_of(store: &Store) -> Result<Title> {
    let error: Option<Option<String>> = store
        .connection()?
        .query_row(
            "SELECT error FROM title_updates WHERE thread_id=?1",
            [ID],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match error {
        None => Title::Idle,
        Some(None) => Title::Pending,
        Some(Some(_)) => Title::Failed,
    })
}

#[cfg(test)]
mod tests {
    use super::{install, load};
    use crate::store::Store;

    #[test]
    fn every_reachable_session_roundtrips_through_sqlite() {
        let states = crate::formal::explore::reachable().unwrap();
        assert_eq!(states.len(), 5922);
        let mut store = Store::open_memory(true).unwrap();
        store.native_sections = true;
        for session in states {
            install(&store, session).unwrap();
            let stored = load(&store).unwrap();
            assert_eq!(stored.version, 1);
            assert_eq!(stored.input_revision, 0);
            assert!(!stored.turn_json.is_empty());
        }
    }
}
