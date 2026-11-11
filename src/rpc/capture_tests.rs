use super::*;
use crate::rpc::identity_fixture::Fixture;

#[test]
fn ssh_g_capture_bounds_stdout_stderr_and_never_reports_raw_diagnostics() -> Result<()> {
    for (field, value) in [
        ("gOutput", json!("x".repeat(CAPTURE_LIMIT + 1))),
        ("gStderr", json!("PRIVATE-DIAGNOSTIC".repeat(CAPTURE_LIMIT))),
        ("gStatus", json!(255)),
    ] {
        let fixture = Fixture::new()?;
        fixture.update("gStderr", json!("PRIVATE-DIAGNOSTIC"))?;
        fixture.update(field, value)?;
        let error = fixture.binding().unwrap_err();
        assert!(!format!("{error:#}").contains("PRIVATE-DIAGNOSTIC"));
        assert!(
            !fixture
                .host
                .directory(&fixture.config)
                .join("source.json")
                .exists()
        );
    }
    Ok(())
}

#[test]
fn ssh_g_timeout_reaps_only_its_owned_process_group() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("pid");
    let command = vec![
        "python3".into(),
        "-B".into(),
        "-c".into(),
        "import os,sys,time; open(sys.argv[1],'w').write(str(os.getpid())); time.sleep(20)".into(),
        path.to_string_lossy().into(),
    ];
    let mut unrelated = ManagedChild(Command::new("sleep").arg("10").spawn()?);
    let start = Instant::now();
    assert!(output(&command, start + Duration::from_millis(100)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let pid = std::fs::read_to_string(path)?;
    let pid = rustix::process::Pid::from_raw(pid.parse()?);
    assert!(matches!(
        rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
        Err(Errno::CHILD)
    ));
    assert!(unrelated.0.try_wait()?.is_none());
    Ok(())
}
