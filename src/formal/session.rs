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

    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Approval => "approval",
            Self::Input => "input",
            Self::Error => "error",
            Self::Finished => "finished",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Seat {
    Unread,
    Movable,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Title {
    Idle,
    Pending,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum When {
    Same,
    Fresh,
    Old,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Session {
    pub column: Column,
    pub phase: Option<Phase>,
    pub pending: Option<Phase>,
    pub manual: bool,
    pub seat: Seat,
    pub title: Title,
    pub review: bool,
    pub anchor: bool,
    pub choice: bool,
    pub note: bool,
    pub archived: bool,
}

impl Session {
    pub fn initial() -> Self {
        Self {
            column: Column::Unknown,
            phase: None,
            pending: None,
            manual: false,
            seat: Seat::Unread,
            title: Title::Idle,
            review: false,
            anchor: false,
            choice: false,
            note: false,
            archived: false,
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
    Input,
    TitleOk,
    TitleFail,
    Archive,
    StaleArchive,
    View,
    Restart,
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
        events.push(Event::Lost(phase));
    }
    events.extend([
        Event::Input,
        Event::TitleOk,
        Event::TitleFail,
        Event::Archive,
        Event::StaleArchive,
        Event::View,
        Event::Restart,
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
        Event::Lost(phase) => lost(session, phase),
        Event::Input => input(session),
        Event::TitleOk => title_ok(session),
        Event::TitleFail => title_fail(session),
        Event::Archive => archive(session),
        Event::StaleManual(_)
        | Event::StaleArchive
        | Event::View
        | Event::Restart
        | Event::Same => session,
    }
}

fn preserves(session: Session, phase: Phase, when: When) -> bool {
    when == When::Old
        && (session.phase.is_none() || phase.terminal() && (session.choice || session.manual))
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
    next
}

fn commit(session: Session, phase: Phase) -> Session {
    let mut next = session;
    next.phase = Some(phase);
    next.pending = None;
    next.column = phase.column();
    next.manual = false;
    next.choice = false;
    next.title = Title::Pending;
    next.seat = Seat::Movable;
    next
}

fn lost(session: Session, phase: Phase) -> Session {
    if session.archived || session.seat != Seat::Movable {
        return session;
    }
    let mut next = session;
    next.pending = Some(phase);
    next
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
    next.seat = seat;
    next.column = column;
    if confirmed {
        next.manual = false;
        next.choice = false;
    }
    if confirmed || session.seat != Seat::Unread || session.title != Title::Idle {
        next.title = Title::Pending;
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
    next.column = column;
    next.seat = Seat::Movable;
    next.manual = true;
    next.anchor = true;
    next.choice = true;
    next.review = false;
    next.title = Title::Pending;
    if column == Column::Blocked {
        next.note = true;
    }
    next
}

fn input(session: Session) -> Session {
    if session.archived {
        return session;
    }
    let mut next = session;
    if session.review || session.anchor || session.column == Column::ToArchive {
        next.review = true;
    }
    next
}

fn title_ok(session: Session) -> Session {
    let mut next = session;
    if session.title != Title::Idle {
        next.title = Title::Idle;
    }
    next
}

fn title_fail(session: Session) -> Session {
    let mut next = session;
    if session.title == Title::Pending {
        next.title = Title::Failed;
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
    next.title = Title::Idle;
    next.pending = None;
    next
}

pub fn invariant(session: Session) -> Result<()> {
    ensure!(!session.manual || session.anchor, "人工选择缺少确认锚点");
    ensure!(!session.choice || session.manual, "选择时间不是人工挪列留下的");
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
                && session.title == Title::Idle
                && !session.manual
                && !session.review
                && !session.anchor
                && !session.choice
                && !session.note
                && !session.archived,
            "未读会话带有分区读回才会产生的标记"
        );
    }
    Ok(())
}
