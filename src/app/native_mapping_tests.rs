use super::*;
use crate::rpc::identity_fixture::Fixture;

struct Sections(Vec<Value>);

impl Rpc for Sections {
    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        ensure!(method == "threadSection/list", "unexpected operation: {method}");
        Ok(json!({"data":self.0,"nextCursor":null}))
    }
}

fn registry(app: &mut App) -> Result<(Value, Sections)> {
    app.config.status_sections = crate::model::board_states().map(|(state, _)|
        (state.into(), uuid::Uuid::new_v4().to_string())).collect();
    app.config.sidebar_state_file = app.config.data_dir.join("sidebar.json");
    let entries: Vec<_> = app.config.section_bindings().enumerate().map(|(index, (_, name, id))|
        json!({"id":id,"name":name,"hostSectionIds":{(&app.config.host_id):format!("backend-{index}")}}))
        .collect();
    let sections = Sections(entries.iter().enumerate().map(|(index, entry)|
        json!({"id":format!("backend-{index}"),"name":entry["name"]})).collect());
    let state = json!({"electron-renderer-content-owner":"owner","electron-owner-atoms:owner":{
        "sidebar-custom-sections-v3":{"account":{"sections":entries}}}});
    std::fs::write(&app.config.sidebar_state_file, state.to_string())?;
    Ok((state, sections))
}

#[test]
fn cached_mapping_accepts_renames_but_rechecks_changed_section_identities() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut app = fixture.app()?;
    let _connection = app.connection(Duration::from_secs(3), Role::RemoteObserver)?;
    let (mut state, mut sections) = registry(&mut app)?;
    let seed = app.mapping_seed()?;
    let record = mapping::Record { scope:seed.scope.clone(), logical:seed.logical, probe:None };
    app.mapping_store.save_native_mapping(&app.config.host_id, &record)?;
    *app.native_mapping_scope.lock().unwrap() = Some(seed.scope.clone());
    app.native_catalog(&mut sections)?;
    for section in &mut sections.0 {
        section["name"] = json!("renamed");
    }
    app.native_catalog(&mut sections)?;
    let entries = state["electron-owner-atoms:owner"]["sidebar-custom-sections-v3"]
        ["account"]["sections"].as_array_mut().unwrap();
    entries[0]["hostSectionIds"][&app.config.host_id] = json!("backend-1");
    entries[1]["hostSectionIds"][&app.config.host_id] = json!("backend-0");
    std::fs::write(&app.config.sidebar_state_file, state.to_string())?;
    assert_eq!(app.mapping_seed()?.scope, seed.scope);
    assert!(app.native_catalog(&mut sections).is_err());
    assert!(!fixture.events().contains("thread/start"));
    assert!(!fixture.events().contains("move_thread_to_sidebar_section"));
    Ok(())
}
