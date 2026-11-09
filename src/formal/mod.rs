mod check;
mod explore;
mod session;

#[cfg(test)]
mod tests {
    use super::{explore, session};

    #[test]
    fn native_session_control_reaches_5922_states() {
        let states = explore::reachable().unwrap();
        assert_eq!(states.len(), 5922);
        assert_eq!(session::events().len(), 39);
        for column in session::Column::ALL {
            assert_eq!(session::Column::parse(column.as_str()).unwrap(), column);
        }
        for phase in session::Phase::ALL {
            assert!(!phase.label().is_empty());
        }
    }
}
