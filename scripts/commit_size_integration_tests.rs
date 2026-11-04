use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn fixture_git(root: &Path, arguments: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_DIR", root.join(".git"))
        .env("GIT_WORK_TREE", root)
        .env("GIT_INDEX_FILE", root.join(".git/index"))
        .args(["--no-optional-locks", "-C"])
        .arg(root)
        .args([
            "-c",
            "user.name=Commit Size Test",
            "-c",
            "user.email=commit-size@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fileMode=true",
        ])
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "Git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn fixture() -> Result<TempDir> {
    let directory = tempdir()?;
    fixture_git(directory.path(), &["init", "--initial-branch=main"])?;
    fixture_git(
        directory.path(),
        &["commit", "--allow-empty", "-m", "baseline"],
    )?;
    Ok(directory)
}

fn save_baseline(root: &Path) -> Result<()> {
    fixture_git(root, &["add", "--all"])?;
    fixture_git(root, &["commit", "-m", "baseline files"])
}

fn measure_index(root: &Path) -> Result<Statistics> {
    let snapshot = tempdir()?;
    let prefix = format!("--prefix={}/", snapshot.path().display());
    fixture_git(root, &["checkout-index", "--all", &prefix])?;
    measure(root, snapshot.path())
}

#[test]
fn staged_line_limit_includes_400_and_rejects_401() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    for lines in [400, 401] {
        fs::write(root.join("added.txt"), "added\n".repeat(lines))?;
        fixture_git(root, &["add", "--all"])?;
        let total = measure_index(root)?;
        assert_eq!((total.files, total.added, total.deleted), (1, lines, 0));
        assert_eq!(total.allowed(), lines == 400);
    }
    Ok(())
}

#[test]
fn empty_files_count_toward_the_13_file_limit() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    for number in 0..14 {
        fs::write(root.join(format!("empty-{number}")), b"")?;
        if number >= 12 {
            fixture_git(root, &["add", "--all"])?;
            let total = measure_index(root)?;
            assert_eq!(
                (total.files, total.added, total.deleted),
                (number + 1, 0, 0)
            );
            assert_eq!(total.allowed(), number == 12);
        }
    }
    Ok(())
}

#[test]
fn additions_and_deletions_are_summed_without_netting() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    fs::write(root.join("replace.txt"), "old\n")?;
    fs::write(root.join("delete.txt"), "removed\nremoved\n")?;
    save_baseline(root)?;
    fs::remove_file(root.join("replace.txt"))?;
    fs::write(root.join("replace.txt"), "replacement\n")?;
    fs::remove_file(root.join("delete.txt"))?;
    fs::write(root.join("new.txt"), "first\nsecond\nthird\n")?;
    fixture_git(root, &["add", "--all"])?;
    let total = measure_index(root)?;
    assert_eq!((total.files, total.added, total.deleted), (3, 4, 3));
    Ok(())
}

#[test]
fn pure_and_modified_renames_each_count_as_one_file() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    let old = root.join("old\tname\n.txt");
    let renamed = root.join("-new => name.txt");
    let content = "preserved content\n".repeat(20);
    fs::write(&old, &content)?;
    save_baseline(root)?;
    fs::rename(&old, &renamed)?;
    fixture_git(root, &["add", "--all"])?;
    let pure = measure_index(root)?;
    assert_eq!((pure.files, pure.added, pure.deleted), (1, 0, 0));
    fs::write(&renamed, format!("{content}additional line\n"))?;
    fixture_git(root, &["add", "--all"])?;
    let modified = measure_index(root)?;
    assert_eq!(
        (modified.files, modified.added, modified.deleted),
        (1, 1, 0)
    );
    Ok(())
}

#[test]
fn executable_permission_changes_count_even_without_line_changes() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    let executable = root.join("executable");
    fs::write(&executable, b"")?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o644))?;
    save_baseline(root)?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    fixture_git(root, &["add", "--all"])?;
    let total = measure_index(root)?;
    assert_eq!((total.files, total.added, total.deleted), (1, 0, 0));
    Ok(())
}

#[test]
fn unstaged_fixes_failures_untracked_and_intent_to_add_are_ignored() -> Result<()> {
    for (staged_lines, unstaged_lines) in [(400, 401), (401, 1)] {
        let directory = fixture()?;
        let root = directory.path();
        let source = root.join("source.txt");
        fs::write(&source, "staged\n".repeat(staged_lines))?;
        fixture_git(root, &["add", "--", "source.txt"])?;
        fs::write(&source, "unstaged\n".repeat(unstaged_lines))?;
        fs::write(root.join("untracked.txt"), "untracked\n".repeat(500))?;
        fs::write(root.join("intent.txt"), "intent\n".repeat(500))?;
        fixture_git(root, &["add", "-N", "--", "intent.txt"])?;
        let total = measure_index(root)?;
        assert_eq!(
            (total.files, total.added, total.deleted),
            (1, staged_lines, 0)
        );
        assert_eq!(total.allowed(), staged_lines == 400);
    }
    Ok(())
}

#[test]
fn binary_files_remain_visible_in_statistics_for_reporting() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    fs::write(root.join("binary.dat"), b"before\0after\n")?;
    fixture_git(root, &["add", "--all"])?;
    let total = measure_index(root)?;
    assert_eq!((total.files, total.added, total.deleted), (1, 0, 0));
    assert_eq!(total.binaries, vec![b"binary.dat".to_vec()]);
    assert!(total.allowed());
    Ok(())
}

#[test]
fn unstaged_attributes_cannot_change_staged_binary_classification() -> Result<()> {
    let directory = fixture()?;
    let root = directory.path();
    fs::write(root.join(".gitattributes"), "*.bin -diff\n")?;
    save_baseline(root)?;
    fs::write(root.join(".gitattributes"), "*.data -diff\n")?;
    fs::write(root.join("sample.data"), "binary by attribute\n")?;
    fs::write(root.join("text.txt"), "first\nsecond\n")?;
    fixture_git(root, &["add", "--all"])?;
    let before = measure_index(root)?;
    assert_eq!((before.files, before.added, before.deleted), (3, 3, 1));
    assert_eq!(before.binaries, vec![b"sample.data".to_vec()]);
    fs::write(root.join(".gitattributes"), "*.data diff\n*.txt -diff\n")?;
    assert_eq!(measure_index(root)?, before);
    Ok(())
}
