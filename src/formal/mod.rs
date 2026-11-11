mod check;
mod draft;
mod explore;
mod fixture;
mod paths;
mod replay;
mod session;
mod section_model;
mod section_fixture;
mod sections;

#[cfg(test)]
mod tests {
    use super::{draft, explore, replay, session, sections};

    #[test]
    fn section_configuration_and_native_resolution_conform_to_the_independent_model() {
        assert_eq!(sections::configurations().unwrap(), 1152);
        assert_eq!(sections::accounts().unwrap(), 4096);
        assert_eq!(sections::permutations().unwrap(), 600);
        assert_eq!(sections::faults().unwrap(), 30);
        sections::surfaces().unwrap();
    }

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
