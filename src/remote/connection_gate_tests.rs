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
