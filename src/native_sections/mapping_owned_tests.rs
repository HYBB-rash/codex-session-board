use super::*;

pub(super) fn client_atoms(state: &Value) -> Result<serde_json::Map<String, Value>> {
    Ok(super::client_atoms(state)?.to_owned())
}

#[test]
fn seed_keeps_owned_atoms_alive_while_reading_account_sections() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let identifier = uuid::Uuid::new_v4().to_string();
    let config = Config {
        host_id: "remote:a".into(),
        sidebar_state_file: directory.path().join("state.json"),
        status_sections: [("review".into(), identifier.clone())].into(),
        ..Config::default()
    };
    let state = json!({
        "electron-renderer-content-owner": "owner",
        "electron-owner-atoms:owner": {
            "sidebar-custom-sections-v3": {
                "account": {"sections": [{
                    "id": identifier, "name": "待审阅",
                    "hostSectionIds": {"remote:a": "backend-review"}
                }]}
            }
        }
    });
    std::fs::write(&config.sidebar_state_file, state.to_string())?;
    let seed = mapping::seed(&config)?;
    assert_eq!(seed.logical, [("backend-review".into(), identifier)].into());
    assert_eq!(seed.scope, json!({
        "owner": "owner", "account": "account",
        "host": "remote:a", "bindings": config.status_sections
    }));
    Ok(())
}
