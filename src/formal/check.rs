use super::session::{Event, Seat, Session, When};
use anyhow::{Result, ensure};

pub(super) fn rejection<Output>(result: Result<Output>, expected: &str) -> Result<()> {
    match result {
        Err(error) if error.chain().count() == 1 && error.to_string() == expected => Ok(()),
        Err(error) => Err(error),
        Ok(_) => anyhow::bail!("应拒绝的操作返回成功: {expected}"),
    }
}

pub fn transition(before: Session, event: Event, after: Session) -> Result<()> {
    identity_events(before, event, after)?;
    lost_keeps_column(before, event, after)?;
    fresh_rules(before, event, after)?;
    old_baseline(before, event, after)?;
    placed_by_hand(before, event, after)
}

fn placed_by_hand(before: Session, event: Event, after: Session) -> Result<()> {
    let chosen = matches!(event, Event::Manual(_));
    ensure!(
        !after.manual || before.manual || chosen,
        "读回或阶段写入制造了人工选择"
    );
    Ok(())
}

fn identity_events(before: Session, event: Event, after: Session) -> Result<()> {
    let identity = matches!(
        event,
        Event::Same | Event::View | Event::StaleManual(_) | Event::StaleArchive
    );
    ensure!(!identity || before == after, "应保持原状态的事件改变了会话");
    Ok(())
}

fn lost_keeps_column(before: Session, event: Event, after: Session) -> Result<()> {
    let (phase, same_turn) = match event {
        Event::Lost(phase) => (phase, false),
        Event::LostSameTurn(phase) => (phase, true),
        _ => return Ok(()),
    };
    if before.archived || before.seat != Seat::Movable
        || same_turn && (before.phase.is_none() || before.phase == Some(phase))
    {
        ensure!(before == after, "未发出的移动改变了会话");
        return Ok(());
    }
    ensure!(
        after.column == before.column && after.phase == before.phase,
        "丢失回执后已确认状态被改写"
    );
    ensure!(after.pending == Some(phase), "丢失回执后没有留下待确认阶段");
    ensure!(after.pending_same_turn == same_turn, "待确认轮次关系丢失");
    Ok(())
}

fn fresh_rules(before: Session, event: Event, after: Session) -> Result<()> {
    let Event::Runtime(phase, When::Fresh) = event else {
        return Ok(());
    };
    if before.archived || before.seat == Seat::Unread {
        ensure!(before == after, "无法移动时新阶段改变了会话");
        return Ok(());
    }
    ensure!(
        after.pending.is_none() && after.phase == Some(phase),
        "新阶段没有写回"
    );
    if before.seat == Seat::Custom {
        ensure!(after.column == before.column, "自定义分区被自动移走");
        return Ok(());
    }
    ensure!(
        after.column == phase.column() && !after.manual,
        "新阶段没有更新可移动列"
    );
    Ok(())
}

fn old_baseline(before: Session, event: Event, after: Session) -> Result<()> {
    let Event::Runtime(phase, When::Old | When::BeforeObservation) = event else {
        return Ok(());
    };
    if before.archived || before.phase.is_some() {
        return Ok(());
    }
    ensure!(
        after.column == before.column && after.phase == Some(phase) && after.pending.is_none(),
        "首次旧阶段改写了列或留下待确认移动"
    );
    ensure!(after.manual == before.manual, "首次旧阶段清除了人工选择");
    Ok(())
}
