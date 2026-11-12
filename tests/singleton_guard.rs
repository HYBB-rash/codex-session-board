#![cfg(target_os = "linux")]
use anyhow::Result;
use codex_session_board::singleton::Guard;
use std::os::unix::fs::symlink;
use std::{fs, path::PathBuf};
#[path = "singleton_guard/support.rs"]
mod support;
use support::Fixture;

#[test]
fn healthy_cookie_is_dangling_and_listener_receives_no_probe() -> Result<()> {
    let fixture = Fixture::new()?;
    let cookie = fixture.temporary.join("SingletonCookie");
    assert!(!cookie.exists());
    let before = fs::symlink_metadata(&cookie)?.modified()?;
    let guard = Guard::acquire(&fixture.profile)?.unwrap();
    guard.prepare()?;
    assert_eq!(before, fs::symlink_metadata(&cookie)?.modified()?);
    fixture.listener.set_nonblocking(true)?;
    assert!(fixture.listener.accept().is_err());
    assert!(fixture.root.path().exists());
    Ok(())
}

#[test]
fn missing_cookie_recovers_but_mismatch_and_file_are_preserved() -> Result<()> {
    let fixture = Fixture::new()?;
    let cookie = fixture.temporary.join("SingletonCookie");
    fs::remove_file(&cookie)?;
    let guard = Guard::acquire(&fixture.profile)?.unwrap();
    guard.prepare()?;
    assert_eq!(fs::read_link(&cookie)?, std::path::Path::new("123456789"));
    fs::remove_file(&cookie)?;
    symlink("987654321", &cookie)?;
    assert!(guard.prepare().is_err());
    assert_eq!(fs::read_link(&cookie)?, std::path::Path::new("987654321"));
    fs::remove_file(&cookie)?;
    fs::write(&cookie, "keep")?;
    assert!(guard.prepare().is_err());
    assert_eq!(fs::read_to_string(&cookie)?, "keep");
    Ok(())
}


#[path = "singleton_guard/corners.rs"]
mod corners;
