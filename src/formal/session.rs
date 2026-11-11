use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Unknown,
    InProgress,
    Review,
    Blocked,
    ToArchive,
}

impl Column {
    pub const ALL: [Self; 5] = [
        Self::Unknown,
        Self::InProgress,
        Self::Review,
        Self::Blocked,
        Self::ToArchive,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Blocked => "blocked",
            Self::ToArchive => "to_archive",
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|column| column.as_str() == text)
            .ok_or_else(|| anyhow::anyhow!("未知列 {text}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Running,
    Approval,
    Input,
    Error,
    Finished,
}

impl Phase {
    pub const ALL: [Self; 5] = [
        Self::Running,
        Self::Approval,
        Self::Input,
        Self::Error,
        Self::Finished,
    ];

    pub fn column(self) -> Column {
        match self {
            Self::Running => Column::InProgress,
            Self::Finished => Column::Review,
            Self::Approval | Self::Input | Self::Error => Column::Blocked,
        }
    }

    pub fn terminal(self) -> bool {
        matches!(self, Self::Error | Self::Finished)
    }

}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Seat {
    Unread,
    Movable,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum When {
    Same,
    Fresh,
    Old,
    BeforeObservation,
    BeforeChoice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Session {
    pub column: Column,
    pub phase: Option<Phase>,
    pub pending: Option<Phase>,
    pub pending_same_turn: bool,
    pub manual: bool,
    pub seat: Seat,
    pub review: bool,
    pub anchor: bool,
    pub archived: bool,
    pub has_input: bool,
}

impl Session {
    pub fn initial() -> Self {
        Self {
            column: Column::Unknown,
            phase: None,
            pending: None,
            pending_same_turn: false,
            manual: false,
            seat: Seat::Unread,
            review: false,
            anchor: false,
            archived: false,
            has_input: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    Read(Column),
    ReadCustom,
    Manual(Column),
    StaleManual(Column),
    Runtime(Phase, When),
    Lost(Phase),
    LostSameTurn(Phase),
    Retry(Phase),
    Input,
    Archive,
    StaleArchive,
    View,
    Same,
}

pub fn events() -> Vec<Event> {
    let mut events = Vec::new();
    for column in Column::ALL {
        events.push(Event::Read(column));
        events.push(Event::Manual(column));
        events.push(Event::StaleManual(column));
    }
    events.push(Event::ReadCustom);
    for phase in Phase::ALL {
        events.push(Event::Runtime(phase, When::Fresh));
        events.push(Event::Runtime(phase, When::Old));
        events.push(Event::Runtime(phase, When::BeforeObservation));
        events.push(Event::Runtime(phase, When::BeforeChoice));
        events.push(Event::Lost(phase));
        events.push(Event::LostSameTurn(phase));
        events.push(Event::Retry(phase));
    }
    events.extend([
        Event::Input,
        Event::Archive,
        Event::StaleArchive,
        Event::View,
        Event::Same,
    ]);
    events
}

pub fn step(session: Session, event: Event) -> Session {
    match event {
        Event::Read(column) => read(session, Seat::Movable, column),
        Event::ReadCustom => read(session, Seat::Custom, Column::Unknown),
        Event::Manual(column) => manual(session, column),
        Event::Runtime(phase, when) => runtime(session, phase, when),
        Event::Lost(phase) => lost(session, phase, false),
        Event::LostSameTurn(phase) => lost(session, phase, true),
        Event::Retry(phase) => retry(session, phase),
        Event::Input => input(session),
        Event::Archive => archive(session),
        Event::StaleManual(_)
        | Event::StaleArchive
        | Event::View
        | Event::Same => session,
    }
}

pub(super) fn preserves(session: Session, phase: Phase, when: When) -> bool {
    session.phase.is_none() && matches!(when, When::Old | When::BeforeObservation)
        || phase.terminal() && session.manual && matches!(when, When::Old | When::BeforeChoice)
}

fn runtime(session: Session, phase: Phase, when: When) -> Session {
    if when == When::Same || session.archived {
        return session;
    }
    if session.seat == Seat::Unread {
        return unread_runtime(session, phase, when);
    }
    if preserves(session, phase, when) || session.seat == Seat::Custom {
        let mut next = session;
        next.phase = Some(phase);
        next.pending = None;
        next.pending_same_turn = false;
        return next;
    }
    commit(session, phase)
}

fn unread_runtime(session: Session, phase: Phase, when: When) -> Session {
    if !preserves(session, phase, when) {
        return session;
    }
    let mut next = session;
    next.phase = Some(phase);
    next.pending = None;
    next.pending_same_turn = false;
    next
}

fn commit(session: Session, phase: Phase) -> Session {
    let mut next = session;
    next.phase = Some(phase);
    next.pending = None;
    next.pending_same_turn = false;
    next.column = phase.column();
    next.manual = false;
    next.seat = Seat::Movable;
    next
}

fn lost(session: Session, phase: Phase, same_turn: bool) -> Session {
    if session.archived || session.seat != Seat::Movable
        || same_turn && (session.phase.is_none() || session.phase == Some(phase))
    {
        return session;
    }
    let mut next = session;
    next.pending = Some(phase);
    next.pending_same_turn = same_turn;
    next
}

fn retry(session: Session, phase: Phase) -> Session {
    if session.pending.is_none()
        || session.pending_same_turn && session.phase == Some(phase)
    {
        return session;
    }
    commit(session, phase)
}

fn read(session: Session, seat: Seat, column: Column) -> Session {
    if session.archived || session.seat == seat && session.column == column {
        return session;
    }
    let confirmed = session
        .pending
        .is_some_and(|phase| phase.column() == column);
    let mut next = session;
    if let Some(phase) = session.pending {
        next.phase = Some(phase);
    }
    next.pending = None;
    next.pending_same_turn = false;
    next.seat = seat;
    next.column = column;
    next.review |= column == Column::ToArchive && !session.anchor && session.has_input;
    if confirmed {
        next.manual = false;
    }
    next
}

fn manual(session: Session, column: Column) -> Session {
    if session.archived || session.seat == Seat::Unread {
        return session;
    }
    let mut next = session;
    if let Some(phase) = session.pending {
        next.phase = Some(phase);
    }
    next.pending = None;
    next.pending_same_turn = false;
    next.column = column;
    next.seat = Seat::Movable;
    next.manual = true;
    next.anchor = true;
    next.review = false;
    next
}

fn input(session: Session) -> Session {
    if session.archived {
        return session;
    }
    let mut next = session;
    next.has_input = true;
    if session.review || session.anchor || session.column == Column::ToArchive {
        next.review = true;
    }
    next
}

fn archive(session: Session) -> Session {
    if session.archived
        || session.review
        || session.column != Column::ToArchive
        || session.seat == Seat::Unread
    {
        return session;
    }
    let mut next = session;
    next.archived = true;
    next.pending = None;
    next.pending_same_turn = false;
    next
}

pub fn invariant(session: Session) -> Result<()> {
    ensure!(!session.review || session.has_input, "复核标记缺少输入");
    ensure!(
        !session.pending_same_turn || session.pending.is_some()
            && session.phase.is_some() && session.pending != session.phase,
        "同轮待确认阶段缺少不同的已确认阶段"
    );
    ensure!(!session.manual || session.anchor, "人工选择缺少确认锚点");
    ensure!(
        !session.archived || session.pending.is_none(),
        "已归档会话仍有待确认阶段"
    );
    ensure!(
        session.pending.is_none() || session.seat == Seat::Movable,
        "未确认阶段出现在不可移动的位置"
    );
    if session.seat == Seat::Unread {
        ensure!(
            session.column == Column::Unknown
                && session.pending.is_none()
                && !session.manual
                && !session.review
                && !session.anchor
                && !session.archived,
            "未读会话带有分区读回才会产生的标记"
        );
    }
    Ok(())
}

pub fn version_delta(before: Session, event: Event, after: Session) -> i64 {
    let changes = match event {
        Event::Read(_) | Event::ReadCustom => {
            (before.seat, before.column) != (after.seat, after.column)
        }
        Event::Manual(_) => !before.archived && before.seat != Seat::Unread,
        Event::Runtime(phase, when) => !before.archived && before.seat == Seat::Movable
            && when != When::Same && !preserves(before, phase, when),
        Event::Retry(phase) => before.pending.is_some()
            && !(before.pending_same_turn && before.phase == Some(phase)),
        Event::Archive => !before.archived && after.archived,
        _ => false,
    };
    i64::from(changes)
}
