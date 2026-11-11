use super::*;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

fn directory(root: &Path, name: &str) -> Result<PathBuf> {
    let path = root.join(name);
    fs::create_dir(&path)?;
    fs::set_permissions(&path, Permissions::from_mode(0o700))?;
    Ok(path)
}

fn private_name() -> String {
    format!("codex-app-tools-{}", rustix::process::getuid().as_raw())
}

fn listener(directory: &Path, name: &str) -> Result<(PathBuf, UnixListener)> {
    let path = directory.join(name);
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok((path, listener))
}

fn connected(configured: &Path) -> Result<UnixStream> {
    connect(configured, deadline_after(Duration::from_millis(200))?)
}

#[test]
fn directory_migration_recovers_in_both_directions_with_missing_original_directory() -> Result<()> {
    for (old, new) in [
        ("codex-app-tools".to_string(), private_name()),
        (private_name(), "codex-app-tools".to_string()),
    ] {
        let root = tempfile::Builder::new().prefix("sb").tempdir()?;
        let current = directory(root.path(), &new)?;
        let (_, server) = listener(&current, "current.sock")?;
        let _client = connected(&root.path().join(old).join("missing.sock"))?;
        let (_accepted, _) = server.accept()?;
    }
    Ok(())
}

#[test]
fn directory_migration_rejects_multiple_live_endpoints_across_both_directories() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let old = directory(root.path(), "codex-app-tools")?;
    let new = directory(root.path(), &private_name())?;
    let (_, _old_server) = listener(&old, "old.sock")?;
    let (_, _new_server) = listener(&new, "new.sock")?;
    let error = connected(&old.join("missing.sock")).unwrap_err();
    assert!(error.to_string().contains("多个分区连接仍存活"));
    Ok(())
}

#[test]
fn directory_migration_preserves_live_preferred_endpoint() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let old = directory(root.path(), "codex-app-tools")?;
    let new = directory(root.path(), &private_name())?;
    let (preferred, server) = listener(&old, "preferred.sock")?;
    let (_, other) = listener(&new, "other.sock")?;
    fs::set_permissions(&new, Permissions::from_mode(0o777))?;
    let _client = connected(&preferred)?;
    let (_accepted, _) = server.accept()?;
    assert_eq!(
        other.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    Ok(())
}

#[test]
fn directory_migration_rechecks_directory_and_socket_permissions() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let old = root.path().join("codex-app-tools").join("missing.sock");
    let new = directory(root.path(), &private_name())?;
    let (socket, _server) = listener(&new, "new.sock")?;
    fs::set_permissions(&new, Permissions::from_mode(0o777))?;
    assert!(connected(&old).is_err());
    fs::set_permissions(&new, Permissions::from_mode(0o700))?;
    fs::set_permissions(&socket, Permissions::from_mode(0o666))?;
    assert!(connected(&old).is_err());
    fs::set_permissions(&socket, Permissions::from_mode(0o600))?;
    let _client = connected(&old)?;
    Ok(())
}

#[test]
fn directory_migration_ignores_other_users_and_preserves_custom_directory_scope() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let other_name = format!("codex-app-tools-{}", rustix::process::getuid().as_raw() + 1);
    let other = directory(root.path(), &other_name)?;
    let (_, other_server) = listener(&other, "other.sock")?;
    let old = root.path().join("codex-app-tools").join("missing.sock");
    assert!(connected(&old).is_err());
    assert_eq!(
        other_server.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let new = directory(root.path(), &private_name())?;
    let (_, new_server) = listener(&new, "new.sock")?;
    let custom = directory(root.path(), "custom")?;
    assert!(connected(&custom.join("missing.sock")).is_err());
    assert_eq!(
        new_server.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let (_, server) = listener(&custom, "current.sock")?;
    let _client = connected(&custom.join("missing.sock"))?;
    let (_accepted, _) = server.accept()?;
    Ok(())
}

#[test]
fn directory_migration_preserves_deadline_and_rejects_symlink_directory() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let actual = directory(root.path(), "actual")?;
    let (_, server) = listener(&actual, "new.sock")?;
    let new = root.path().join(private_name());
    std::os::unix::fs::symlink(&actual, &new)?;
    let old = root.path().join("codex-app-tools").join("missing.sock");
    assert!(connected(&old).is_err());
    fs::remove_file(&new)?;
    fs::rename(&actual, &new)?;
    let started = Instant::now();
    assert!(connect(&old, started).is_err());
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(
        server.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let _client = connected(&old)?;
    Ok(())
}

#[test]
fn directory_migration_rejects_unsafe_second_directory_after_finding_live_candidate() -> Result<()>
{
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let old = directory(root.path(), "codex-app-tools")?;
    let new = directory(root.path(), &private_name())?;
    let (preferred, server) = listener(&old, "old.sock")?;
    let missing = old.join("missing.sock");
    fs::set_permissions(&new, Permissions::from_mode(0o777))?;
    assert!(connected(&missing).is_err());
    fs::set_permissions(&new, Permissions::from_mode(0o700))?;
    fs::write(new.join("invalid.sock"), b"not a socket")?;
    assert!(connected(&missing).is_err());
    fs::set_permissions(&preferred, Permissions::from_mode(0o666))?;
    let (_, _replacement) = listener(&new, "new.sock")?;
    assert!(connected(&preferred).is_err());
    let (mut accepted, _) = server.accept()?;
    assert_eq!(accepted.read(&mut [0; 1])?, 0);
    Ok(())
}

#[test]
fn directory_migration_times_out_on_second_directory_without_sending_rpc_frames() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    let old = directory(root.path(), "codex-app-tools")?;
    let new = directory(root.path(), &private_name())?;
    let (_, server) = listener(&old, "old.sock")?;
    let (blocked, full) = listener(&new, "full.sock")?;
    rustix::net::listen(&full, 0)?;
    let _queued = connect_unix(&blocked, deadline_after(Duration::from_secs(1))?)?;
    let started = Instant::now();
    let error = connect(
        &old.join("missing.sock"),
        started + Duration::from_millis(50),
    )
    .unwrap_err();
    #[cfg(target_os = "linux")]
    assert_eq!(error.downcast_ref::<io::Error>().unwrap().kind(), io::ErrorKind::TimedOut);
    #[cfg(target_os = "macos")]
    assert_eq!(error.to_string(), "多个分区连接仍存活，无法唯一确定新 socket");
    assert!(started.elapsed() < Duration::from_millis(500));
    let (mut accepted, _) = server.accept()?;
    assert_eq!(accepted.read(&mut [0; 1])?, 0);
    Ok(())
}

#[test]
fn directory_migration_expands_only_exact_standard_names() -> Result<()> {
    let root = tempfile::Builder::new().prefix("sb").tempdir()?;
    for name in [
        "codex-app-tools-backup".to_string(),
        format!("{}-backup", private_name()),
        format!("codex-app-tools-0{}", rustix::process::getuid().as_raw()),
    ] {
        let parent = root.path().join(name);
        assert_eq!(directories(&parent.join("missing.sock"))?, vec![parent]);
    }
    Ok(())
}
