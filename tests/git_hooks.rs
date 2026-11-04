use anyhow::{Result, ensure};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn git(root: &Path, arguments: &[&str]) -> Result<Output> {
    Ok(Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_DIR", root.join(".git"))
        .env("GIT_WORK_TREE", root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .args([
            "-c",
            "user.name=Hook Test",
            "-c",
            "user.email=hook-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(arguments)
        .output()?)
}

fn successful_git(root: &Path, arguments: &[&str]) -> Result<Output> {
    let output = git(root, arguments)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn install(root: &Path) -> Result<Output> {
    Ok(Command::new("bash")
        .arg(root.join("scripts/install-git-hooks"))
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()?)
}

fn source(root: &Path, value: u8) -> Result<()> {
    fs::write(
        root.join("src/lib.rs"),
        format!(
            "pub fn value() -> u8 {{ {value} }}\n\
             #[test]\nfn staged_value_is_valid() {{ assert_eq!(value(), 42); }}\n\
             #[test]\nfn git_environment_is_cleared() {{\n\
             assert!(std::env::var_os(\"GIT_DIR\").is_none());\n\
             assert!(std::env::var_os(\"GIT_WORK_TREE\").is_none());\n\
             assert!(std::env::var_os(\"GIT_INDEX_FILE\").is_none());\n}}\n"
        ),
    )?;
    Ok(())
}

fn fixture(value: u8) -> Result<TempDir> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    fs::create_dir(root.join(".githooks"))?;
    fs::create_dir(root.join("scripts"))?;
    fs::create_dir(root.join("src"))?;
    fs::write(
        root.join(".githooks/pre-commit"),
        include_str!("../.githooks/pre-commit"),
    )?;
    fs::set_permissions(
        root.join(".githooks/pre-commit"),
        fs::Permissions::from_mode(0o755),
    )?;
    fs::write(
        root.join("scripts/install-git-hooks"),
        include_str!("../scripts/install-git-hooks"),
    )?;
    fs::write(root.join(".gitignore"), "/target/\n")?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='hook-fixture'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    fs::write(
        root.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname='hook-fixture'\nversion='0.1.0'\n",
    )?;
    source(root, value)?;
    successful_git(root, &["init", "--initial-branch=main"])?;
    let installed = install(root)?;
    ensure!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    successful_git(root, &["add", "--all"])?;
    Ok(directory)
}

#[test]
fn passing_staged_tests_allow_a_commit_and_installation_is_idempotent() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    assert!(install(root)?.status.success());
    let output = successful_git(root, &["commit", "-m", "passing fixture"])?;
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("允许提交")
            || String::from_utf8_lossy(&output.stderr).contains("允许提交")
    );
    successful_git(root, &["rev-parse", "--verify", "HEAD"])?;
    Ok(())
}

#[test]
fn failing_staged_tests_block_commits_and_preserve_the_index() -> Result<()> {
    let directory = fixture(0)?;
    let root = directory.path();
    let before = successful_git(root, &["write-tree"])?;
    let output = git(root, &["commit", "-m", "failing fixture"])?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("提交已阻止"));
    assert!(
        !git(root, &["rev-parse", "--verify", "HEAD"])?
            .status
            .success()
    );
    assert_eq!(before.stdout, successful_git(root, &["write-tree"])?.stdout);
    Ok(())
}

#[test]
fn an_unstaged_fix_does_not_hide_failing_staged_code() -> Result<()> {
    let directory = fixture(0)?;
    let root = directory.path();
    source(root, 42)?;
    let before = fs::read(root.join("src/lib.rs"))?;
    assert!(
        !git(root, &["commit", "-m", "still failing"])?
            .status
            .success()
    );
    assert_eq!(before, fs::read(root.join("src/lib.rs"))?);
    assert!(
        !git(root, &["rev-parse", "--verify", "HEAD"])?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn an_unstaged_failure_does_not_change_the_passing_commit_snapshot() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    source(root, 0)?;
    let before = fs::read(root.join("src/lib.rs"))?;
    successful_git(root, &["commit", "-m", "passing staged snapshot"])?;
    assert_eq!(before, fs::read(root.join("src/lib.rs"))?);
    let committed = successful_git(root, &["show", "HEAD:src/lib.rs"])?;
    assert!(String::from_utf8_lossy(&committed.stdout).contains("{ 42 }"));
    Ok(())
}

#[test]
fn installation_preserves_existing_hooks_configuration() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    successful_git(
        root,
        &["config", "--local", "core.hooksPath", "custom-hooks"],
    )?;
    assert!(!install(root)?.status.success());
    let configured = successful_git(root, &["config", "--get", "core.hooksPath"])?;
    assert_eq!(String::from_utf8(configured.stdout)?.trim(), "custom-hooks");
    Ok(())
}
