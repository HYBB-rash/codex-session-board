#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "singleton_guard/support.rs"]
mod support;
use anyhow::Result;
use codex_session_board::{app, config, observer};
use fs2::FileExt;
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use support::Fixture;

fn configuration(fixture: &Fixture) -> Result<config::Config> {
    let path = fixture.root.path().join("config.json");
    fs::write(
        &path,
        serde_json::json!({"data_dir":"data", "sync_titles":false,
        "desktop_socket":"", "codex_command":["/usr/bin/false"],
        "client_singleton_profile":"profile", "open_command":["/usr/bin/true"],
        "poll_seconds":1,"rpc_timeout_seconds":1,"runtime_poll_seconds":1})
        .to_string(),
    )?;
    config::load(Some(&path))
}

#[test]
fn navigation_repairs_and_blocks_launch_on_wrong_cookie() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut config = configuration(&fixture)?;
    assert_eq!(
        config.client_singleton_profile,
        Some(fixture.profile.clone())
    );
    fs::create_dir_all(&config.data_dir)?;
    let marker = fixture.root.path().join("launched");
    config.open_command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "printf %s \"$1\" > \"$0\"".into(),
        marker.display().to_string(),
    ];
    let cookie = fixture.temporary.join("SingletonCookie");
    fs::remove_file(&cookie)?;
    app::open_conversation("11111111-1111-4111-8111-111111111111", &config)?;
    assert!(marker.exists());
    fs::remove_file(&marker)?;
    fs::remove_file(&cookie)?;
    std::os::unix::fs::symlink("999", &cookie)?;
    assert!(app::open_conversation("11111111-1111-4111-8111-111111111111", &config).is_err());
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn observer_holds_directory_and_shutdown_releases_it() -> Result<()> {
    let fixture = Fixture::new()?;
    let cookie = fixture.temporary.join("SingletonCookie");
    fs::remove_file(&cookie)?;
    let app = Arc::new(app::App::new(configuration(&fixture)?)?);
    observer::start(&app)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.health.lock().unwrap()["singletonProtected"] != true && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let protected = app.health.lock().unwrap()["singletonProtected"] == true;
    let directory = fs::File::open(&fixture.temporary)?;
    let locked = directory.try_lock_exclusive().is_err();
    app.shutdown()?;
    assert!(protected && locked);
    assert!(fs::symlink_metadata(cookie).is_err());
    directory.try_lock_exclusive()?;
    assert!(fixture.listener.local_addr().is_ok());
    Ok(())
}

#[test]
fn navigation_retains_guard_until_launcher_exits() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut config = configuration(&fixture)?;
    fs::create_dir_all(&config.data_dir)?;
    let marker = fixture.root.path().join("release-launcher");
    config.open_command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while [ ! -f \"$0\" ]; do sleep 0.02; done".into(),
        marker.display().to_string(),
    ];
    app::open_conversation("11111111-1111-4111-8111-111111111111", &config)?;
    let directory = fs::File::open(&fixture.temporary)?;
    let locked = directory.try_lock_exclusive().is_err();
    fs::write(marker, "exit")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while directory.try_lock_exclusive().is_err() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(locked);
    directory.try_lock_exclusive()?;
    Ok(())
}

#[test]
fn overlapping_runtimes_keep_protection_until_both_shutdown() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = Arc::new(app::App::new(configuration(&fixture)?)?);
    let mut second_config = configuration(&fixture)?;
    second_config.data_dir = fixture.root.path().join("second-data");
    let second = Arc::new(app::App::new(second_config)?);
    for runtime in [&first, &second] {
        observer::start(runtime)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.health.lock().unwrap()["singletonProtected"] != true
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(runtime.health.lock().unwrap()["singletonProtected"], true);
    }
    let directory = fs::File::open(&fixture.temporary)?;
    assert!(directory.try_lock_exclusive().is_err());
    first.shutdown()?;
    assert!(directory.try_lock_exclusive().is_err());
    second.shutdown()?;
    directory.try_lock_exclusive()?;
    Ok(())
}
