use super::*;
use crate::tests::{commit, fixture, rewrite_version};
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
