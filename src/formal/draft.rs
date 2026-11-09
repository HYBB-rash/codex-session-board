use crate::store::Store;
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde_json::Value;

const PROJECT: &str = "project-1";
const REQUEST: &str = "11111111-1111-4111-8111-111111111111";
const THREAD: &str = "22222222-2222-4222-8222-222222222222";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Draft {
    Absent,
    Starting,
    Created,
    Threaded,
    Complete,
    Abandoned,
}

#[derive(Clone, Copy, Debug)]
enum DraftEvent {
    Begin,
    Mark,
    Attach,
    Complete,
    Abandon,
}

const DRAFT_EVENTS: [DraftEvent; 5] = [
    DraftEvent::Begin,
    DraftEvent::Mark,
    DraftEvent::Attach,
    DraftEvent::Complete,
    DraftEvent::Abandon,
];

fn draft_step(draft: Draft, event: DraftEvent) -> Draft {
    match (draft, event) {
        (Draft::Absent, DraftEvent::Begin) => Draft::Starting,
        (_, DraftEvent::Begin) => draft,
        (Draft::Starting, DraftEvent::Mark) => Draft::Created,
        (Draft::Created, DraftEvent::Mark) | (Draft::Threaded, DraftEvent::Mark) => draft,
        (Draft::Starting | Draft::Created, DraftEvent::Attach) => Draft::Threaded,
        (Draft::Threaded, DraftEvent::Attach) => Draft::Threaded,
        (Draft::Starting | Draft::Created | Draft::Threaded, DraftEvent::Complete) => {
            Draft::Complete
        }
        (Draft::Starting | Draft::Created | Draft::Threaded, DraftEvent::Abandon) => {
            Draft::Abandoned
        }
        _ => draft,
    }
}

pub fn conform_drafts() -> Result<usize> {
    let mut seen = std::collections::HashSet::from([Draft::Absent]);
    let mut queue = std::collections::VecDeque::from([Draft::Absent]);
    let mut edges = 0;
    while let Some(draft) = queue.pop_front() {
        for event in DRAFT_EVENTS {
            let next = draft_step(draft, event);
            replay(draft, event, next)?;
            edges += 1;
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    ensure!(seen.len() == 6, "新建阶段应为 6 个，实际 {}", seen.len());
    Ok(edges)
}

fn replay(draft: Draft, event: DraftEvent, expected: Draft) -> Result<()> {
    let store = Store::open_memory(true)?;
    install(&store, draft)?;
    apply(&store, draft, event)?;
    ensure!(
        project(&store)? == expected,
        "新建 {draft:?} 经 {event:?} 模型 {expected:?} 数据库 {:?}",
        project(&store)?
    );
    Ok(())
}

fn install(store: &Store, draft: Draft) -> Result<()> {
    if draft == Draft::Absent {
        return Ok(());
    }
    store.draft_begin(PROJECT, REQUEST)?;
    match draft {
        Draft::Absent | Draft::Starting => Ok(()),
        Draft::Created => store.draft_update(REQUEST, None, "created").map(|_| ()),
        Draft::Threaded => store
            .draft_update(REQUEST, Some(THREAD), "created")
            .map(|_| ()),
        Draft::Complete => store
            .draft_update(REQUEST, Some(THREAD), "complete")
            .map(|_| ()),
        Draft::Abandoned => store.draft_update(REQUEST, None, "abandoned").map(|_| ()),
    }
}

fn apply(store: &Store, draft: Draft, event: DraftEvent) -> Result<()> {
    let result = match event {
        DraftEvent::Begin => store.draft_begin(PROJECT, REQUEST).map(|_| ()),
        DraftEvent::Mark => store.draft_update(REQUEST, None, "created").map(|_| ()),
        DraftEvent::Attach => store
            .draft_update(REQUEST, Some(THREAD), "created")
            .map(|_| ()),
        DraftEvent::Complete => complete(store, draft),
        DraftEvent::Abandon => store.draft_update(REQUEST, None, "abandoned").map(|_| ()),
    };
    match result {
        Ok(()) => Ok(()),
        Err(_) if draft_step(draft, event) == draft => Ok(()),
        Err(error) => Err(error),
    }
}

fn complete(store: &Store, draft: Draft) -> Result<()> {
    let thread = match draft {
        Draft::Threaded => None,
        Draft::Starting | Draft::Created => Some(THREAD),
        Draft::Absent | Draft::Complete | Draft::Abandoned => None,
    };
    store.draft_update(REQUEST, thread, "complete").map(|_| ())
}

fn project(store: &Store) -> Result<Draft> {
    let value: Option<String> = store
        .connection()?
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [format!("draft_request:{REQUEST}")],
            |row| row.get(0),
        )
        .optional()?;
    let Some(value) = value else {
        return Ok(Draft::Absent);
    };
    let record: Value = serde_json::from_str(&value)?;
    Ok(decode(&record))
}

fn decode(record: &Value) -> Draft {
    match record["stage"].as_str() {
        Some("starting") => Draft::Starting,
        Some("created") if record["threadId"].is_string() => Draft::Threaded,
        Some("created") => Draft::Created,
        Some("complete") => Draft::Complete,
        Some("abandoned") => Draft::Abandoned,
        _ => Draft::Absent,
    }
}
