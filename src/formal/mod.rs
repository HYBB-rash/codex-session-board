mod check;
mod draft;
mod explore;
mod fixture;
mod replay;
mod session;

#[cfg(test)]
mod tests {
    use super::{draft, explore, replay, session};

    #[test]
    fn native_session_control_matches_the_store_on_every_reachable_edge() {
        let states = explore::reachable().unwrap();
        let edges = replay::conform(&states).unwrap();
        replay::store_refuses_a_column_before_a_section_is_cached().unwrap();
        assert_eq!(session::events().len(), 39);
        assert_eq!(states.len(), 5242);
        assert_eq!(edges, 204_438);
    }

    #[test]
    fn remote_draft_stages_match_the_store_on_every_edge() {
        let edges = draft::conform_drafts().unwrap();
        assert_eq!(edges, 30);
    }
}
