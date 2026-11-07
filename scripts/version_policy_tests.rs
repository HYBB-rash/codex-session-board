use super::*;
use crate::tests::{commit, fixture, fixture_git, rewrite_version};
use crate::{git, prepare, validate_history};
use std::fs;

fn save(root: &Path, rules: &Rules) -> Result<()> {
    fs::write(root.join(RULES_FILE), serde_json::to_vec_pretty(rules)?)?;
    Ok(())
}

fn activate(root: &Path, rules: &mut Rules) -> Result<()> {
    rules.versioning.rollover_after = Some(git(root, &["rev-parse", "HEAD"])?.trim().into());
    save(root, rules)
}

#[test]
fn rollover_boundaries_ranges_and_overflow() -> Result<()> {
    for (before, after) in [
        ("0.1.98", "0.1.99"),
        ("0.1.99", "0.2.0"),
        ("0.9.99", "1.0.0"),
        ("1.9.99", "2.0.0"),
        ("9.9.99", "10.0.0"),
        ("18446744073709551615.0.0", "18446744073709551615.0.1"),
    ] {
        assert_eq!(
            Version::parse(before)?.next_rollover()?,
            Version::parse(after)?
        );
    }
    for invalid in ["0.10.0", "0.1.100", "18446744073709551615.9.99"] {
        assert!(Version::parse(invalid)?.next_rollover().is_err());
    }
    for invalid in ["0.01.0", "0.1.00", "0.1", "0.1.1-alpha", "-1.0.0"] {
        assert!(Version::parse(invalid).is_err());
    }
    Ok(())
}

#[test]
fn legacy_high_patch_and_approved_release_survive_the_transition() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    rules.versioning.baseline = "0.1.99".into();
    rewrite_version(root, "0.1.99")?;
    save(root, &rules)?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    assert_eq!(
        validate_history(root, &rules)?.version.to_string(),
        "0.1.100"
    );
    prepare(root, &mut rules, Some("旧规则阶段决定"))?;
    commit(root)?;
    activate(root, &mut rules)?;
    assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.2.1");
    commit(root)?;
    assert_eq!(validate_history(root, &rules)?.version.to_string(), "0.2.1");
    Ok(())
}

#[test]
fn preparation_is_idempotent_and_manual_release_is_rejected() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    rules.versioning.baseline = "0.9.98".into();
    rewrite_version(root, "0.9.98")?;
    save(root, &rules)?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    activate(root, &mut rules)?;
    for _attempt in 0..2 {
        assert_eq!(prepare(root, &mut rules, None)?.to_string(), "1.0.0");
    }
    let before = fs::read(root.join(RULES_FILE))?;
    assert!(prepare(root, &mut rules, Some("阶段完成")).is_err());
    assert_eq!(before, fs::read(root.join(RULES_FILE))?);
    commit(root)?;
    assert_eq!(validate_history(root, &rules)?.version.to_string(), "1.0.0");
    Ok(())
}

#[test]
fn boundary_cannot_move_disappear_or_authorize_skipped_versions() -> Result<()> {
    for invalid in [None, Some("wrong"), Some("0.3.0")] {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        activate(root, &mut rules)?;
        prepare(root, &mut rules, None)?;
        commit(root)?;
        if invalid == Some("0.3.0") {
            rewrite_version(root, "0.3.0")?;
        } else {
            rules.versioning.rollover_after = invalid.map(str::to_string);
            save(root, &rules)?;
            rewrite_version(root, "0.1.3")?;
        }
        commit(root)?;
        assert!(validate_history(root, &rules).is_err());
    }
    Ok(())
}

#[test]
fn merge_inherits_rollover_from_the_second_parent_before_and_after_commit() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    rules.versioning.baseline = "0.1.98".into();
    rewrite_version(root, "0.1.98")?;
    save(root, &rules)?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    fixture_git(root, &["checkout", "-b", "feature"])?;
    activate(root, &mut rules)?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    fixture_git(root, &["checkout", "main"])?;
    fixture_git(root, &["merge", "--no-commit", "--no-ff", "feature"])?;
    assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.2.0");
    commit(root)?;
    assert_eq!(validate_history(root, &rules)?.version.to_string(), "0.2.0");
    Ok(())
}

#[test]
fn pending_merge_rejects_invalid_history_and_conflicting_real_boundaries() -> Result<()> {
    for conflicting in [false, true] {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        fixture_git(root, &["checkout", "-b", "feature"])?;
        fs::write(root.join("feature.txt"), "feature")?;
        if conflicting {
            activate(root, &mut rules)?;
            prepare(root, &mut rules, None)?;
        }
        commit(root)?;
        fixture_git(root, &["checkout", "main"])?;
        rules = Rules::parse(&fs::read_to_string(root.join(RULES_FILE))?)?;
        prepare(root, &mut rules, None)?;
        commit(root)?;
        if conflicting {
            activate(root, &mut rules)?;
            prepare(root, &mut rules, None)?;
            commit(root)?;
        }
        fixture_git(
            root,
            &["merge", "--no-commit", "--no-ff", "-s", "ours", "feature"],
        )?;
        assert!(prepare(root, &mut rules, None).is_err());
    }
    Ok(())
}

#[test]
fn moving_the_boundary_to_a_real_ancestor_or_hiding_invalid_history_fails() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    prepare(root, &mut rules, None)?;
    commit(root)?;
    let ancestor = git(root, &["rev-parse", "HEAD"])?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    activate(root, &mut rules)?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    let original_boundary = rules.versioning.rollover_after.clone();
    rules.versioning.rollover_after = Some(ancestor.trim().into());
    save(root, &rules)?;
    rewrite_version(root, "0.1.4")?;
    commit(root)?;
    rules.versioning.rollover_after = original_boundary;
    save(root, &rules)?;
    rewrite_version(root, "0.1.5")?;
    commit(root)?;
    assert!(validate_history(root, &rules).is_err());
    Ok(())
}

#[test]
fn repeat_preparation_and_rejected_changes_preserve_files_and_index() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    prepare(root, &mut rules, None)?;
    commit(root)?;
    activate(root, &mut rules)?;
    prepare(root, &mut rules, None)?;
    fixture_git(root, &["add", "--all"])?;
    let paths = ["Cargo.toml", "Cargo.lock", RULES_FILE, ".git/index"];
    let before = paths
        .map(|path| fs::read(root.join(path)))
        .into_iter()
        .collect::<io::Result<Vec<_>>>()?;
    rules = Rules::parse(&fs::read_to_string(root.join(RULES_FILE))?)?;
    prepare(root, &mut rules, None)?;
    assert!(prepare(root, &mut rules, Some("阶段完成")).is_err());
    let after = paths
        .map(|path| fs::read(root.join(path)))
        .into_iter()
        .collect::<io::Result<Vec<_>>>()?;
    assert_eq!(before, after);
    commit(root)?;
    assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.1.3");
    Ok(())
}

#[test]
fn legacy_out_of_range_head_cannot_be_normalized_backwards() -> Result<()> {
    let (directory, mut rules) = fixture()?;
    let root = directory.path();
    rules.versioning.baseline = "0.1.99".into();
    rewrite_version(root, "0.1.99")?;
    save(root, &rules)?;
    prepare(root, &mut rules, None)?;
    commit(root)?;
    activate(root, &mut rules)?;
    let before = fs::read(root.join("Cargo.toml"))?;
    assert!(prepare(root, &mut rules, None).is_err());
    assert_eq!(before, fs::read(root.join("Cargo.toml"))?);
    Ok(())
}
