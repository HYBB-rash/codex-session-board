use super::super::{lock, publish};
use super::fixture::{Fixture, directories};
use anyhow::Result;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

#[test]
fn migration_refuses_existing_root_or_host_data_before_publication() -> Result<()> {
    for host_conflict in [false, true] {
        let fixture = Fixture::new()?;
        let destination = if host_conflict {
            directories(&fixture.config.data_dir)[1].clone()
        } else {
            fixture.config.data_dir.clone()
        };
        fs::create_dir_all(&destination)?;
        let existing = destination.join(if host_conflict {
            "keep"
        } else {
            "status.sqlite3"
        });
        fs::write(&existing, "existing target")?;
        assert!(fixture.publish().is_err());
        assert_eq!(fs::read_to_string(existing)?, "existing target");
        assert!(!fixture.configuration().exists());
        if host_conflict {
            assert!(!fixture.config.data_dir.join("status.sqlite3").exists());
        }
    }
    Ok(())
}

#[test]
fn migration_refuses_existing_sqlite_sidecars() -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let fixture = Fixture::new()?;
        fs::create_dir_all(&fixture.config.data_dir)?;
        let existing = fixture
            .config
            .data_dir
            .join(format!("status.sqlite3{suffix}"));
        fs::write(&existing, "unrelated existing SQLite sidecar")?;
        assert!(fixture.publish().is_err());
        assert_eq!(
            fs::read_to_string(existing)?,
            "unrelated existing SQLite sidecar"
        );
        assert!(!fixture.config.data_dir.join("status.sqlite3").exists());
        assert!(!fixture.configuration().exists());
    }
    Ok(())
}

fn source_link(fixture: &Fixture, case: &str) -> PathBuf {
    let host = directories(&fixture.source)[1].clone();
    match case {
        "hosts" => fixture.source.join("remote-hosts"),
        "host" => host,
        "database" => host.join("status.sqlite3"),
        "identity" => host.join("source.json"),
        "progress" => host.join("scan-progress.json"),
        "root-database" => fixture.source.join("status.sqlite3"),
        suffix => fixture.source.join(format!("status.sqlite3-{suffix}")),
    }
}

fn source_root(fixture: &Fixture, aliased: bool) -> Result<PathBuf> {
    if aliased {
        let alias = fixture.directory.path().join("source-alias");
        symlink(&fixture.source, &alias)?;
        Ok(alias)
    } else {
        Ok(fixture.source.clone())
    }
}

fn destination_link(fixture: &Fixture, case: &str) -> PathBuf {
    match case {
        "hosts" => fixture.config.data_dir.join("remote-hosts"),
        "host" => directories(&fixture.config.data_dir)[1].clone(),
        "database" => fixture.config.data_dir.join("status.sqlite3"),
        "configuration" => fixture.configuration(),
        suffix => fixture
            .config
            .data_dir
            .join(format!("status.sqlite3-{suffix}")),
    }
}

#[test]
fn migration_rejects_source_symlinks_without_following_them() -> Result<()> {
    for case in [
        "hosts",
        "host",
        "database",
        "identity",
        "progress",
        "root-database",
        "wal",
        "shm",
        "journal",
    ] {
        for aliased in [false, true] {
            let mut fixture = Fixture::new()?;
            fixture.stores.clear();
            let path = source_link(&fixture, case);
            if !path.exists() {
                fs::write(&path, "existing SQLite sidecar")?;
            }
            let external = fixture.directory.path().join("external");
            fs::rename(&path, &external)?;
            let before = fs::read(&external).ok();
            symlink(&external, &path)?;
            let source = source_root(&fixture, aliased)?;
            assert!(
                publish(
                    &fixture.configuration(),
                    &fixture.config,
                    &source.join("status.sqlite3")
                )
                .is_err(),
                "{case}, alias={aliased}"
            );
            assert_eq!(fs::read(&external).ok(), before);
            assert!(!fixture.config.data_dir.exists());
            assert!(!fixture.configuration().exists());
        }
    }
    Ok(())
}

#[test]
fn migration_rejects_destination_symlinks_including_dangling_configuration() -> Result<()> {
    for case in [
        "hosts",
        "host",
        "database",
        "wal",
        "shm",
        "journal",
        "configuration",
    ] {
        for aliased in [false, true] {
            let mut fixture = Fixture::new()?;
            if aliased {
                fs::create_dir(&fixture.config.data_dir)?;
                let alias = fixture.directory.path().join("destination-alias");
                symlink(&fixture.config.data_dir, &alias)?;
                fixture.config.data_dir = alias;
            }
            let external = fixture.directory.path().join("external");
            fs::create_dir(&external)?;
            fs::write(external.join("keep"), "untouched")?;
            let path = destination_link(&fixture, case);
            fs::create_dir_all(path.parent().unwrap())?;
            let target = if case == "configuration" {
                external.join("missing")
            } else {
                external.clone()
            };
            symlink(target, &path)?;
            assert!(fixture.publish().is_err(), "{case}, alias={aliased}");
            assert_eq!(fs::read_to_string(external.join("keep"))?, "untouched");
            assert_eq!(fs::read_dir(&external)?.count(), 1);
        }
    }
    Ok(())
}

#[test]
fn migration_rejects_configuration_overlapping_database_or_sidecar() -> Result<()> {
    for sidecar in [false, true] {
        let fixture = Fixture::new()?;
        let path = if sidecar {
            directories(&fixture.config.data_dir)[1].join("source.json")
        } else {
            fixture.config.data_dir.join("status.sqlite3")
        };
        assert!(
            publish(
                &path,
                &fixture.config,
                &fixture.source.join("status.sqlite3")
            )
            .is_err()
        );
        assert!(!fixture.config.data_dir.exists());
    }
    Ok(())
}

#[test]
fn migration_refuses_existing_configuration_before_creating_destination() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.configuration();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, b"existing configuration")?;
    assert!(fixture.publish().is_err());
    assert_eq!(fs::read(path)?, b"existing configuration");
    assert!(!fixture.config.data_dir.exists());
    Ok(())
}

#[test]
fn migration_refuses_runtime_lock_symlinks_before_opening_external_files() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let data = directory.path().join("data");
    fs::create_dir(&data)?;
    for existing in [true, false] {
        let external = directory.path().join("external");
        if existing {
            fs::write(&external, b"existing lock target")?;
        }
        let path = data.join("launch.lock");
        symlink(&external, &path)?;
        assert!(lock(&data).is_err());
        if existing {
            assert_eq!(fs::read(&external)?, b"existing lock target");
            fs::remove_file(&external)?;
        } else {
            assert!(!external.exists());
        }
        fs::remove_file(path)?;
    }
    Ok(())
}
