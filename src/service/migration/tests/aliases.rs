use super::super::{configuration, load_source, lock, publish};
use super::fixture::Fixture;
use crate::config::{self, Config};
use anyhow::Result;
use serde_json::json;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

#[test]
fn source_loading_accepts_configuration_and_data_aliases() -> Result<()> {
    for relative in [
        "../data-alias",
        "../parent-alias/source",
        "../data-alias/../source",
    ] {
        let fixture = Fixture::new()?;
        let real = fixture.directory.path().join("config");
        let alias = fixture.directory.path().join("config-alias");
        let file_alias = fixture.directory.path().join("source-alias.json");
        fs::create_dir(&real)?;
        symlink(&real, &alias)?;
        symlink(&fixture.source, fixture.directory.path().join("data-alias"))?;
        symlink(
            fixture.directory.path(),
            fixture.directory.path().join("parent-alias"),
        )?;
        let source_config = real.join("source.json");
        config::write_json(&source_config, &json!({"data_dir": relative}), false)?;
        symlink(&source_config, &file_alias)?;
        for path in [source_config, alias.join("source.json"), file_alias] {
            assert_eq!(load_source(&path)?.data_dir, fixture.source);
        }
        let loaded = load_source(&alias.join("source.json"))?;
        assert!(publish(
            &fixture.configuration(),
            &fixture.config,
            &loaded.data_dir.join("status.sqlite3")
        )?);
        fixture.assert_preserved()?;
    }
    Ok(())
}

#[test]
fn migration_accepts_source_and_destination_root_aliases() -> Result<()> {
    let fixture = Fixture::new()?;
    let source = fixture.directory.path().join("source-alias");
    let target = fixture.directory.path().join("destination-alias");
    fs::create_dir(&fixture.config.data_dir)?;
    symlink(&fixture.source, &source)?;
    symlink(&fixture.config.data_dir, &target)?;
    let config = Config {
        data_dir: target.clone(),
        ..fixture.config.clone()
    };
    assert!(publish(
        &fixture.configuration(),
        &config,
        &source.join("status.sqlite3")
    )?);
    fixture.assert_preserved()?;
    let published = load_source(&fixture.configuration())?;
    assert_eq!(published.data_dir, fixture.config.data_dir);
    assert!(fs::symlink_metadata(source)?.file_type().is_symlink());
    assert!(fs::symlink_metadata(target)?.file_type().is_symlink());
    Ok(())
}

#[test]
fn migration_accepts_directory_ancestor_aliases_for_missing_targets() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let alias = fixture.directory.path().join("ancestor-alias");
    symlink(fixture.directory.path(), &alias)?;
    fixture.config.data_dir = alias.join("nested/data");
    let path = alias.join("settings/nested/config.json");
    assert!(publish(
        &path,
        &fixture.config,
        &alias.join("source/status.sqlite3")
    )?);
    fixture.assert_preserved()?;
    assert_eq!(
        load_source(&path)?.data_dir,
        fixture.directory.path().join("nested/data")
    );
    assert!(fs::symlink_metadata(alias)?.file_type().is_symlink());
    Ok(())
}

#[test]
fn migration_accepts_default_configuration_parent_alias_after_cli_resolution() -> Result<()> {
    let fixture = Fixture::new()?;
    let real = fixture.directory.path().join("real");
    let alias = fixture.directory.path().join("alias");
    fs::create_dir(&real)?;
    symlink(&real, &alias)?;
    let original = alias.join("config.json");
    let resolved = config::resolve_path(&original)?;
    assert_eq!(configuration(&original)?, real.join("config.json"));
    assert_eq!(configuration(&resolved)?, real.join("config.json"));
    assert!(publish(
        &resolved,
        &fixture.config,
        &fixture.source.join("status.sqlite3")
    )?);
    assert!(original.is_file());
    fixture.assert_preserved()?;
    Ok(())
}

#[test]
fn migration_accepts_relative_configuration_without_changing_current_directory() -> Result<()> {
    let fixture = Fixture::new()?;
    let current = std::env::current_dir()?;
    let mut relative = PathBuf::new();
    for _ in current.components().skip(1) {
        relative.push("..");
    }
    relative.push(fixture.configuration().strip_prefix("/")?);
    assert!(!relative.is_absolute());
    assert!(publish(
        &relative,
        &fixture.config,
        &fixture.source.join("status.sqlite3")
    )?);
    assert_eq!(std::env::current_dir()?, current);
    fixture.assert_preserved()?;
    assert!(fixture.configuration().is_file());
    Ok(())
}

#[test]
fn migration_lock_accepts_data_directory_alias() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir(&fixture.config.data_dir)?;
    let alias = fixture.directory.path().join("alias");
    symlink(&fixture.config.data_dir, &alias)?;
    let _guard = lock(&alias)?;
    assert!(fixture.config.data_dir.join("launch.lock").is_file());
    assert!(fs::symlink_metadata(alias)?.file_type().is_symlink());
    Ok(())
}

#[test]
fn source_loading_rejects_dangling_configuration_or_data_aliases() -> Result<()> {
    let fixture = Fixture::new()?;
    let alias = fixture.directory.path().join("alias");
    symlink(fixture.directory.path().join("missing"), &alias)?;
    assert!(load_source(&alias).is_err());
    let path = fixture.directory.path().join("source.json");
    config::write_json(&path, &json!({"data_dir": alias}), false)?;
    assert!(load_source(&path).is_err());
    assert!(!fixture.config.data_dir.exists());
    Ok(())
}

#[test]
fn migration_rejects_dangling_destination_root_or_parent_aliases() -> Result<()> {
    for root in [false, true] {
        let fixture = Fixture::new()?;
        let missing = fixture.directory.path().join("missing");
        let alias = fixture.directory.path().join("alias");
        symlink(&missing, &alias)?;
        let config = Config {
            data_dir: if root {
                alias.clone()
            } else {
                fixture.config.data_dir.clone()
            },
            ..fixture.config.clone()
        };
        let path = if root {
            fixture.configuration()
        } else {
            alias.join("config.json")
        };
        assert!(publish(&path, &config, &fixture.source.join("status.sqlite3")).is_err());
        assert!(!missing.exists());
        assert!(!fixture.config.data_dir.exists());
        assert!(fs::symlink_metadata(alias)?.file_type().is_symlink());
    }
    Ok(())
}

#[test]
fn migration_rejects_final_configuration_symlinks_through_parent_alias() -> Result<()> {
    for existing in [false, true] {
        let fixture = Fixture::new()?;
        let alias = fixture.directory.path().join("alias");
        symlink(fixture.directory.path(), &alias)?;
        let external = fixture.directory.path().join("external");
        if existing {
            fs::write(&external, b"external configuration")?;
        }
        let path = alias.join("config.json");
        symlink(&external, &path)?;
        assert!(configuration(&path).is_err());
        assert!(
            publish(
                &path,
                &fixture.config,
                &fixture.source.join("status.sqlite3")
            )
            .is_err()
        );
        assert!(fs::symlink_metadata(path)?.file_type().is_symlink());
        if existing {
            assert_eq!(fs::read(external)?, b"external configuration");
        } else {
            assert!(!external.exists());
        }
        assert!(!fixture.config.data_dir.exists());
    }
    Ok(())
}
