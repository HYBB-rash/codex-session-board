use super::*;
use crate::prepare;
use crate::tests::{commit, fixture, fixture_git, rewrite_version};

#[test]
fn staged_version_survives_unstaged_edits_without_changing_the_index() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    prepare(root, &mut rules, None)?;
    commit(root)?;
    prepare(root, &mut rules, None)?;
    fixture_git(root, &["add", "Cargo.toml", "Cargo.lock"])?;
    rewrite_version(root, "0.9.9")?;
    let before = fs::read(root.join(".git/index"))?;
    let exported = snapshot(root)?;
    assert!(version(exported.path(), root, &rules)?);
    assert_eq!(before, fs::read(root.join(".git/index"))?);
    assert!(!crate::check(root, &rules)?);
    Ok(())
}

#[test]
fn unstaged_version_or_lockfile_fixes_cannot_authorize_a_commit() -> Result<()> {
    for partial in [false, true] {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        prepare(root, &mut rules, None)?;
        if partial {
            fixture_git(root, &["add", "Cargo.toml"])?;
        }
        let exported = snapshot(root)?;
        assert!(version(exported.path(), root, &rules).is_err());
    }
    Ok(())
}

#[test]
fn initial_and_approved_minor_versions_are_checked_from_the_snapshot() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    prepare(root, &mut rules, None)?;
    fixture_git(root, &["add", "--all"])?;
    assert!(version(snapshot(root)?.path(), root, &rules)?);
    commit(root)?;
    prepare(root, &mut rules, Some("用户决定：测试阶段已完成"))?;
    fixture_git(root, &["add", "--all"])?;
    let exported = snapshot(root)?;
    let staged_rules = Rules::parse(&fs::read_to_string(exported.path().join(RULES_FILE))?)?;
    assert!(version(exported.path(), root, &staged_rules)?);
    rules.versioning.feature_releases.clear();
    assert!(version(exported.path(), root, &rules).is_err());
    Ok(())
}

#[test]
fn staged_rollover_uses_only_the_rules_and_versions_in_the_index() -> Result<()> {
    for stage_rules in [false, true] {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        rules.versioning.baseline = "0.1.98".into();
        rewrite_version(root, "0.1.98")?;
        fs::write(root.join(RULES_FILE), serde_json::to_vec(&rules)?)?;
        prepare(root, &mut rules, None)?;
        commit(root)?;
        rules.versioning.rollover_after =
            Some(crate::git(root, &["rev-parse", "HEAD"])?.trim().into());
        fs::write(root.join(RULES_FILE), serde_json::to_vec(&rules)?)?;
        prepare(root, &mut rules, None)?;
        fixture_git(root, &["add", "Cargo.toml", "Cargo.lock"])?;
        if stage_rules {
            fixture_git(root, &["add", RULES_FILE])?;
            rules.versioning.rollover_after = Some("unstaged-invalid-boundary".into());
            fs::write(root.join(RULES_FILE), serde_json::to_vec(&rules)?)?;
            rewrite_version(root, "0.9.9")?;
        }
        let exported = snapshot(root)?;
        let staged_rules = Rules::parse(&fs::read_to_string(exported.path().join(RULES_FILE))?)?;
        assert_eq!(
            version(exported.path(), root, &staged_rules).is_ok(),
            stage_rules
        );
    }
    Ok(())
}
