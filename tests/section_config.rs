use anyhow::Result;
use codex_session_board::{config::{self, Config}, model};
use serde_json::{Value, json};
use std::path::Path;

fn source(directory: &Path) -> Result<std::path::PathBuf> {
    let config = Config {
        data_dir: "data".into(),
        native_sections: true,
        sidebar_state_file: directory.join("state.json"),
        sidebar_socket: directory.join("sidebar.sock"),
        sidebar_caller: uuid::Uuid::new_v4().to_string(),
        status_sections: model::STATES.into_iter()
            .map(|(state, _)| (state.into(), uuid::Uuid::new_v4().to_string())).collect(),
        auto_archive_seconds: Some(1800),
        ..Config::default()
    };
    let mut value = serde_json::to_value(config)?;
    value["deploymentMetadata"] = json!({"preserve":true});
    let path = directory.join("old.json");
    std::fs::write(&path, value.to_string())?;
    Ok(path)
}

#[test]
fn section_migration_preserves_source_data_and_unrelated_configuration() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = source(directory.path())?;
    let bytes = std::fs::read(&source)?;
    assert!(config::load(Some(&source)).is_err());
    let target = directory.path().join("another/new.json");
    let receipt = config::migrate_sections(&source, &target)?;
    let loaded = config::load(Some(&target))?;
    assert_eq!(loaded.status_sections.len(), 5);
    assert_eq!(loaded.data_dir, directory.path().join("data"));
    assert_eq!(loaded.auto_archive_seconds, Some(1800));
    assert_eq!(receipt["configId"], config::config_id(&loaded)?);
    let published: Value = serde_json::from_slice(&std::fs::read(&target)?)?;
    assert_eq!(published["deploymentMetadata"], json!({"preserve":true}));
    assert_eq!(std::fs::read(&source)?, bytes);
    assert!(!directory.path().join("data").exists());
    assert!(config::migrate_sections(&source, &target).is_err());
    assert_eq!(config::load(Some(&target))?.status_sections, loaded.status_sections);
    assert!(config::migrate_sections(&source, &source).is_err());
    Ok(())
}

#[test]
fn section_migration_refuses_missing_active_bindings_and_legacy_mode() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = source(directory.path())?;
    let original: Value = serde_json::from_slice(&std::fs::read(&source)?)?;
    for mode in ["missing", "legacy", "duplicate"] {
        let mut value = original.clone();
        match mode {
            "missing" => { value["status_sections"].as_object_mut().unwrap().remove("review"); }
            "legacy" => { value["legacy_mode"] = json!(true); }
            _ => { value["status_sections"]["review"] = value["status_sections"]["blocked"].clone(); }
        }
        std::fs::write(&source, value.to_string())?;
        let target = directory.path().join(format!("{mode}.json"));
        assert!(config::migrate_sections(&source, &target).is_err());
        assert!(!target.exists());
    }
    Ok(())
}
