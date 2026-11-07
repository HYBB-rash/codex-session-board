use super::*;
use crate::rpc::identity_fixture::Fixture;

#[test]
fn marker_handles_every_banner_split_and_preserves_following_binary_bytes() -> Result<()> {
    let marker = "abcdef0123456789abcdef0123456789";
    let peer = Fixture::new()?.peer()?;
    let bytes = [0, 255, 128, 13, 10];
    let mut packet = format!(
        "banner without newline{marker}\n{}\n",
        serde_json::to_string(&peer)?
    )
    .into_bytes();
    packet.extend(bytes);
    for split in 0..=packet.len() {
        let mut stream =
            std::io::Cursor::new(&packet[..split]).chain(std::io::Cursor::new(&packet[split..]));
        assert_eq!(header(&mut stream, marker)?, peer);
        let mut actual = Vec::new();
        stream.read_to_end(&mut actual)?;
        assert_eq!(actual, bytes);
    }
    Ok(())
}

#[test]
fn framing_rejects_eof_wrong_nonce_oversize_banner_header_and_invalid_peer() -> Result<()> {
    let marker = "a".repeat(32);
    let peer = Fixture::new()?.peer()?;
    for mut packet in [
        Vec::new(),
        format!("{}\n{}\n", "b".repeat(32), serde_json::to_string(&peer)?).into_bytes(),
        vec![b'x'; BANNER_LIMIT + marker.len() + 1],
        format!("{marker}\n{}\n", "x".repeat(PEER_LIMIT)).into_bytes(),
        format!("{marker}\n{{\"private-field\":\"DO-NOT-PRINT\"}}\n").into_bytes(),
        format!(
            "{marker}\n{}\n",
            serde_json::to_string(&Peer {
                home: "relative".into(),
                ..peer.clone()
            })?
        )
        .into_bytes(),
    ] {
        let error = header(&mut packet.as_slice(), &marker).unwrap_err();
        assert!(!format!("{error:#}").contains("DO-NOT-PRINT"));
        packet.fill(0);
    }
    Ok(())
}

#[test]
fn remote_startup_stall_times_out_and_never_initializes() -> Result<()> {
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    fixture.update("delay", json!(5))?;
    let start = Instant::now();
    assert!(DesktopConnection::remote(&binding, Duration::from_millis(100), Role::Reader).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!fixture.events().contains("initialize"));
    Ok(())
}

#[test]
fn initialize_codex_home_is_independent_and_required_before_business_methods() -> Result<()> {
    for home in [json!("/different-daemon"), Value::Null] {
        let fixture = Fixture::new()?;
        let binding = fixture.binding()?;
        fixture.update("initializeHome", home)?;
        let before = std::fs::read(&binding.path)?;
        assert!(
            DesktopConnection::remote(&binding, Duration::from_secs(2), Role::TitleWriter).is_err()
        );
        assert_eq!(std::fs::read(&binding.path)?, before);
        let events = fixture.events();
        assert!(events.lines().any(|event| event == "initialize"));
        assert!(
            !events
                .lines()
                .any(|event| event == "initialized" || event == "thread/name/set")
        );
    }
    Ok(())
}

#[test]
fn verified_proxy_strips_banner_initializes_and_binds_peer_before_writing() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    let mut connection = app.connection(Duration::from_secs(3), Role::TitleWriter)?;
    assert_eq!(
        connection.call("thread/name/set", json!({}))?,
        json!({"ok":true})
    );
    let binding = app.config.remote_binding.as_ref().unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&binding.path)?)?;
    assert_eq!(saved["peer"], serde_json::to_value(fixture.peer()?)?);
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
fn cached_writer_rechecks_alias_and_peer_before_any_mutating_rpc() -> Result<()> {
    for (key, value) in [
        ("hostname", json!("other")),
        ("user", json!("other")),
        ("port", json!(2222)),
        (
            "peer",
            json!({"uid":502,"home":"/Users/remote","codexHome":"/Users/remote/.codex","declaredCodexHome":"/Users/remote/.codex"}),
        ),
        (
            "peer",
            json!({"uid":501,"home":"/Users/remote","codexHome":"/changed","declaredCodexHome":"/changed"}),
        ),
    ] {
        let fixture = Fixture::new()?;
        let binding = fixture.binding()?;
        let mut connection =
            DesktopConnection::remote(&binding, Duration::from_secs(3), Role::DraftWriter)?;
        let before = std::fs::read(&binding.path)?;
        fixture.update(key, value)?;
        assert!(connection.call("thread/start", json!({})).is_err());
        assert_eq!(std::fs::read(&binding.path)?, before);
        assert!(
            !fixture
                .events()
                .lines()
                .any(|event| event == "thread/start")
        );
    }
    Ok(())
}

#[test]
fn clock_and_proxy_share_peer_checks_and_reject_different_clock_authority() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    let mut connection = runtime::RuntimeConnection::connect(&app)?;
    assert_eq!(connection.call("thread/list", json!({}))?["ok"], true);
    fixture.update("clockPeer", json!({"uid":502,"home":"/Users/remote","codexHome":"/Users/remote/.codex","declaredCodexHome":"/Users/remote/.codex"}))?;
    assert!(runtime::RuntimeConnection::connect(&app).is_err());
    Ok(())
}

#[test]
fn nonblocking_fixture_accept_waits_for_the_connection_and_has_a_deadline() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("accept.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    let deadline = deadline_after(Duration::from_millis(20))?;
    assert_eq!(
        deadline_io(deadline, || listener.accept())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        UnixStream::connect(path)
    });
    let deadline = deadline_after(Duration::from_secs(1))?;
    let (_accepted, _) = deadline_io(deadline, || listener.accept())?;
    let _client = worker.join().unwrap()?;
    Ok(())
}

#[test]
fn initialize_cannot_substitute_a_logical_alias_for_the_resolved_peer_home() -> Result<()> {
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    let mut peer = fixture.peer()?;
    peer.codex_home = "/resolved/codex".into();
    peer.declared_codex_home = "/alias/codex".into();
    fixture.update("peer", serde_json::to_value(&peer)?)?;
    assert!(DesktopConnection::remote(&binding, Duration::from_secs(2), Role::Reader).is_err());
    assert!(serde_json::from_slice::<Value>(&std::fs::read(&binding.path)?)?["peer"].is_null());
    fixture.update("initializeHome", json!(peer.codex_home))?;
    drop(DesktopConnection::remote(
        &binding,
        Duration::from_secs(2),
        Role::Reader,
    )?);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&binding.path)?)?["peer"],
        serde_json::to_value(peer)?
    );
    Ok(())
}

#[test]
fn draft_factory_keeps_clock_validation_for_thread_start_and_thread_read_metadata() -> Result<()> {
    for method in ["thread/start", "thread/read"] {
        let fixture = Fixture::new()?;
        fixture.update(
            "result",
            json!({"thread":{"createdAt":"invalid","updatedAt":1}}),
        )?;
        let app = fixture.app()?;
        let mut connection = runtime::RuntimeConnection::draft(&app)?;
        let error = connection.call(method, json!({})).unwrap_err();
        assert!(error.to_string().contains("时间格式"));
        assert!(fixture.events().lines().any(|event| event == method));
    }
    Ok(())
}
