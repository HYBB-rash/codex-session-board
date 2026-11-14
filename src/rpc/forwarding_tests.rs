use super::*;
use crate::rpc::identity_fixture::Fixture;

fn writer(settings: &Config, project: bool) -> Result<(SidebarConnection, UnixStream)> {
    let mut settings = settings.clone();
    let directory = tempfile::tempdir()?;
    let listener = std::os::unix::net::UnixListener::bind(directory.path().join("local"))?;
    if settings.local_authority.is_none() {
        settings.local_authority = Some(authority::LocalAuthority::owned_listener(&listener)?);
    }
    let (client, server) = UnixStream::pair()?;
    client.set_nonblocking(true)?;
    server.set_nonblocking(true)?;
    Ok((
        SidebarConnection {
            stream: DeadlineStream {
                stream: client.into(),
                deadline: deadline_after(Duration::from_secs(3))?,
            },
            caller: "caller".into(),
            host: "local".into(),
            request_id: 0,
            read_only: false,
            project_only: project,
            origin: Some(Guard::new(&settings)),
        },
        server,
    ))
}

fn registered(fixture: &Fixture) -> Result<Config> {
    let mut settings = fixture.config.clone();
    settings.sidebar_state_file = settings.data_dir.with_file_name("registry.json");
    crate::config::write_json(
        &settings.sidebar_state_file,
        &json!({
            "codex-managed-remote-connections":[{
                "hostId":fixture.host.host_id,"displayName":"test","alias":"example"
            }]
        }),
        false,
    )?;
    Ok(settings)
}

#[test]
fn sidebar_bound_writer_rejects_changed_peer_or_alias_before_sending_any_frame() -> Result<()> {
    for (field, value) in [
        ("hostname", json!("different")),
        (
            "peer",
            json!({"uid":502,"home":"/Users/remote","codexHome":"/Users/remote/.codex","declaredCodexHome":"/Users/remote/.codex"}),
        ),
    ] {
        let fixture = Fixture::new()?;
        let app = fixture.app()?;
        drop(app.connection(Duration::from_secs(3), Role::NativeReader)?);
        let (mut writer, mut server) = writer(&app.config, false)?;
        fixture.update(field, value)?;
        let before = std::fs::read(app.config.data_dir.join("source.json"))?;
        assert!(
            writer
                .call(
                    "move_thread_to_sidebar_section",
                    json!({"hostId":fixture.host.host_id,"threadId":"fake"})
                )
                .is_err()
        );
        assert_eq!(
            server.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            std::fs::read(app.config.data_dir.join("source.json"))?,
            before
        );
    }
    Ok(())
}

#[test]
fn project_forwarder_rechecks_registered_origin_even_after_draft_connection_succeeded() -> Result<()>
{
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    drop(runtime::RuntimeConnection::draft(&app)?);
    let settings = registered(&fixture)?;
    let (mut writer, mut server) = writer(&settings, true)?;
    fixture.update("port", json!(2222))?;
    assert!(
        writer
            .call(
                "move_thread_to_project",
                json!({"hostId":fixture.host.host_id,"threadId":"fake"})
            )
            .is_err()
    );
    assert_eq!(
        server.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(
        fixture
            .events()
            .lines()
            .all(|event| event != "thread/start")
    );
    Ok(())
}

#[test]
fn forwarded_writes_reject_missing_host_registration_or_a_different_bound_target() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    drop(app.connection(Duration::from_secs(3), Role::Reader)?);
    let (mut bound, mut server) = writer(&app.config, false)?;
    assert!(
        bound
            .call(
                "move_thread_to_sidebar_section",
                json!({"hostId":"remote:other"})
            )
            .is_err()
    );
    assert_eq!(
        server.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let settings = registered(&fixture)?;
    crate::config::write_json(
        &settings.sidebar_state_file,
        &json!({"codex-managed-remote-connections":[]}),
        true,
    )?;
    let (mut removed, mut server) = writer(&settings, true)?;
    assert!(
        removed
            .call(
                "move_thread_to_project",
                json!({"hostId":fixture.host.host_id})
            )
            .is_err()
    );
    assert_eq!(
        server.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    Ok(())
}

fn acknowledge(mut server: UnixStream) -> Result<()> {
    server.set_nonblocking(false)?;
    server.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut header = [0; 4];
    server.read_exact(&mut header)?;
    let mut bytes = vec![0; u32::from_le_bytes(header) as usize];
    server.read_exact(&mut bytes)?;
    let request: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(request["params"]["hostId"], "local");
    assert_eq!(request["params"]["arguments"]["hostId"], "remote:test");
    let response = serde_json::to_vec(&json!({"id":request["id"],"result":{
        "success":true,"contentItems":[{"type":"inputText","text":"{\"ok\":true}"}]
    }}))?;
    server.write_all(&u32::try_from(response.len())?.to_le_bytes())?;
    server.write_all(&response)?;
    Ok(())
}

#[test]
fn verified_bound_and_registered_origins_can_forward_on_the_local_desktop_socket() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    drop(app.connection(Duration::from_secs(3), Role::NativeReader)?);
    for project in [false, true] {
        let settings = if project {
            registered(&fixture)?
        } else {
            app.config.clone()
        };
        let (mut writer, server) = writer(&settings, project)?;
        let worker = thread::spawn(move || acknowledge(server));
        let method = if project {
            "move_thread_to_project"
        } else {
            "move_thread_to_sidebar_section"
        };
        assert_eq!(
            writer.call(
                method,
                json!({"hostId":fixture.host.host_id,"threadId":"fake"})
            )?,
            json!({"ok":true})
        );
        worker.join().unwrap()?;
    }
    Ok(())
}

fn reassign(fixture: &Fixture, same_endpoint: bool) -> Result<()> {
    if same_endpoint {
        let mut peer = fixture.peer()?;
        peer.codex_home = "/different".into();
        peer.declared_codex_home = "/different".into();
        fixture.update("peerByTarget", json!({"replacement":peer}))?;
    } else {
        fixture.update(
            "targets",
            json!({"replacement":{"hostname":"different","user":"remote","port":22}}),
        )?;
    }
    crate::config::write_json(
        &fixture.config.sidebar_state_file,
        &json!({
            "codex-managed-remote-connections":[{
                "hostId":fixture.host.host_id,"displayName":"test","alias":"replacement"
            }]
        }),
        true,
    )
}

#[test]
fn cached_proxy_and_sidebar_reselect_desktop_alias_instead_of_trusting_a_live_old_alias()
-> Result<()> {
    for same_endpoint in [false, true] {
        let mut fixture = Fixture::new()?;
        fixture.config = registered(&fixture)?;
        let app = fixture.app()?;
        let mut connection = app.connection(Duration::from_secs(3), Role::TitleWriter)?;
        let (mut writer, mut server) = writer(&app.config, false)?;
        reassign(&fixture, same_endpoint)?;
        assert!(connection.call("thread/name/set", json!({})).is_err());
        assert!(
            writer
                .call(
                    "move_thread_to_sidebar_section",
                    json!({"hostId":fixture.host.host_id})
                )
                .is_err()
        );
        assert_eq!(
            server.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert!(
            !fixture
                .events()
                .lines()
                .any(|event| event == "thread/name/set")
        );
    }
    Ok(())
}
