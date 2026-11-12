use super::*;
use fs2::FileExt;
use std::{
    fs::File,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Barrier},
};

#[test]
fn simultaneous_repairs_converge_without_overwrite() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.temporary.join("SingletonCookie"))?;
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let profile = fixture.profile.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || -> Result<()> {
                let guard = Guard::acquire(&profile)?.expect("running");
                barrier.wait();
                guard.prepare()
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker")?;
    }
    assert_eq!(
        fs::read_link(fixture.temporary.join("SingletonCookie"))?,
        PathBuf::from("123456789")
    );
    Ok(())
}

#[test]
fn generation_change_invalidates_old_guard_and_uses_current_cookie() -> Result<()> {
    let fixture = Fixture::new()?;
    let old = Guard::acquire(&fixture.profile)?.expect("running");
    fs::remove_file(fixture.profile.join("SingletonCookie"))?;
    fs::remove_file(fixture.temporary.join("SingletonCookie"))?;
    symlink("777777", fixture.profile.join("SingletonCookie"))?;
    assert!(old.prepare().is_err());
    assert!(fs::symlink_metadata(fixture.temporary.join("SingletonCookie")).is_err());
    Guard::acquire(&fixture.profile)?
        .expect("current")
        .prepare()?;
    assert_eq!(
        fs::read_link(fixture.temporary.join("SingletonCookie"))?,
        PathBuf::from("777777")
    );
    Ok(())
}

#[test]
fn missing_listener_or_socket_never_recreates_cookie() -> Result<()> {
    let fixture = Fixture::new()?;
    let guard = Guard::acquire(&fixture.profile)?.expect("running");
    fs::remove_file(fixture.temporary.join("SingletonCookie"))?;
    drop(fixture.listener);
    assert!(guard.prepare().is_err());
    assert!(Guard::acquire(&fixture.profile).is_err());
    assert!(fs::symlink_metadata(fixture.temporary.join("SingletonCookie")).is_err());
    Ok(())
}

#[test]
fn replaced_directory_and_changed_permissions_invalidate_guard() -> Result<()> {
    let fixture = Fixture::new()?;
    let guard = Guard::acquire(&fixture.profile)?.expect("running");
    fs::set_permissions(&fixture.temporary, fs::Permissions::from_mode(0o755))?;
    assert!(guard.prepare().is_err());
    fs::set_permissions(&fixture.temporary, fs::Permissions::from_mode(0o700))?;
    fs::rename(&fixture.temporary, fixture.root.path().join("old"))?;
    fs::create_dir(&fixture.temporary)?;
    assert!(guard.prepare().is_err());
    assert!(fs::read_dir(&fixture.temporary)?.next().is_none());
    Ok(())
}

#[test]
fn directory_lock_lasts_until_last_guard_is_dropped() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = Guard::acquire(&fixture.profile)?.expect("first");
    let second = Guard::acquire(&fixture.profile)?.expect("second");
    let directory = File::open(&fixture.temporary)?;
    assert!(directory.try_lock_exclusive().is_err());
    drop(first);
    assert!(directory.try_lock_exclusive().is_err());
    drop(second);
    directory.try_lock_exclusive()?;
    Ok(())
}

#[test]
fn cold_start_partial_markers_and_foreign_pid_are_distinct() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.profile.join("SingletonLock"))?;
    assert!(Guard::acquire(&fixture.profile).is_err());
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")?;
    symlink(
        format!("{}-1", hostname.trim()),
        fixture.profile.join("SingletonLock"),
    )?;
    assert!(Guard::acquire(&fixture.profile).is_err());
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        fs::remove_file(fixture.profile.join(marker))?;
    }
    assert!(Guard::acquire(&fixture.profile)?.is_none());
    Ok(())
}
