use crate::store::Store;
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

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
    identity_guards(&store, expected)
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
    let before = record(store)?;
    let result = match event {
        DraftEvent::Begin => begin(store, draft),
        DraftEvent::Mark => store.draft_update(REQUEST, None, "created"),
        DraftEvent::Attach => store.draft_update(REQUEST, Some(THREAD), "created"),
        DraftEvent::Complete => complete(store, draft),
        DraftEvent::Abandon => store.draft_update(REQUEST, None, "abandoned"),
    };
    let accepts = matches!(event, DraftEvent::Begin)
        || matches!(draft, Draft::Starting | Draft::Created | Draft::Threaded);
    ensure!(result.is_ok() == accepts, "{draft:?} {event:?}: {result:?}");
    if let Ok(returned) = result {
        let mut expected = expected_record(draft_step(draft, event));
        if matches!(event, DraftEvent::Begin | DraftEvent::Abandon)
            && let Some(before) = &before
        {
            expected["threadId"] = before["threadId"].clone();
        }
        ensure!(
            returned == expected,
            "新建返回记录不符合阶段或身份: {returned}"
        );
        ensure!(record(store)? == Some(returned), "新建返回值与持久记录不同");
    } else {
        ensure!(record(store)? == before, "拒绝新建操作后记录被修改");
    }
    validate_pending(store, draft_step(draft, event))
}

fn begin(store: &Store, draft: Draft) -> Result<Value> {
    let (record, fresh) = store.draft_begin(PROJECT, REQUEST)?;
    ensure!(fresh == (draft == Draft::Absent), "首次/重复新建标志错误");
    Ok(record)
}

fn complete(store: &Store, draft: Draft) -> Result<Value> {
    let thread = match draft {
        Draft::Threaded => None,
        Draft::Starting | Draft::Created => Some(THREAD),
        Draft::Absent | Draft::Complete | Draft::Abandoned => None,
    };
    store.draft_update(REQUEST, thread, "complete")
}

fn record(store: &Store) -> Result<Option<Value>> {
    let value: Option<String> = store
        .connection()?
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [format!("draft_request:{REQUEST}")],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| serde_json::from_str(&value).map_err(Into::into))
        .transpose()
}

fn project(store: &Store) -> Result<Draft> {
    Ok(record(store)?.as_ref().map(decode).unwrap_or(Draft::Absent))
}

fn expected_record(draft: Draft) -> Value {
    let (stage, thread) = match draft {
        Draft::Absent => return Value::Null,
        Draft::Starting => ("starting", None),
        Draft::Created => ("created", None),
        Draft::Threaded => ("created", Some(THREAD)),
        Draft::Complete => ("complete", Some(THREAD)),
        Draft::Abandoned => ("abandoned", None),
    };
    json!({"project":PROJECT,"requestId":REQUEST,"stage":stage,"threadId":thread})
}

fn validate_pending(store: &Store, draft: Draft) -> Result<()> {
    let records = store.pending_drafts()?;
    let pending = matches!(draft, Draft::Starting | Draft::Created | Draft::Threaded);
    ensure!(
        records.len() == usize::from(pending),
        "待核对索引与阶段不一致"
    );
    if pending {
        ensure!(records[0] == expected_record(draft), "待核对请求身份不一致");
    }
    Ok(())
}

fn identity_guards(store: &Store, draft: Draft) -> Result<()> {
    if draft == Draft::Absent {
        return Ok(());
    }
    let before = record(store)?;
    let pending = store.pending_drafts()?;
    ensure!(
        store.draft_begin("another-project", REQUEST).is_err(),
        "同一请求被另一个项目复用"
    );
    if before
        .as_ref()
        .is_some_and(|record| record["threadId"].is_string())
    {
        ensure!(
            store
                .draft_update(REQUEST, Some(REQUEST), "created")
                .is_err(),
            "已确认的会话身份被替换"
        );
    }
    if !pending.is_empty() {
        ensure!(
            store.draft_begin(PROJECT, THREAD).is_err(),
            "待核对期间允许了第二个创建请求"
        );
    }
    ensure!(record(store)? == before, "拒绝跨身份操作后记录改变");
    ensure!(
        store.pending_drafts()? == pending,
        "拒绝跨身份操作后待核对索引改变"
    );
    Ok(())
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
