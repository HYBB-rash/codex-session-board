use super::fixture::{ID, flags, install, load, project, revision, seat_of};
use super::session::{Column, Event, Phase, Seat, Session, When, events, step};
use crate::native_sections::Observation;
use crate::store::{RuntimeChange, Store};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub fn conform(states: &[Session]) -> Result<usize> {
    let mut store = Store::open_memory(true)?;
    store.native_sections = true;
    let mut edges = 0;
    for session in states {
        for event in events() {
            install(&store, *session)?;
            apply(&store, event)?;
            ensure!(
                project(&store)? == step(*session, event),
                "状态 {session:?}\n事件 {event:?}\n模型 {:?}\n数据库 {:?}",
                step(*session, event),
                project(&store)?
            );
            edges += 1;
        }
    }
    Ok(edges)
}

pub fn store_can_write_before_a_section_is_cached() -> Result<()> {
    let mut store = Store::open_memory(true)?;
    store.native_sections = true;
    install(&store, Session::initial())?;
    store.set_native_state(
        ID,
        Some("review"),
        "",
        (1, 0),
        &Observation {
            state: "review".into(),
            version: "open".into(),
        },
    )?;
    ensure!(
        project(&store)?.seat == Seat::Movable,
        "直接写库没有落下原生列"
    );
    Ok(())
}

fn apply(store: &Store, event: Event) -> Result<()> {
    match event {
        Event::Read(column) => read(store, Seat::Movable, column),
        Event::ReadCustom => read(store, Seat::Custom, Column::Unknown),
        Event::Manual(column) => edit(store, column, false),
        Event::StaleManual(column) => edit(store, column, true),
        Event::Runtime(phase, when) => runtime(store, phase, when),
        Event::Lost(phase) => lost(store, phase),
        Event::Input => input(store),
        Event::TitleOk => title(store, true),
        Event::TitleFail => title(store, false),
        Event::Archive => archive(store, false),
        Event::StaleArchive => archive(store, true),
        Event::View => view(store),
        Event::Restart | Event::Same => same(store),
    }
}

fn read(store: &Store, seat: Seat, column: Column) -> Result<()> {
    store.cache_native(
        ID,
        &Observation {
            state: column.as_str().into(),
            version: revision(seat).context("读回缺少分区")?.into(),
        },
    )
}

fn edit(store: &Store, column: Column, stale: bool) -> Result<()> {
    let stored = load(store)?;
    if stored.native_revision.is_none() {
        return Ok(());
    }
    let version = if stale { 0 } else { stored.version };
    let note = if column == Column::Blocked || !stored.note.trim().is_empty() {
        "记"
    } else {
        ""
    };
    match store.set_native_state(
        ID,
        Some(column.as_str()),
        note,
        (version, stored.input_revision),
        &seen(column),
    ) {
        Ok(()) => Ok(()),
        Err(_) if stale || stored.archived == 1 => Ok(()),
        Err(error) => Err(error),
    }
}

fn seen(column: Column) -> Observation {
    Observation {
        state: column.as_str().into(),
        version: "open".into(),
    }
}

fn runtime(store: &Store, phase: Phase, when: When) -> Result<()> {
    let seat = seat_of(load(store)?.native_revision.as_deref())?;
    match prepare(store, phase, when) {
        Err(_) if seat == Seat::Unread => Ok(()),
        Err(error) => Err(error),
        Ok(None) => Ok(()),
        Ok(Some(change)) if seat == Seat::Custom => store.consume_native_runtime(ID, &change),
        Ok(Some(change)) => finish(store, &change),
    }
}

fn finish(store: &Store, change: &RuntimeChange) -> Result<()> {
    store.commit_native_runtime(
        ID,
        change,
        &Observation {
            state: change.state.into(),
            version: "open".into(),
        },
    )
}

fn lost(store: &Store, phase: Phase) -> Result<()> {
    let stored = load(store)?;
    let seat = seat_of(stored.native_revision.as_deref())?;
    if stored.archived == 1 {
        ensure!(
            prepare(store, phase, When::Fresh)?.is_none(),
            "已归档会话又开始移动"
        );
        return Ok(());
    }
    if seat == Seat::Unread {
        ensure!(
            prepare(store, phase, When::Fresh).is_err(),
            "未读会话开始了自动移动"
        );
        return Ok(());
    }
    if seat != Seat::Movable {
        return Ok(());
    }
    ensure!(
        prepare(store, phase, When::Fresh)?.is_some(),
        "可移动会话没有留下待确认阶段"
    );
    Ok(())
}

fn prepare(store: &Store, phase: Phase, when: When) -> Result<Option<RuntimeChange>> {
    let (turn, time, since) = match when {
        When::Fresh => (format!("fresh-{}", phase.label()), 1.0e12, 0.0),
        When::Old => (format!("old-{}", phase.label()), 1.0, 10_000.0),
        When::Same => (format!("held-{}", phase.label()), 1.0e12, 0.0),
    };
    let (status, body) = evidence(phase, &turn, time);
    store.prepare_native_runtime(ID, &status, &body, since)
}

fn evidence(phase: Phase, turn: &str, time: f64) -> (Value, Value) {
    let (kind, status) = match phase {
        Phase::Running | Phase::Approval | Phase::Input => ("active", "inProgress"),
        Phase::Error => ("idle", "failed"),
        Phase::Finished => ("idle", "completed"),
    };
    (
        json!({"type":kind,"activeFlags":flags(phase)}),
        json!({"id":turn,"status":status,"startedAt":time,"completedAt":time,"userItems":[]}),
    )
}

fn same(store: &Store) -> Result<()> {
    let before = project(store)?;
    let Some(phase) = before.phase else {
        return Ok(());
    };
    ensure!(
        prepare(store, phase, When::Same)?.is_none(),
        "重复快照被当成新阶段"
    );
    ensure!(project(store)? == before, "重复快照写了数据库");
    Ok(())
}

fn input(store: &Store) -> Result<()> {
    store.observe(
        ID,
        &json!({"id":"turn-input","status":"completed","startedAt":1.0e12,"completedAt":1.0e12,"userItems":["new"]}),
        None,
    )
}

fn view(store: &Store) -> Result<()> {
    let turn: Value = serde_json::from_str(&load(store)?.turn_json)?;
    store.observe(ID, &turn, None)
}

fn title(store: &Store, synced: bool) -> Result<()> {
    let version = load(store)?.version;
    if synced {
        store.title_synced(ID, version, "[待审阅] 会话")?;
        Ok(())
    } else {
        store.title_failed(ID, version, "失败")
    }
}

fn archive(store: &Store, stale: bool) -> Result<()> {
    let stored = load(store)?;
    let version = if stale { 0 } else { stored.version };
    if store
        .prepare_archive(ID, (version, stored.input_revision))
        .is_err()
    {
        return Ok(());
    }
    store.confirm_archive(ID, (version, stored.input_revision))
}
