use super::fixture::{ID, flags, install, load, project, revision, seat_of, turn_id};
use super::session::{Column, Event, Phase, Seat, Session, When, events, step, version_delta};
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
            let row = load(&store)?;
            ensure!(row.version == 1 + version_delta(*session, event, step(*session, event)),
                "{session:?} {event:?}: 状态版本增量错误");
            ensure!(row.input_revision == i64::from(matches!(event, Event::Input) && !session.archived),
                "{session:?} {event:?}: 输入版本增量错误");
            edges += 1;
        }
    }
    Ok(edges)
}

pub fn store_refuses_a_column_before_a_section_is_cached() -> Result<()> {
    let mut store = Store::open_memory(true)?;
    store.native_sections = true;
    install(&store, Session::initial())?;
    let refused = store.set_native_state(
        ID,
        Some("review"),
        "",
        (1, 0),
        &Observation {
            state: "review".into(),
            version: "open".into(),
        },
    );
    match refused {
        Err(error) => ensure!(error.to_string().contains("缺少原生位置"), "{error}"),
        Ok(()) => anyhow::bail!("未读会话被直接写成有列"),
    }
    ensure!(
        project(&store)?.seat == Seat::Unread && project(&store)?.column == Column::Unknown,
        "拒绝写入后未读会话带上了列"
    );
    Ok(())
}

pub(super) fn apply(store: &Store, event: Event) -> Result<()> {
    match event {
        Event::Read(column) => read(store, Seat::Movable, column),
        Event::ReadCustom => read(store, Seat::Custom, Column::Unknown),
        Event::Manual(column) => edit(store, column, false),
        Event::StaleManual(column) => edit(store, column, true),
        Event::Runtime(phase, when) => runtime(store, phase, when),
        Event::Lost(phase) => lost(store, phase, false),
        Event::LostSameTurn(phase) => lost(store, phase, true),
        Event::Retry(phase) => retry(store, phase),
        Event::Input => input(store),
        Event::Archive => archive(store, false),
        Event::StaleArchive => archive(store, true),
        Event::View => view(store),
        Event::Same => same(store),
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
    let version = if stale { 0 } else { stored.version };
    let note = if column == Column::Blocked || !stored.note.trim().is_empty() {
        "记"
    } else {
        ""
    };
    let result = store.set_native_state(
        ID,
        Some(column.as_str()),
        note,
        (version, stored.input_revision),
        &seen(column),
    );
    let accepts = !stale && stored.archived == 0 && stored.native_section.is_some();
    ensure!(result.is_ok() == accepts, "人工选择返回结果错误: {result:?}");
    if !accepts {
        ensure!(load(store)? == stored, "拒绝人工选择后持久记录改变");
    }
    Ok(())
}

fn seen(column: Column) -> Observation {
    Observation {
        state: column.as_str().into(),
        version: "open".into(),
    }
}

fn runtime(store: &Store, phase: Phase, when: When) -> Result<()> {
    let seat = seat_of(load(store)?.native_section.as_deref())?;
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

fn lost(store: &Store, phase: Phase, same_turn: bool) -> Result<()> {
    let stored = load(store)?;
    let seat = seat_of(stored.native_section.as_deref())?;
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
    if same_turn {
        return lost_same_turn(store, phase, stored.runtime_key.as_deref());
    }
    ensure!(
        prepare(store, phase, When::Fresh)?.is_some(),
        "可移动会话没有留下待确认阶段"
    );
    Ok(())
}

fn lost_same_turn(store: &Store, phase: Phase, key: Option<&str>) -> Result<()> {
    let Some(turn) = turn_id(key)? else { return Ok(()); };
    let (status, body) = evidence(phase, &turn, 1.0e12);
    let change = store.prepare_native_runtime(ID, &status, &body, 0.0)?;
    ensure!(change.is_some() == (project(store)?.phase != Some(phase)),
        "同轮阶段去重或待确认结果错误");
    Ok(())
}

fn prepare(store: &Store, phase: Phase, when: When) -> Result<Option<RuntimeChange>> {
    let stored = load(store)?;
    let held = turn_id(stored.runtime_key.as_deref())?;
    let pending = turn_id(stored.pending_key.as_deref())?;
    let mut turn = "next".to_string();
    while held.as_ref() == Some(&turn) || pending.as_ref() == Some(&turn) {
        turn.push('-');
    }
    if when == When::Same {
        let Some(held) = held else { return Ok(None); };
        turn = held;
    }
    let choice = stored.choice_at.unwrap_or(50.0);
    let (time, since) = match when {
        When::Fresh | When::Same => (1.0e12, 0.0),
        When::Old => (1.0, 1.0e12 + 1.0),
        When::BeforeObservation => (choice + 1.0, 1.0e12 + 1.0),
        When::BeforeChoice => (choice - 1.0, 0.0),
    };
    let (status, body) = evidence(phase, &turn, time);
    store.prepare_native_runtime(ID, &status, &body, since)
}

fn retry(store: &Store, phase: Phase) -> Result<()> {
    let stored = load(store)?;
    let Some(turn) = turn_id(stored.pending_key.as_deref())? else { return Ok(()); };
    let (status, body) = evidence(phase, &turn, 1.0e12);
    if let Some(change) = store.prepare_native_runtime(ID, &status, &body, 1.0e12 + 1.0)? {
        finish(store, &change)?;
    }
    Ok(())
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
    let next = load(store)?.input_revision + 1;
    store.observe(
        ID,
        &json!({"id":format!("input-{next}"),"status":"completed","startedAt":1.0e12,"completedAt":1.0e12,"userItems":[format!("new-{next}")]}),
        None,
    )
}

fn view(store: &Store) -> Result<()> {
    let turn: Value = serde_json::from_str(&load(store)?.turn_json)?;
    store.observe(ID, &turn, None)
}

fn archive(store: &Store, stale: bool) -> Result<()> {
    let stored = load(store)?;
    let version = if stale { 0 } else { stored.version };
    let accepts = !stale && stored.archived == 0 && stored.review == 0
        && stored.native_state.as_deref() == Some("to_archive");
    let prepared = store.prepare_archive(ID, (version, stored.input_revision));
    ensure!(prepared.is_ok() == accepts, "归档准备返回结果错误: {prepared:?}");
    if let Ok(turn) = prepared {
        ensure!(turn == serde_json::from_str::<Value>(&stored.turn_json)?,
            "归档准备返回了错误的输入依据");
    }
    ensure!(load(store)? == stored, "归档准备修改了持久记录");
    let result = store.confirm_archive(ID, (version, stored.input_revision));
    ensure!(result.is_ok() == accepts, "归档确认返回结果错误: {result:?}");
    if !accepts {
        ensure!(load(store)? == stored, "拒绝归档后持久记录改变");
    }
    Ok(())
}
