use super::super::paths::SIDECARS;
use super::fixture::{Fixture, directories, runtime_files, stored};
use anyhow::Result;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn migration_preserves_root_and_all_host_wal_state_and_only_allowed_sidecars() -> Result<()> {
    let fixture = Fixture::new()?;
    for directory in directories(&fixture.source) {
        assert!(fs::metadata(directory.join("status.sqlite3-wal"))?.len() > 0);
        runtime_files(&directory)?;
    }
    assert!(fixture.publish()?);
    fixture.assert_preserved()?;
    for (source, target) in directories(&fixture.source)
        .iter()
        .zip(directories(&fixture.config.data_dir))
    {
        for name in [
            "server.json",
            "launch.lock",
            "server.log",
            "credentials.json",
            "desktop.sock",
        ] {
            assert!(source.join(name).exists());
            assert!(!target.join(name).exists());
        }
        if source != &fixture.source {
            for name in SIDECARS {
                assert_eq!(fs::read(source.join(name))?, fs::read(target.join(name))?);
                assert_eq!(
                    fs::metadata(target.join(name))?.permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }
    assert!(fixture.configuration().is_file());
    Ok(())
}

#[test]
fn migration_copies_host_databases_when_the_main_database_is_absent() -> Result<()> {
    let mut fixture = Fixture::new()?;
    drop(fixture.stores.remove(0));
    fs::remove_file(fixture.source.join("status.sqlite3"))?;
    assert!(!fixture.publish()?);
    assert!(!fixture.config.data_dir.join("status.sqlite3").exists());
    for (source, target) in directories(&fixture.source)
        .iter()
        .zip(directories(&fixture.config.data_dir))
        .skip(1)
    {
        assert_eq!(stored(source)?, stored(&target)?);
    }
    assert!(fixture.configuration().is_file());
    Ok(())
}
