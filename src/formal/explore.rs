use super::check::transition;
use super::session::{Session, events, invariant, step};
use anyhow::Result;
use std::collections::{HashSet, VecDeque};

pub fn reachable() -> Result<Vec<Session>> {
    let mut seen = HashSet::from([Session::initial()]);
    let mut queue = VecDeque::from([Session::initial()]);
    while let Some(session) = queue.pop_front() {
        invariant(session)?;
        admit(&mut seen, &mut queue, session)?;
    }
    Ok(seen.into_iter().collect())
}

fn admit(
    seen: &mut HashSet<Session>,
    queue: &mut VecDeque<Session>,
    session: Session,
) -> Result<()> {
    for event in events() {
        let next = step(session, event);
        transition(session, event, next)?;
        invariant(next)?;
        if seen.insert(next) {
            queue.push_back(next);
        }
    }
    Ok(())
}
