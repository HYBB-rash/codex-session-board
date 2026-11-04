use super::*;
use tempfile::{TempDir, tempdir};

fn run_git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Comment Policy Test",
            "-c",
            "user.email=comment-policy@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()?;
    ensure!(
        output.status.success(),
        "测试 Git 操作失败 {:?}：{}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn fixture(files: &[(&str, &str)]) -> Result<(TempDir, String)> {
    let directory = tempdir()?;
    let root = directory.path();
    run_git(
        root,
        &["init", "--quiet", "--initial-branch=main", "--template="],
    )?;
    for (path, source) in files {
        write_source(root, path, source)?;
    }
    run_git(root, &["add", "--all"])?;
    let baseline = commit(root)?;
    Ok((directory, baseline))
}

fn commit(root: &Path) -> Result<String> {
    run_git(
        root,
        &["commit", "--quiet", "--allow-empty", "-m", "fixture"],
    )?;
    Ok(String::from_utf8(run_git(root, &["rev-parse", "HEAD"])?)?
        .trim()
        .to_string())
}

fn write_source(root: &Path, path: &str, source: &str) -> Result<()> {
    let target = root.join(path);
    fs::create_dir_all(target.parent().context("测试路径缺少父目录")?)?;
    fs::write(target, source)?;
    Ok(())
}

#[test]
fn literals_lifetimes_and_character_delimiters_are_not_comments() -> Result<()> {
    let source = r####"
        let normal = "https://host/* literal */ // string";
        let escaped = "\" // string \\\\";
        let raw = r###"/* raw */ "//" "###;
        let bytes = b"// bytes /* text */";
        let raw_bytes = br##"/* bytes */ //"##;
        let c_string = c"/* C string */ //";
        let raw_c_string = cr##"// raw C string /* */"##;
        let slash = '/';
        let quote = '\'';
        let byte = b'/';
        fn borrow<'value>(value: &'value str) -> &'value str { value }
    "####;
    assert!(comments(source)?.is_empty());
    Ok(())
}

#[test]
fn documentation_and_ordinary_comment_boundaries_follow_rust_tokens() -> Result<()> {
    let source = "/// outer\n//! inner\n/** outer */\n/*! inner */\n//// ordinary\n/**/\n/*** ordinary */\n// ordinary\n";
    let found = comments(source)?;
    assert_eq!(
        found
            .iter()
            .map(|comment| comment.text.as_str())
            .collect::<Vec<_>>(),
        ["//// ordinary", "/**/", "/*** ordinary */", "// ordinary"]
    );
    assert_eq!(
        found.iter().map(|comment| comment.line).collect::<Vec<_>>(),
        [5, 6, 7, 8]
    );
    Ok(())
}

#[test]
fn raw_identifiers_and_raw_lifetimes_are_not_lexical_errors() -> Result<()> {
    assert!(comments("let r#invalid = 1; let r#type = r#invalid;")?.is_empty());
    assert!(comments("fn borrow<'r#value>(value: &'r#value str) {}")?.is_empty());
    Ok(())
}

#[test]
fn nested_blocks_are_single_comments_and_doc_blocks_stay_documentation() -> Result<()> {
    let source = "/* 外层\n/* 内层 */\n尾部 */\n/** 文档 /* 内层 */ */\n// 末尾";
    let found = comments(source)?;
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].text, "/* 外层\n/* 内层 */\n尾部 */");
    assert_eq!(found[0].line, 1);
    assert_eq!(found[1].line, 5);
    Ok(())
}

#[test]
fn bom_shebang_crlf_and_unicode_preserve_comment_lines() -> Result<()> {
    let source =
        "\u{feff}#!/usr/bin/env rust-script // shebang\r\nlet 名称 = \"🦀\";\r\n// 旧注释\r\n";
    let found = comments(source)?;
    assert_eq!(
        found,
        [Comment {
            text: "// 旧注释\r".to_string(),
            line: 3
        }]
    );
    let attributes = "#! /* ordinary */ [allow(dead_code)]\nfn main() {}";
    assert_eq!(comments(attributes)?.len(), 1);
    Ok(())
}

#[test]
fn lexer_errors_do_not_hide_unclosed_comments_or_literals() {
    let invalid = [
        "/* unfinished",
        "/** unfinished",
        "/* outer /* inner */",
        "\"unfinished",
        "b\"unfinished",
        "c\"unfinished",
        "'\\",
        "b'\\",
        "r###\"unfinished\"##",
        "br###\"unfinished\"##",
        "cr###\"unfinished\"##",
        "r##invalid",
        "0x",
        "1e",
        "'123",
        "§",
    ];
    for source in invalid {
        assert!(comments(source).is_err(), "错误词法输入被放行：{source:?}");
    }
}

#[test]
fn matching_baseline_comments_can_move_or_be_deleted() -> Result<()> {
    let (directory, baseline) =
        fixture(&[("src/main.rs", "// old\n/* retained */\nfn main() {}\n")])?;
    let root = directory.path();
    write_source(
        root,
        "src/main.rs",
        "\n\nfn main() {}\n// old\n/* retained */\n",
    )?;
    assert!(check(root, root, &baseline)?);
    write_source(root, "src/main.rs", "fn main() {}\n")?;
    assert!(check(root, root, &baseline)?);
    fs::remove_file(root.join("src/main.rs"))?;
    assert!(check(root, root, &baseline)?);
    Ok(())
}

#[test]
fn added_modified_and_duplicated_comments_fail_exact_multiset_comparison() -> Result<()> {
    let (directory, baseline) = fixture(&[("main.rs", "// old\nfn main() {}\n")])?;
    let root = directory.path();
    for source in [
        "// old\n// new\nfn main() {}\n",
        "// modified\nfn main() {}\n",
        "// old \nfn main() {}\n",
        "// old\n// old\nfn main() {}\n",
    ] {
        write_source(root, "main.rs", source)?;
        assert!(
            !check(root, root, &baseline)?,
            "新增普通注释被放行：{source:?}"
        );
    }
    Ok(())
}

#[test]
fn duplicate_baseline_counts_allow_only_the_recorded_number() -> Result<()> {
    let (directory, baseline) = fixture(&[("main.rs", "// same\n// same\nfn main() {}\n")])?;
    let root = directory.path();
    write_source(root, "main.rs", "\n// same\nfn main() {}\n")?;
    assert!(check(root, root, &baseline)?);
    write_source(root, "main.rs", "// same\n// same\n// same\n")?;
    assert!(!check(root, root, &baseline)?);
    Ok(())
}

#[test]
fn later_commits_do_not_reset_the_fixed_comment_baseline() -> Result<()> {
    let (directory, baseline) = fixture(&[("main.rs", "fn main() {}\n")])?;
    let root = directory.path();
    write_source(root, "main.rs", "// newly committed\nfn main() {}\n")?;
    run_git(root, &["add", "--all"])?;
    let latest = commit(root)?;
    assert!(!check(root, root, &baseline)?);
    assert!(check(root, root, &latest)?);
    Ok(())
}

#[test]
fn copying_to_another_file_does_not_inherit_its_comment_allowance() -> Result<()> {
    let (directory, baseline) = fixture(&[("original.rs", "// old\n")])?;
    let root = directory.path();
    write_source(root, "copy.rs", "// old\n")?;
    assert!(!check(root, root, &baseline)?);
    fs::remove_file(root.join("original.rs"))?;
    assert!(!check(root, root, &baseline)?);
    Ok(())
}

#[test]
fn untracked_and_staged_new_files_are_checked_but_ignored_files_are_not() -> Result<()> {
    let (directory, baseline) = fixture(&[
        ("main.rs", "fn main() {}\n"),
        (".gitignore", "ignored.rs\n"),
    ])?;
    let root = directory.path();
    write_source(root, "ignored.rs", "// ignored\n")?;
    write_source(root, "notes.txt", "// not Rust\n")?;
    assert!(check(root, root, &baseline)?);
    write_source(root, "新 文件\nline.rs", "// new untracked\n")?;
    assert!(!check(root, root, &baseline)?);
    run_git(root, &["add", "--all"])?;
    assert!(!check(root, root, &baseline)?);
    Ok(())
}

#[test]
fn new_documentation_and_comment_like_strings_are_allowed() -> Result<()> {
    let (directory, baseline) = fixture(&[("main.rs", "fn main() {}\n")])?;
    let root = directory.path();
    write_source(
        root,
        "docs.rs",
        "//! docs\n/** docs */\nconst URL: &str = \"https://host/*x*/\";\n",
    )?;
    assert!(check(root, root, &baseline)?);
    Ok(())
}

#[test]
fn baseline_must_be_a_full_existing_commit_and_an_ancestor() -> Result<()> {
    let (directory, baseline) = fixture(&[("main.rs", "fn main() {}\n")])?;
    let root = directory.path();
    for invalid in ["HEAD", &baseline[..8], &"0".repeat(40), &"g".repeat(40)] {
        assert!(check(root, root, invalid).is_err());
    }
    let blob = String::from_utf8(run_git(root, &["rev-parse", "HEAD:main.rs"])?)?;
    assert!(check(root, root, blob.trim()).is_err());
    let future = commit(root)?;
    run_git(root, &["reset", "--hard", &baseline])?;
    assert!(check(root, root, &future).is_err());
    let descendant = commit(root)?;
    assert!(check(root, root, &baseline)?);
    assert_ne!(descendant, baseline);
    Ok(())
}

#[test]
fn snapshot_root_uses_repository_inventory_and_snapshot_contents() -> Result<()> {
    let (directory, baseline) = fixture(&[("src/main.rs", "fn main() {}\n")])?;
    let repository = directory.path();
    let snapshot = tempdir()?;
    write_source(repository, "src/main.rs", "// working tree only\n")?;
    write_source(repository, "untracked.rs", "// absent from snapshot\n")?;
    write_source(snapshot.path(), "src/main.rs", "fn main() {}\n")?;
    write_source(
        snapshot.path(),
        "outside_inventory.rs",
        "// not listed in repository\n",
    )?;
    assert!(check(snapshot.path(), repository, &baseline)?);
    write_source(
        snapshot.path(),
        "src/main.rs",
        "// staged snapshot comment\n",
    )?;
    assert!(!check(snapshot.path(), repository, &baseline)?);
    Ok(())
}

#[test]
fn lexical_and_read_errors_are_errors_instead_of_policy_successes() -> Result<()> {
    let (directory, baseline) = fixture(&[("main.rs", "fn main() {}\n")])?;
    let root = directory.path();
    write_source(root, "main.rs", "/* unfinished")?;
    let error = check(root, root, &baseline).expect_err("词法错误必须阻止检查");
    let diagnostic = format!("{error:#}");
    assert!(diagnostic.contains("main.rs") && diagnostic.contains("第 1 行"));
    fs::write(root.join("main.rs"), [0xff])?;
    assert!(check(root, root, &baseline).is_err());
    fs::remove_file(root.join("main.rs"))?;
    fs::create_dir(root.join("main.rs"))?;
    assert!(check(root, root, &baseline).is_err());
    assert!(check(&root.join("missing-root"), root, &baseline).is_err());
    Ok(())
}

#[test]
fn malformed_baseline_source_cannot_be_silently_grandfathered() -> Result<()> {
    let (directory, baseline) = fixture(&[("bad.rs", "/** unfinished")])?;
    write_source(directory.path(), "bad.rs", "fn repaired() {}")?;
    assert!(check(directory.path(), directory.path(), &baseline).is_err());
    Ok(())
}
