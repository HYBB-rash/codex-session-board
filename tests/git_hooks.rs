use anyhow::{Result, ensure};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const COMMENT_SYMBOLS_SOURCE: &str = r####"
/** Documentation may contain ordinary comment symbols. */
pub fn string_symbols() -> &'static str {
    "escaped quote: \" // text /* text */"
}

/// Raw string contents are not comments.
pub fn raw_string_symbols() -> &'static str {
    r###"quote " // raw text /* raw text */"###
}

/// Byte string contents are not comments.
pub fn byte_string_symbols() -> &'static [u8] {
    br#"// bytes /* bytes */"#
}
"####;

fn git(root: &Path, arguments: &[&str]) -> Result<Output> {
    git_index(root, arguments, &root.join(".git/index"))
}

fn git_index(root: &Path, arguments: &[&str], index: &Path) -> Result<Output> {
    Ok(Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_DIR", root.join(".git"))
        .env("GIT_WORK_TREE", root)
        .env("GIT_INDEX_FILE", index)
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
    fixture_with_comments(value, "")
}

fn fixture_with_comments(value: u8, comments: &str) -> Result<TempDir> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    fixture_files(root)?;
    source(root, value)?;
    prepend_source(root, comments)?;
    successful_cargo(root, &["generate-lockfile", "--offline"])?;
    successful_git(root, &["init", "--initial-branch=main"])?;
    successful_git(root, &["add", "--all"])?;
    successful_git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-m",
            "comment baseline",
        ],
    )?;
    let baseline = successful_git(root, &["rev-parse", "HEAD"])?;
    let baseline = String::from_utf8(baseline.stdout)?;
    activate_checks(root, baseline.trim())?;
    Ok(directory)
}

fn activate_checks(root: &Path, baseline: &str) -> Result<()> {
    let rules = serde_json::json!({
        "schema_version": 1,
        "versioning": {"baseline": "0.1.0", "feature_releases": []},
        "rust_comments": {"baseline_commit": baseline}
    });
    fs::write(root.join("project-rules.json"), serde_json::to_vec(&rules)?)?;
    successful_cargo(
        root,
        &[
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "--example",
            "project_check",
            "--",
            "--prepare-commit",
        ],
    )?;
    let installed = install(root)?;
    ensure!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    successful_git(root, &["add", "--all"])?;
    Ok(())
}

fn fixture_files(root: &Path) -> Result<()> {
    for directory in [".githooks", "scripts", "src"] {
        fs::create_dir(root.join(directory))?;
    }
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
    fs::write(root.join("clippy.toml"), include_str!("../clippy.toml"))?;
    fixture_manifest(root)?;
    copy_checker(root)?;
    fs::write(
        root.join("project-rules.json"),
        r#"{"schema_version":1,"versioning":{"baseline":"0.1.0","feature_releases":[]}}"#,
    )?;
    Ok(())
}

fn fixture_manifest(root: &Path) -> Result<()> {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='hook-fixture'\nversion='0.1.1'\nedition='2024'\n\
         \n[[example]]\nname='project_check'\npath='scripts/project_check.rs'\ntest=false\n\
         \n[lints.clippy]\nunwrap_used='deny'\nexpect_used='deny'\ntoo_many_lines='deny'\n\
         \n[lints.rust]\nunsafe_code='forbid'\n\
         \n[dependencies]\nanyhow='1'\nserde_json='1'\nra-ap-rustc_lexer='=0.183.0'\n\
         clap={version='4',features=['derive']}\nregex='1'\ntoml='=1.1.2'\n\
         serde={version='1',features=['derive']}\n\
         \n[dev-dependencies]\ntempfile='3'\n",
    )?;
    fs::copy(
        std::env::current_dir()?.join("Cargo.lock"),
        root.join("Cargo.lock"),
    )?;
    Ok(())
}

fn copy_checker(root: &Path) -> Result<()> {
    let scripts = std::env::current_dir()?.join("scripts");
    for entry in fs::read_dir(scripts)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".rs") || entry.path().is_dir() {
            copy_fixture_path(&entry.path(), &root.join("scripts").join(name.as_ref()))?;
        }
    }
    let check = root.join("scripts/check");
    fs::write(&check, include_str!("../scripts/check"))?;
    fs::set_permissions(check, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn copy_fixture_path(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_fixture_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, destination)?;
    }
    Ok(())
}

fn successful_cargo(root: &Path, arguments: &[&str]) -> Result<Output> {
    let output = Command::new("cargo")
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn prepend_source(root: &Path, prefix: &str) -> Result<()> {
    let path = root.join("src/lib.rs");
    let original = fs::read_to_string(&path)?;
    fs::write(path, format!("{prefix}{original}"))?;
    Ok(())
}

fn blocked_commit(root: &Path, message: &str) -> Result<Output> {
    let head = successful_git(root, &["rev-parse", "HEAD"])?;
    let index = successful_git(root, &["write-tree"])?;
    let output = git(root, &["commit", "-m", message])?;
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        head.stdout,
        successful_git(root, &["rev-parse", "HEAD"])?.stdout
    );
    assert_eq!(index.stdout, successful_git(root, &["write-tree"])?.stdout);
    Ok(output)
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
    let check = Command::new("/bin/bash")
        .arg("scripts/check")
        .current_dir(root)
        .output()?;
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    Ok(())
}

#[test]
fn failing_staged_tests_block_commits_and_preserve_the_index() -> Result<()> {
    let directory = fixture(0)?;
    let root = directory.path();
    let output = blocked_commit(root, "failing fixture")?;
    assert!(String::from_utf8_lossy(&output.stderr).contains("提交已阻止"));
    Ok(())
}

#[test]
fn an_unstaged_fix_does_not_hide_failing_staged_code() -> Result<()> {
    let directory = fixture(0)?;
    let root = directory.path();
    source(root, 42)?;
    let before = fs::read(root.join("src/lib.rs"))?;
    blocked_commit(root, "still failing")?;
    assert_eq!(before, fs::read(root.join("src/lib.rs"))?);
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

#[test]
fn clippy_failure_blocks_a_commit_even_when_unit_tests_would_pass() -> Result<()> {
    for (method, lint) in [
        ("unwrap()", "unwrap_used"),
        ("expect(\"fixed test input\")", "expect_used"),
    ] {
        let directory = fixture(42)?;
        let root = directory.path();
        fs::write(
            root.join("src/lib.rs"),
            format!(
                "pub fn value() -> u8 {{ \"42\".parse::<u8>().{method} }}\n\
                 #[test]\nfn passing_test() {{ assert_eq!(value(), 42); }}\n"
            ),
        )?;
        successful_git(root, &["add", "--all"])?;
        let output = blocked_commit(root, "blocked by clippy")?;
        assert!(String::from_utf8_lossy(&output.stderr).contains("Clippy"));
        assert!(String::from_utf8_lossy(&output.stderr).contains(lint));
    }
    Ok(())
}

#[test]
fn unwrap_and_expect_remain_allowed_inside_tests() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn value() -> u8 { 42 }\n\
         #[test]\nfn test_helpers_are_allowed() {\n\
         assert_eq!(value().checked_add(0).unwrap(), 42);\n\
         assert_eq!(value().checked_add(0).expect(\"test value\"), 42);\n}\n",
    )?;
    successful_git(root, &["add", "--all"])?;
    successful_git(root, &["commit", "-m", "test unwraps are allowed"])?;
    Ok(())
}

fn limited_function_source(body_lines: usize) -> String {
    let increments = body_lines - 2;
    format!(
        "pub fn value() -> u8 {{\nlet mut value = {}u8;\n{}value\n}}\n\
         #[test]\nfn passing_test() {{ assert_eq!(value(), 42); }}\n",
        42 - increments,
        "value += 1;\n".repeat(increments)
    )
}

#[test]
fn function_line_limit_accepts_41_and_blocks_42_on_commit() -> Result<()> {
    for (body_lines, accepted) in [(41, true), (42, false)] {
        let directory = fixture(42)?;
        let root = directory.path();
        fs::write(root.join("src/lib.rs"), limited_function_source(body_lines))?;
        successful_git(root, &["add", "--all"])?;
        let output = if accepted {
            successful_git(root, &["commit", "-m", "function length boundary"])?
        } else {
            blocked_commit(root, "function length boundary")?
        };
        assert_eq!(
            output.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !accepted {
            assert!(String::from_utf8_lossy(&output.stderr).contains("too_many_lines"));
        }
    }
    Ok(())
}

fn assert_comment_rejected(output: &Output) {
    let transcript = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(transcript.contains("注释"), "{transcript}");
    assert!(transcript.contains("提交已阻止"), "{transcript}");
    assert!(!transcript.contains("正在运行 Clippy"), "{transcript}");
}

#[test]
fn ordinary_comments_block_commits_even_when_tests_and_clippy_pass() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    prepend_source(root, "// A new ordinary comment.\n")?;
    successful_git(root, &["add", "--all"])?;
    successful_cargo(root, &["test", "--locked", "--offline"])?;
    successful_cargo(
        root,
        &[
            "clippy",
            "--all-targets",
            "--locked",
            "--offline",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    let output = blocked_commit(root, "ordinary comment is forbidden")?;
    assert_comment_rejected(&output);
    Ok(())
}

#[test]
fn baseline_comments_survive_code_changes() -> Result<()> {
    let directory = fixture_with_comments(42, "// Existing line.\n/* Existing block. */\n")?;
    let root = directory.path();
    let path = root.join("src/lib.rs");
    let original = fs::read_to_string(&path)?;
    fs::write(
        path,
        format!("{original}\npub fn next_value() -> u8 {{ value() + 1 }}\n"),
    )?;
    successful_git(root, &["add", "--all"])?;
    successful_git(
        root,
        &["commit", "-m", "keep baseline comments and change code"],
    )?;
    let committed = successful_git(root, &["show", "HEAD:src/lib.rs"])?;
    let committed = String::from_utf8(committed.stdout)?;
    assert!(committed.contains("// Existing line."));
    assert!(committed.contains("/* Existing block. */"));
    assert!(committed.contains("pub fn next_value()"));
    Ok(())
}

#[test]
fn documentation_and_comment_symbols_inside_strings_allow_a_commit() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    let path = root.join("src/lib.rs");
    let original = fs::read_to_string(&path)?;
    fs::write(
        path,
        format!(
            "//! Crate documentation.\n/// Value documentation.\n{original}\n{COMMENT_SYMBOLS_SOURCE}"
        ),
    )?;
    successful_git(root, &["add", "--all"])?;
    successful_git(
        root,
        &["commit", "-m", "documentation and strings are allowed"],
    )?;
    Ok(())
}

#[test]
fn unstaged_comment_removal_cannot_hide_a_staged_ordinary_comment() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    let path = root.join("src/lib.rs");
    let original = fs::read(&path)?;
    prepend_source(root, "// Staged ordinary comment.\n")?;
    successful_git(root, &["add", "--all"])?;
    fs::write(&path, &original)?;
    let output = blocked_commit(root, "unstaged removal cannot hide staged comment")?;
    assert_comment_rejected(&output);
    assert_eq!(original, fs::read(path)?);
    Ok(())
}

#[test]
fn unstaged_ordinary_comments_do_not_reject_a_clean_staged_snapshot() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    prepend_source(root, "// This comment is not staged.\n")?;
    let before = fs::read(root.join("src/lib.rs"))?;
    successful_git(
        root,
        &["commit", "-m", "only the staged snapshot is checked"],
    )?;
    assert_eq!(before, fs::read(root.join("src/lib.rs"))?);
    let committed = successful_git(root, &["show", "HEAD:src/lib.rs"])?;
    assert!(!String::from_utf8_lossy(&committed.stdout).contains("This comment is not staged"));
    Ok(())
}

#[test]
fn duplicating_a_baseline_comment_is_rejected() -> Result<()> {
    let directory = fixture_with_comments(42, "// Existing comment.\n")?;
    let root = directory.path();
    prepend_source(root, "// Existing comment.\n")?;
    successful_git(root, &["add", "--all"])?;
    let output = blocked_commit(root, "baseline comments have a fixed multiplicity")?;
    assert_comment_rejected(&output);
    Ok(())
}

#[test]
fn explicit_pre_commit_mode_ignores_unstaged_rust_and_manifest_failures() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    fs::write(root.join("src/lib.rs"), "invalid Rust source")?;
    fs::write(root.join("Cargo.toml"), "invalid TOML manifest")?;
    let output = Command::new("bash")
        .arg(root.join("scripts/check"))
        .arg("--pre-commit")
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("暂存区版本 v0.1.2"));
    Ok(())
}

fn transcript(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn staged_totals(root: &Path) -> Result<(usize, usize)> {
    let output = successful_git(root, &["diff", "--cached", "--numstat"])?;
    let text = String::from_utf8(output.stdout)?;
    let mut lines = 0;
    for record in text.lines() {
        for count in record.split('\t').take(2) {
            lines += count.parse::<usize>()?;
        }
    }
    Ok((text.lines().count(), lines))
}

fn boundary_payload(root: &Path, target_lines: usize, target_files: usize) -> Result<()> {
    let (files, lines) = staged_totals(root)?;
    ensure!(
        files < target_files && lines < target_lines,
        "Fixture already exceeds boundary"
    );
    fs::write(root.join("payload"), "text\n".repeat(target_lines - lines))?;
    for number in files + 1..target_files {
        fs::write(root.join(format!("payload-{number}")), b"")?;
    }
    successful_git(root, &["add", "--all"])?;
    assert_eq!(staged_totals(root)?, (target_files, target_lines));
    Ok(())
}

#[test]
fn hooks_accept_exact_limits_and_block_either_excess_before_clippy() -> Result<()> {
    for (lines, files, accepted) in [(400, 13, true), (401, 13, false), (400, 14, false)] {
        let directory = fixture(42)?;
        let root = directory.path();
        boundary_payload(root, lines, files)?;
        let output = if accepted {
            successful_git(root, &["commit", "-m", "exact limits"])?
        } else {
            blocked_commit(root, "excessive candidate")?
        };
        let text = transcript(&output);
        assert!(text.contains(&format!("{files} 个文件")));
        assert!(text.contains(&format!("总变更 {lines} 行")));
        if !accepted {
            assert!(text.contains("拆分提交"));
            assert!(!text.contains("正在运行 Clippy"));
        }
    }
    Ok(())
}

#[test]
fn hooks_report_binary_files_and_include_them_in_the_file_count() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    let (files, _) = staged_totals(root)?;
    fs::write(root.join("binary.data"), b"binary\0payload")?;
    successful_git(root, &["add", "--all"])?;
    let output = successful_git(root, &["commit", "-m", "visible binary"])?;
    let text = transcript(&output);
    assert!(text.contains("二进制文件") && text.contains("binary.data"));
    assert!(text.contains("无法按文本行数衡量"));
    assert!(text.contains(&format!("{} 个文件", files + 1)));
    Ok(())
}

#[test]
fn hooks_reject_an_unstaged_version_preparation() -> Result<()> {
    let directory = fixture(42)?;
    let root = directory.path();
    successful_git(
        root,
        &["checkout", "HEAD", "--", "Cargo.toml", "Cargo.lock"],
    )?;
    successful_cargo(
        root,
        &[
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "--example",
            "project_check",
            "--",
            "--prepare-commit",
        ],
    )?;
    let text = transcript(&blocked_commit(root, "version not staged")?);
    assert!(text.contains("暂存区版本") && text.contains("v0.1.2"));
    assert!(!text.contains("正在运行 Clippy"));
    Ok(())
}

#[test]
fn hooks_use_the_actual_alternative_index_in_both_directions() -> Result<()> {
    for excessive_candidate in [true, false] {
        let directory = fixture(42)?;
        let root = directory.path();
        let default_index = root.join(".git/index");
        let candidate = root.join(".git/candidate-index");
        fs::copy(&default_index, &candidate)?;
        fs::write(root.join("payload"), "line\n".repeat(401))?;
        let excessive = if excessive_candidate {
            &candidate
        } else {
            &default_index
        };
        assert!(
            git_index(root, &["add", "payload"], excessive)?
                .status
                .success()
        );
        let before = successful_git(root, &["rev-parse", "HEAD"])?;
        let output = git_index(root, &["commit", "-m", "alternative index"], &candidate)?;
        assert_eq!(
            output.status.success(),
            !excessive_candidate,
            "{}",
            transcript(&output)
        );
        if excessive_candidate {
            assert!(transcript(&output).contains("总变更行数超过 400"));
            assert_eq!(
                before.stdout,
                successful_git(root, &["rev-parse", "HEAD"])?.stdout
            );
        } else {
            assert!(
                !git(root, &["cat-file", "-e", "HEAD:payload"])?
                    .status
                    .success()
            );
        }
    }
    Ok(())
}
