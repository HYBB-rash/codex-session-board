mod check;
mod draft;
mod explore;
mod fixture;
#[path = "../../tests/support/model_database.rs"]
mod model_database;
mod paths;
mod persistence;
mod replay;
mod records;
mod session;
mod time;
mod versions;

#[cfg(test)]
mod tests {
    use super::{draft, explore, replay, session};

    #[test]
    fn native_session_control_matches_the_store_on_every_reachable_edge() {
        let states = explore::reachable().unwrap();
        let edges = replay::conform(&states).unwrap();
        replay::store_refuses_a_column_before_a_section_is_cached().unwrap();
        assert_eq!(session::events().len(), 56);
        assert_eq!(states.len(), 2465);
        assert_eq!(edges, 138_040);
    }

    #[test]
    fn remote_draft_stages_match_the_store_on_every_edge() {
        let edges = draft::conform_drafts().unwrap();
        assert_eq!(edges, 30);
    }
}
mod archive;
