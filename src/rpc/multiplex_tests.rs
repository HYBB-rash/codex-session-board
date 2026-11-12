use super::*;
use crate::rpc::identity_fixture::Fixture;

#[test]
fn shared_connection_restarts_dead_master_and_reaps_owned_process_on_drop() -> Result<()> {
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    let command = binding.launch(":", &"a".repeat(32))?;
    let multiplex = Multiplex::default();
    let first = multiplex.command(&command, deadline_after(Duration::from_secs(3))?)?;
    let (identifier, directory) = {
        let state = multiplex.0.lock().unwrap();
        let master = state.master.as_ref().unwrap();
        (master.process.0.id(), master.directory.path().to_path_buf())
    };
    assert!(first.iter().any(|value| value.starts_with("ControlPath=")));
    assert_eq!(std::fs::metadata(&directory)?.mode() & 0o777, 0o700);
    multiplex
        .0
        .lock()
        .unwrap()
        .master
        .as_mut()
        .unwrap()
        .process
        .0
        .kill()?;
    multiplex.0.lock().unwrap().master.as_mut().unwrap().process.0.wait()?;
    let next = multiplex.command(&command, deadline_after(Duration::from_secs(3))?)?;
    assert_ne!(first, next);
    assert!(!directory.exists());
    drop(multiplex);
    let pid = rustix::process::Pid::from_raw(identifier.try_into()?);
    assert!(matches!(
        rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
        Err(Errno::CHILD)
    ));
    assert_eq!(
        fixture
            .events()
            .lines()
            .filter(|line| *line == "master")
            .count(),
        2
    );
    Ok(())
}

#[test]
fn shared_connection_failure_and_lock_wait_obey_request_deadline() -> Result<()> {
    let fixture = Fixture::new()?;
    let command = fixture.binding()?.launch(":", &"a".repeat(32))?;
    fixture.update("masterFailure", json!(true))?;
    let multiplex = Multiplex::default();
    assert!(
        multiplex
            .command(&command, deadline_after(Duration::from_secs(2))?)
            .is_err()
    );
    assert!(multiplex.0.lock().unwrap().master.is_none());
    let _lock = multiplex.0.lock().unwrap();
    let started = Instant::now();
    assert!(
        multiplex
            .command(&command, deadline_after(Duration::from_millis(30))?)
            .is_err()
    );
    assert!(started.elapsed() < Duration::from_millis(250));
    Ok(())
}

#[test]
fn warm_guard_performance_budget_keeps_one_authentication_and_all_peer_checks() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.update("connectDelay", json!(0.5))?;
    let binding = fixture.binding()?;
    let mut elapsed = Vec::new();
    for _ in 0..8 {
        let started = Instant::now();
        framing::verify_current(&binding, deadline_after(Duration::from_secs(3))?)?;
        elapsed.push(started.elapsed().as_secs_f64());
    }
    let cold = elapsed.remove(0);
    elapsed.sort_by(f64::total_cmp);
    let median = elapsed[elapsed.len() / 2];
    assert!(
        median < 0.2 && median < cold * 0.4,
        "cold={cold:.3}s warm={median:.3}s"
    );
    let events = fixture.events();
    assert_eq!(events.lines().filter(|line| *line == "master").count(), 1);
    assert_eq!(events.lines().filter(|line| *line == "probe").count(), 8);
    fixture.update(
        "peer",
        json!({"uid":501,"home":"/changed","codexHome":"/changed",
        "declaredCodexHome":"/changed"}),
    )?;
    binding.confirm(&fixture.peer()?, deadline_after(Duration::from_secs(2))?)?;
    fixture.update(
        "peer",
        json!({"uid":502,"home":"/changed","codexHome":"/changed",
        "declaredCodexHome":"/changed"}),
    )?;
    assert!(framing::guard(&binding, Duration::from_secs(2)).is_err());
    Ok(())
}
