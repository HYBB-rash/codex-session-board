use super::*;
use crate::rpc::{Role, identity_fixture::Fixture};
use std::time::Duration;

fn managed(fixture: &Fixture) -> Result<(App, Arc<App>)> {
    let mut settings = fixture.config.clone();
    settings.sidebar_state_file = settings.data_dir.with_file_name("sidebar.json");
    crate::config::write_json(
        &settings.sidebar_state_file,
        &json!({
            "codex-managed-remote-connections":[{
                "hostId":fixture.host.host_id,"displayName":"test","alias":"example"
            }]
        }),
        false,
    )?;
    let app = App::new(settings)?;
    let remote = app.build_remote(&fixture.host)?;
    app.remotes
        .lock()
        .unwrap()
        .insert(fixture.host.host_id.clone(), remote.clone());
    Ok((app, remote))
}

#[test]
fn managed_write_gate_requires_initialization_and_preserves_notes_after_home_change() -> Result<()>
{
    let fixture = Fixture::new()?;
    let (root, remote) = managed(&fixture)?;
    let identifier = uuid::Uuid::new_v4().to_string();
    remote
        .store
        .discover(&[json!({"id":identifier,"name":"test"})])?;
    let mut body = json!({
        "id":crate::store::remote::identity(&fixture.host.host_id, &identifier),
        "state":"review","note":"keep","version":0,"inputVersion":0
    });
    assert!(root.route_remote("/api/state", &body).is_err());
    assert_eq!(remote.store.note(&identifier)?, "");
    drop(remote.connection(Duration::from_secs(3), Role::RemoteObserver)?);
    root.route_remote("/api/state", &body)?;
    assert_eq!(remote.store.note(&identifier)?, "keep");
    let source = remote.config.data_dir.join("source.json");
    let before = std::fs::read(&source)?;
    let mut peer = fixture.peer()?;
    peer.codex_home = "/other".into();
    peer.declared_codex_home = "/other".into();
    fixture.update("peer", serde_json::to_value(peer)?)?;
    body["version"] = json!(1);
    body["note"] = json!("do not save");
    assert!(root.route_remote("/api/state", &body).is_err());
    assert_eq!(remote.store.note(&identifier)?, "keep");
    assert_eq!(std::fs::read(source)?, before);
    assert!(
        !fixture
            .events()
            .lines()
            .any(|event| event.starts_with("thread/"))
    );
    Ok(())
}

#[test]
fn remote_auto_archive_requires_its_own_explicit_positive_interval() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.config.auto_archive_seconds = Some(1800);
    let (mut root, remote) = managed(&fixture)?;
    assert_eq!(root.config.auto_archive_seconds, Some(1800));
    assert_eq!(remote.config.auto_archive_seconds, None);
    assert!(!remote.config.native_sections);
    assert_eq!(remote.config.remote_retry, fixture.host.auto_connect);
    root.config.remote_auto_archive_seconds = Some(900);
    let enabled = root.build_remote(&fixture.host)?;
    assert_eq!(enabled.config.auto_archive_seconds, Some(900));
    assert_eq!(root.config.auto_archive_seconds, Some(1800));
    root.config.remote_auto_archive_seconds = Some(0);
    assert!(root.config.validate().is_err());
    root.config.remote_auto_archive_seconds = Some(900);
    root.config.legacy_mode = true;
    root.config.auto_archive_seconds = None;
    assert!(root.config.validate_modes().is_err());
    Ok(())
}
