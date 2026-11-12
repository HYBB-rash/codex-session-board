use anyhow::Result;
use codex_session_board::{
    app,
    config::{self, Config},
};
use std::fs;

#[test]
fn singleton_protection_defaults_off() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.json");
    fs::write(&path, "{}")?;
    assert!(Config::default().client_singleton_profile.is_none());
    assert!(config::load(Some(&path))?.client_singleton_profile.is_none());
    Ok(())
}

#[test]
fn disabled_protection_preserves_navigation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let captured = directory.path().join("url");
    let settings = Config {
        data_dir: directory.path().to_path_buf(),
        open_command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf '%s' \"$1\" > \"$0\"".into(),
            captured.to_string_lossy().into(),
        ],
        ..Config::default()
    };
    settings.validate()?;
    let identifier = "11111111-1111-4111-8111-111111111111";
    let url = app::open_conversation(identifier, &settings)?;
    assert_eq!(url, app::conversation_url(identifier, &settings.host_id)?);
    assert_eq!(fs::read_to_string(captured)?, url);
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn configured_linux_protection_is_rejected() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profile = directory.path().join("profile");
    fs::create_dir(&profile)?;
    let path = directory.path().join("config.json");
    fs::write(
        &path,
        serde_json::json!({"client_singleton_profile": profile}).to_string(),
    )?;
    let error = config::load(Some(&path))
        .err()
        .ok_or_else(|| anyhow::anyhow!("macOS accepted Linux singleton protection"))?;
    assert!(
        error
            .to_string()
            .contains("client_singleton_profile 仅支持 Linux 非空路径")
    );
    assert_eq!(fs::read_dir(profile)?.count(), 0);
    Ok(())
}
