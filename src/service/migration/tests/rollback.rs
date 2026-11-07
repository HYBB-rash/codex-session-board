use super::super::{Created, Plan, Stage, publish_with};
use super::fixture::{Fixture, directories, populate};
use crate::config;
use anyhow::Result;
use std::fs;
use std::os::unix::fs::symlink;

#[test]
fn second_host_backup_failure_cleans_staging_and_allows_retry() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let second = directories(&fixture.source)[2].clone();
    drop(fixture.stores.pop());
    fs::write(
        second.join("status.sqlite3"),
        "invalid second host database",
    )?;
    fs::create_dir_all(&fixture.config.data_dir)?;
    let keep = fixture.config.data_dir.join("keep");
    fs::write(&keep, "unrelated destination data")?;
    assert!(fixture.publish().is_err());
    assert_eq!(fs::read_to_string(&keep)?, "unrelated destination data");
    assert_eq!(fs::read_dir(&fixture.config.data_dir)?.count(), 1);
    assert!(!fixture.configuration().exists());
    fs::remove_file(second.join("status.sqlite3"))?;
    fixture.stores.push(populate(&second, 2)?);
    assert!(fixture.publish()?);
    fixture.assert_preserved()?;
    assert_eq!(fs::read_to_string(keep)?, "unrelated destination data");
    Ok(())
}

#[test]
fn configuration_failure_rolls_back_installed_files_and_preserves_external_additions() -> Result<()>
{
    let fixture = Fixture::new()?;
    let external = fixture.config.data_dir.join("remote-hosts/external");
    let result = publish_with(
        &fixture.configuration(),
        &fixture.config,
        &fixture.source.join("status.sqlite3"),
        |_| {
            for target in directories(&fixture.config.data_dir) {
                assert!(target.join("status.sqlite3").is_file());
            }
            fs::write(&external, "external addition")?;
            anyhow::bail!("injected configuration publication failure")
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&external)?, "external addition");
    assert!(!fixture.configuration().exists());
    for target in directories(&fixture.config.data_dir) {
        assert!(!target.join("status.sqlite3").exists());
    }
    assert!(fixture.publish()?);
    fixture.assert_preserved()?;
    assert_eq!(fs::read_to_string(external)?, "external addition");
    Ok(())
}

#[test]
fn rollback_preserves_files_replaced_by_another_writer() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("file");
    fs::write(&path, "migration data")?;
    let mut created = Created {
        entries: vec![(path.clone(), fs::symlink_metadata(&path)?)],
    };
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, "external replacement")?;
    fs::rename(replacement, &path)?;
    assert!(created.rollback().is_err());
    assert_eq!(fs::read_to_string(path)?, "external replacement");
    Ok(())
}

#[test]
fn installation_refuses_concurrent_target_entries_and_preserves_them() -> Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let fixture = Fixture::new()?;
        let plan = Plan::new(&fixture.source.join("status.sqlite3"))?;
        plan.validate(&fixture.configuration(), &fixture.config.data_dir)?;
        let mut created = Created::default();
        created.directory(&fixture.config.data_dir)?;
        let stage = Stage::new(&fixture.config.data_dir)?;
        plan.prepare(&stage.0)?;
        let external = fixture
            .config
            .data_dir
            .join(format!("status.sqlite3{suffix}"));
        fs::write(&external, b"concurrent external data")?;
        assert!(
            plan.install(&stage.0, &fixture.config.data_dir, &mut created)
                .is_err()
        );
        drop(stage);
        assert!(created.rollback().is_err());
        assert_eq!(fs::read(&external)?, b"concurrent external data");
        assert_eq!(fs::read_dir(&fixture.config.data_dir)?.count(), 1);
        assert!(!fixture.configuration().exists());
    }
    Ok(())
}

#[test]
fn configuration_race_preserves_external_files_and_symlinks() -> Result<()> {
    for linked in [false, true] {
        let fixture = Fixture::new()?;
        let path = fixture.configuration();
        let external = fixture.directory.path().join("external");
        fs::write(&external, b"concurrent external configuration")?;
        let result = publish_with(
            &path,
            &fixture.config,
            &fixture.source.join("status.sqlite3"),
            |value| {
                if linked {
                    symlink(&external, &path)?;
                } else {
                    fs::write(&path, b"concurrent external configuration")?;
                }
                config::write_json(&path, value, false)
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&path)?, b"concurrent external configuration");
        assert_eq!(fs::read(&external)?, b"concurrent external configuration");
        assert_eq!(
            fs::symlink_metadata(&path)?.file_type().is_symlink(),
            linked
        );
        assert!(!fixture.config.data_dir.exists());
    }
    Ok(())
}
