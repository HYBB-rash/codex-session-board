mod session;

#[cfg(test)]
mod tests {
    use super::session::{events, invariant, step, Column, Event, Phase, Session};

    #[test]
    fn initial_snapshot_does_not_move_an_unread_session() {
        let session = Session::initial();
        invariant(session).unwrap();
        assert_eq!(events().len(), 39);
        assert_eq!(step(session, Event::View), session);
        assert_eq!(step(session, Event::Same), session);
        for column in Column::ALL {
            assert_eq!(Column::parse(column.as_str()).unwrap(), column);
        }
        for phase in Phase::ALL {
            assert!(!phase.label().is_empty());
        }
    }
}
