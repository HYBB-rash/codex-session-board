use super::*;
use crate::rpc::{Role, Rpc, identity_fixture::Fixture};
use serde_json::json;

fn registry(fixture: &Fixture, duplicate: bool) -> Result<()> {
    let mut hosts =
        vec![json!({"hostId":fixture.host.host_id,"displayName":"first","alias":"example"})];
    if duplicate {
        hosts.push(json!({"hostId":"remote:alias","displayName":"second","alias":"example"}));
    }
    config::write_json(
        &fixture.config.sidebar_state_file,
        &json!({"codex-managed-remote-connections":hosts}),
        fixture.config.sidebar_state_file.exists(),
    )
}

fn setup() -> Result<(Fixture, Binding, Binding)> {
    let mut fixture = Fixture::new()?;
    fixture.config.sidebar_state_file = fixture.config.data_dir.with_file_name("registry.json");
    registry(&fixture, true)?;
    let first = fixture.binding()?;
    let mut host = fixture.host.clone();
    host.host_id = "remote:alias".into();
    let second = Binding::prepare(&host, &fixture.config)?;
    Ok((fixture, first, second))
}

#[test]
fn duplicate_observable_data_sources_keep_reads_but_reject_writes_until_registration_is_unique()
-> Result<()> {
    let (fixture, first, second) = setup()?;
    let mut config = fixture.config.clone();
    config.remote_binding = Some(first.clone());
    let app = crate::app::App::new(config)?;
    let mut writer = app.connection(Duration::from_secs(3), Role::TitleWriter)?;
    second.confirm(&fixture.peer()?, Instant::now() + Duration::from_secs(1))?;
    let before = std::fs::read(&first.path)?;
    assert!(first.require_confirmed().is_err());
    assert!(second.require_confirmed().is_err());
    assert!(writer.call("thread/name/set", json!({})).is_err());
    assert_eq!(writer.call("thread/read", json!({}))?["ok"], true);
    assert!(
        !fixture
            .events()
            .lines()
            .any(|event| event == "thread/name/set")
    );
    assert_eq!(std::fs::read(&first.path)?, before);
    registry(&fixture, false)?;
    first.require_confirmed()?;
    assert_eq!(writer.call("thread/name/set", json!({}))?["ok"], true);
    assert_eq!(
        fixture
            .events()
            .lines()
            .filter(|event| *event == "thread/name/set")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn duplicate_check_uses_physical_data_identity_and_does_not_block_distinct_or_unusable_sources()
-> Result<()> {
    let (fixture, first, second) = setup()?;
    first.confirm(&fixture.peer()?, Instant::now() + Duration::from_secs(1))?;
    first.require_confirmed()?;
    let mut peer = fixture.peer()?;
    peer.codex_home = "/another".into();
    peer.declared_codex_home = "/another".into();
    second.confirm(&peer, Instant::now() + Duration::from_secs(1))?;
    first.require_confirmed()?;
    let mut record = load(&second.path)?;
    peer.codex_home = fixture.peer()?.codex_home;
    peer.declared_codex_home = "/logical-alias".into();
    peer.home = "/another-home".into();
    record.peer = Some(peer);
    publish(&second.path, &record, true)?;
    assert!(first.require_confirmed().is_err());
    std::fs::write(&second.path, "{broken")?;
    first.require_confirmed()?;
    assert!(second.require_confirmed().is_err());
    registry(&fixture, false)?;
    first.require_confirmed()?;
    Ok(())
}
