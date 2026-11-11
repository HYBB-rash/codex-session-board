use super::fixture::{ID, project};
use super::session::{Column, Event, Phase, Session, When, invariant, step};
use crate::store::Store;
use anyhow::Result;
use serde_json::json;

fn start() -> Result<(Store, Session)> {
    let mut store = Store::open_memory(true)?;
    store.native_sections = true;
    store.discover(&[json!({"id":ID,"name":"连续路径","updatedAt":1})])?;
    assert_eq!(project(&store)?, Session::initial());
    Ok((store, Session::initial()))
}

fn follow(store: &Store, expected: &mut Session, events: &[Event]) -> Result<()> {
    for event in events {
        let next = step(*expected, *event);
        super::check::transition(*expected, *event, next)?;
        invariant(next)?;
        super::replay::apply(store, *event)?;
        assert_eq!(project(store)?, next, "{expected:?} -> {event:?}");
        *expected = next;
    }
    Ok(())
}

#[test]
fn input_history_and_reconfirmation_follow_real_continuous_paths() -> Result<()> {
    let (store, mut expected) = start()?;
    follow(&store, &mut expected, &[
        Event::Read(Column::Review), Event::Input,
        Event::Read(Column::ToArchive), Event::Archive,
    ])?;
    assert!(expected.has_input && expected.review && !expected.archived);
    follow(&store, &mut expected, &[
        Event::Manual(Column::ToArchive), Event::View,
    ])?;
    assert!(expected.has_input && !expected.review);
    follow(&store, &mut expected, &[Event::Input, Event::Archive])?;
    assert!(expected.review && !expected.archived);
    follow(&store, &mut expected, &[
        Event::Manual(Column::ToArchive), Event::View, Event::Archive,
    ])?;
    assert!(expected.archived && expected.has_input && !expected.review);
    Ok(())
}

#[test]
fn pending_identity_and_fresh_turns_follow_real_continuous_paths() -> Result<()> {
    for target in Phase::ALL {
        let (store, mut expected) = start()?;
        follow(&store, &mut expected, &[
            Event::Read(Column::Review), Event::Lost(Phase::Running), Event::Retry(target),
        ])?;
        assert_eq!(expected.phase, Some(target));
        assert_eq!(expected.column, target.column());
        assert!(expected.pending.is_none());
        follow(&store, &mut expected, &[
            Event::Manual(Column::ToArchive), Event::Same,
        ])?;
        assert_eq!(expected.column, Column::ToArchive);
        follow(&store, &mut expected, &[Event::Runtime(target, When::Fresh)])?;
        assert_eq!(expected.column, target.column());
        for pending in Phase::ALL {
            if pending != target {
                same_turn_pending(target, pending)?;
            }
        }
    }
    Ok(())
}

fn same_turn_pending(held: Phase, pending: Phase) -> Result<()> {
    let (store, mut expected) = start()?;
    follow(&store, &mut expected, &[
        Event::Read(Column::Review), Event::Runtime(held, When::Fresh),
        Event::LostSameTurn(pending), Event::Retry(held),
    ])?;
    assert_eq!(expected.phase, Some(held));
    assert_eq!(expected.pending, Some(pending));
    assert!(expected.pending_same_turn);
    follow(&store, &mut expected, &[Event::Retry(pending)])?;
    assert_eq!(expected.phase, Some(pending));
    assert!(expected.pending.is_none() && !expected.pending_same_turn);
    follow(&store, &mut expected, &[
        Event::Lost(held), Event::Read(Column::ToArchive), Event::Same,
    ])?;
    assert_eq!(expected.column, Column::ToArchive);
    assert!(expected.pending.is_none());
    Ok(())
}
