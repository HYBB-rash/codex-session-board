mod check;
mod explore;
mod fixture;
mod session;

#[cfg(test)]
mod tests {
    use super::{explore, session};

    #[test]
    fn native_session_control_reaches_5922_states() {
        let states = explore::reachable().unwrap();
        assert_eq!(states.len(), 5922);
        assert_eq!(session::events().len(), 39);
    }
}
