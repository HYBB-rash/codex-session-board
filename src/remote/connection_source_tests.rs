use super::*;
use crate::rpc::identity_fixture::Fixture;
use serde_json::json;

#[test]
fn ssh_identity_keeps_only_effective_hostname_user_port_and_rejects_ambiguity() -> Result<()> {
    let parsed = Endpoint::parse(
        b"hostname mac\nuser remote\nport 2222\nidentityfile SENSITIVE\nproxycommand SECRET\n",
    )?;
    assert_eq!(
        serde_json::to_value(parsed)?,
        json!({"hostname":"mac","user":"remote","port":2222})
    );
    for bytes in [
        b"hostname mac\nhostname other\nuser r\nport 22\n".as_slice(),
        b"hostname mac\nuser r\n",
        b"hostname mac\nuser r\nport 0\n",
        b"hostname mac\nuser r\nport 99999\n",
        b"hostname mac\nuser r\0\nport 22\n",
        b"hostname mac\nuser invalid user\nport 22\n",
    ] {
        assert!(Endpoint::parse(bytes).is_err());
    }
    Ok(())
}

#[test]
fn source_identity_survives_timeout_keepalive_wrapper_and_nonce_changes() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    binding.confirm(&fixture.peer()?, Instant::now() + Duration::from_secs(1))?;
    let before = std::fs::read(&binding.path)?;
    fixture.config.rpc_timeout_seconds = 7;
    fixture
        .config
        .ssh_command
        .extend(["-o".into(), "ServerAliveInterval=2".into()]);
    let next = fixture.binding()?;
    assert_eq!(next.endpoint, binding.endpoint);
    assert_ne!(next.ssh, binding.ssh);
    assert_ne!(
        next.launch(":", &"a".repeat(32))?,
        next.launch(":", &"b".repeat(32))?
    );
    fixture.host.verify_source(&fixture.config)?;
    assert_eq!(std::fs::read(&binding.path)?, before);
    let serialized = String::from_utf8(before)?;
    for excluded in [
        "ConnectTimeout",
        "ServerAlive",
        "command",
        "NOT-PERSISTED",
        "identityfile",
    ] {
        assert!(!serialized.contains(excluded));
    }
    Ok(())
}

#[test]
fn alias_reassignment_of_hostname_user_or_port_preserves_source_and_database() -> Result<()> {
    for (field, value) in [
        ("hostname", json!("other")),
        ("user", json!("another")),
        ("port", json!(2222)),
    ] {
        let fixture = Fixture::new()?;
        let binding = fixture.binding()?;
        std::fs::write(binding.path.with_file_name("status.sqlite3"), b"preserved")?;
        let before = std::fs::read(&binding.path)?;
        fixture.update(field, value)?;
        assert!(
            binding
                .verify(Instant::now() + Duration::from_secs(1))
                .is_err()
        );
        assert!(fixture.host.verify_source(&fixture.config).is_err());
        assert!(fixture.binding().is_err());
        assert_eq!(std::fs::read(&binding.path)?, before);
        assert_eq!(
            std::fs::read(binding.path.with_file_name("status.sqlite3"))?,
            b"preserved"
        );
    }
    Ok(())
}

#[test]
fn legacy_corrupt_and_missing_source_records_are_never_automatically_rebound() -> Result<()> {
    for record in [
        b"{\"hostId\":\"remote:test\",\"command\":[\"ssh\"]}".as_slice(),
        b"{broken",
    ] {
        let fixture = Fixture::new()?;
        let path = fixture.host.directory(&fixture.config).join("source.json");
        config::create_private_dir(path.parent().unwrap())?;
        std::fs::write(&path, record)?;
        assert!(fixture.binding().is_err());
        assert_eq!(std::fs::read(path)?, record);
        assert!(fixture.events().is_empty());
    }
    let fixture = Fixture::new()?;
    let directory = fixture.host.directory(&fixture.config);
    config::create_private_dir(&directory)?;
    std::fs::write(directory.join("status.sqlite3"), "existing")?;
    assert!(fixture.binding().is_err());
    assert!(!directory.join("source.json").exists());
    Ok(())
}

#[test]
fn peer_identity_rejects_changed_uid_home_and_codex_home() -> Result<()> {
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    let peer = fixture.peer()?;
    binding.confirm(&peer, Instant::now() + Duration::from_secs(1))?;
    let before = std::fs::read(&binding.path)?;
    for field in ["uid", "home", "codexHome", "declaredCodexHome"] {
        let mut changed = serde_json::to_value(&peer)?;
        changed[field] = if field == "uid" {
            json!(502)
        } else {
            json!("/different")
        };
        assert!(
            binding
                .check_peer(&serde_json::from_value(changed)?)
                .is_err()
        );
    }
    assert_eq!(std::fs::read(&binding.path)?, before);
    Ok(())
}

#[test]
fn source_confirmation_lock_is_bounded_without_overwriting_the_record() -> Result<()> {
    use fs2::FileExt;
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(binding.path.with_extension("lock"))?;
    file.lock_exclusive()?;
    let before = std::fs::read(&binding.path)?;
    let start = Instant::now();
    assert!(
        binding
            .confirm(&fixture.peer()?, start + Duration::from_millis(25))
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(std::fs::read(&binding.path)?, before);
    Ok(())
}

#[test]
fn empty_alias_uses_hostname_without_interpreting_shell_characters() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.host.alias = Some(String::new());
    fixture.host.hostname = Some("remote@literal;host".into());
    let command = fixture
        .host
        .command(&fixture.config, "printf '%s' '$literal'")?;
    assert_eq!(command[command.len() - 2], "remote@literal;host");
    assert!(!command.last().unwrap().contains("app-server --listen"));
    Ok(())
}

#[test]
fn concurrent_first_confirmation_selects_one_peer_without_rebinding() -> Result<()> {
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    let first = fixture.peer()?;
    let second = Peer {
        uid: 502,
        ..first.clone()
    };
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = [first, second]
        .into_iter()
        .map(|peer| {
            let binding = binding.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let result = binding.confirm(&peer, Instant::now() + Duration::from_secs(2));
                (peer, result)
            })
        })
        .collect();
    let mut winner = None;
    let mut failed = 0;
    for worker in workers {
        let (peer, result) = worker.join().unwrap();
        if result.is_ok() {
            assert!(winner.replace(peer).is_none());
        } else {
            failed += 1;
        }
    }
    assert_eq!(failed, 1);
    assert_eq!(load(&binding.path)?.peer, winner);
    Ok(())
}

#[test]
fn oversized_identity_is_rejected_before_publishing_an_unreadable_source_record() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.update("hostname", json!("h".repeat(17000)))?;
    assert!(fixture.binding().is_err());
    assert!(
        !fixture
            .host
            .directory(&fixture.config)
            .join("source.json")
            .exists()
    );
    Ok(())
}
