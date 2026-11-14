use super::*;

#[test]
fn remote_summary_projection_and_wire_performance_gate() -> Result<()> {
    let result = Command::new("python3").arg("-B")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test_remote_turn_proxy.py"))
        .output()?;
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    Ok(())
}

#[test]
fn observer_uses_remote_summary_and_archive_writer_keeps_full_input_verification() -> Result<()> {
    for (role, compact) in [(Role::RemoteObserver, true), (Role::ArchiveWriter, false)] {
        let fixture = identity_fixture::Fixture::new()?;
        let app = fixture.app()?;
        let _connection = app.connection(Duration::from_secs(3), role)?;
        assert_eq!(fixture.events().lines().any(|event| event == "compact"), compact);
    }
    Ok(())
}
