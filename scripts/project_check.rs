use anyhow::{Context, Result, ensure};
use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::ops::Range;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod comment_policy;
mod commit_size;
mod document_policy;
mod staged_check;

const RULES_FILE: &str = "project-rules.json";

#[derive(Parser)]
#[command(
    about = "检查项目约束；默认只读，不暂存、不提交、不推送",
    group(clap::ArgGroup::new("snapshot_mode").args(["comments_only", "pre_commit"]))
)]
struct Cli {
    #[arg(
        long,
        conflicts_with_all = ["prepare_commit", "feature_complete"],
        help = "仅检查 Rust 普通注释增量，不检查工作区和版本"
    )]
    comments_only: bool,
    #[arg(
        long,
        conflicts_with_all = ["prepare_commit", "feature_complete"],
        help = "检查本次实际暂存内容、提交规模和版本，不要求工作区干净"
    )]
    pre_commit: bool,
    #[arg(
        long,
        requires = "snapshot_mode",
        help = "从指定 Git 仓库读取既有注释基线，检查当前目录中的暂存快照"
    )]
    baseline_repository: Option<PathBuf>,
    #[arg(long, help = "准备下一次提交的版本，同步 Cargo.toml 和 Cargo.lock")]
    prepare_commit: bool,
    #[arg(
        long,
        requires = "prepare_commit",
        value_name = "用户明确决定的说明",
        help = "仅在用户明确宣布阶段功能完成后使用，将次版本加一并将补丁号清零"
    )]
    feature_complete: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    fn parse(text: &str) -> Result<Self> {
        let parts = text.split('.').collect::<Vec<_>>();
        ensure!(parts.len() == 3, "版本必须是三段整数，例如 0.1.1：{text}");
        let version = Self {
            major: parts[0].parse()?,
            minor: parts[1].parse()?,
            patch: parts[2].parse()?,
        };
        ensure!(version.to_string() == text, "版本格式不规范：{text}");
        Ok(version)
    }

    fn next_patch(self) -> Result<Self> {
        Ok(Self {
            patch: self.patch.checked_add(1).context("补丁版本溢出")?,
            ..self
        })
    }

    fn next_minor(self) -> Result<Self> {
        Ok(Self {
            minor: self.minor.checked_add(1).context("次版本溢出")?,
            patch: 0,
            ..self
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    schema_version: u64,
    versioning: VersionRules,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rust_comments: Option<CommentRules>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CommentRules {
    baseline_commit: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VersionRules {
    baseline: String,
    feature_releases: Vec<FeatureRelease>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FeatureRelease {
    after_commit: String,
    from: String,
    to: String,
    decision: String,
}

impl Rules {
    fn parse(text: &str) -> Result<Self> {
        let rules: Self = serde_json::from_str(text).context("约束配置不是有效 JSON")?;
        ensure!(rules.schema_version == 1, "不支持的约束配置版本");
        Version::parse(&rules.versioning.baseline)?;
        Ok(rules)
    }

    fn approves(&self, parent: &str, from: Version, to: Version) -> bool {
        self.versioning.feature_releases.iter().any(|release| {
            release.after_commit == parent
                && release.from == from.to_string()
                && release.to == to.to_string()
                && !release.decision.trim().is_empty()
        })
    }
}

struct History {
    tip: Option<String>,
    version: Version,
    count: usize,
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<Output> {
    Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .context("无法运行 Git")
}

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = git_output(root, arguments)?;
    ensure!(
        output.status.success(),
        "Git 命令失败：{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("Git 返回了非 UTF-8 文本")
}

fn root_directory() -> Result<PathBuf> {
    let output = git(&std::env::current_dir()?, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim_end_matches('\n')))
}

fn package(text: &str) -> Result<(String, Version)> {
    let document = text.parse::<toml::Table>().context("Cargo.toml 无法解析")?;
    let package = document.get("package").context("缺少 [package]")?;
    let name = package
        .get("name")
        .and_then(toml::Value::as_str)
        .context("缺少包名")?;
    let version = package
        .get("version")
        .and_then(toml::Value::as_str)
        .context("须在 [package] 直接声明版本字符串")?;
    Ok((name.to_string(), Version::parse(version)?))
}

fn locked_version(text: &str, name: &str) -> Result<Version> {
    let document = text.parse::<toml::Table>().context("Cargo.lock 无法解析")?;
    let packages = document
        .get("package")
        .and_then(toml::Value::as_array)
        .context("锁文件缺少包列表")?;
    let matches = packages
        .iter()
        .filter(|package| {
            package.get("name").and_then(toml::Value::as_str) == Some(name)
                && package.get("source").is_none()
        })
        .collect::<Vec<_>>();
    ensure!(matches.len() == 1, "锁文件中应有且仅有一个本地包 {name}");
    Version::parse(
        matches[0]
            .get("version")
            .and_then(toml::Value::as_str)
            .context("锁文件缺少版本")?,
    )
}

fn committed(root: &Path, commit: &str, filename: &str) -> Result<String> {
    git(root, &["show", &format!("{commit}:{filename}")])
}

fn validate_history(root: &Path, rules: &Rules) -> Result<History> {
    ensure!(
        git(root, &["rev-parse", "--is-shallow-repository"])?.trim() == "false",
        "浅克隆缺少完整版本历史，请先补全 Git 历史"
    );
    let head = git_output(root, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    let mut history = History {
        tip: None,
        version: Version::parse(&rules.versioning.baseline)?,
        count: 0,
    };
    if !head.status.success() {
        ensure!(head.status.code() == Some(1), "无法读取当前 HEAD");
        return Ok(history);
    }
    let baseline = history.version;
    let commits = git(
        root,
        &["rev-list", "--topo-order", "--reverse", "--parents", "HEAD"],
    )?;
    let mut versions = HashMap::new();
    for entry in commits.lines() {
        let mut fields = entry.split_whitespace();
        let commit = fields.next().context("提交记录缺少提交号")?;
        let parent = fields.next();
        let previous = match parent {
            Some(parent) => *versions.get(parent).context("缺少父提交的版本")?,
            None => baseline,
        };
        let version = validate_commit_version(root, rules, commit, parent, previous)?;
        versions.insert(commit, version);
        history.count += 1;
    }
    let tip = String::from_utf8(head.stdout)?.trim().to_string();
    history.version = *versions.get(tip.as_str()).context("缺少 HEAD 的版本")?;
    history.tip = Some(tip);
    Ok(history)
}

fn validate_commit_version(
    root: &Path,
    rules: &Rules,
    commit: &str,
    parent: Option<&str>,
    previous: Version,
) -> Result<Version> {
    let (name, version) = package(&committed(root, commit, "Cargo.toml")?)?;
    ensure!(
        locked_version(&committed(root, commit, "Cargo.lock")?, &name)? == version,
        "提交 {commit} 中 Cargo.toml 与 Cargo.lock 的版本不一致"
    );
    let recorded = Rules::parse(&committed(root, commit, RULES_FILE)?)?;
    ensure!(
        recorded.versioning.baseline == rules.versioning.baseline,
        "不能修改既有版本基线来绕过历史检查"
    );
    let patch = previous.next_patch()?;
    let minor = previous.next_minor()?;
    let approved = parent
        .is_some_and(|parent| version == minor && recorded.approves(parent, previous, version));
    ensure!(
        version == patch || approved,
        "提交 {commit} 的版本 v{version} 不符合规范：普通提交须为 v{patch}；阶段完成须有用户决定记录并恰好升到 v{minor}"
    );
    Ok(version)
}

fn status(root: &Path) -> Result<Vec<u8>> {
    let output = git_output(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    ensure!(output.status.success(), "无法检查工作区状态");
    Ok(output.stdout)
}

fn check_hooks(root: &Path) -> bool {
    let result = (|| -> Result<()> {
        let configured =
            git(root, &["config", "--get", "core.hooksPath"]).context("尚未配置提交前测试钩子")?;
        ensure!(
            configured.trim() == ".githooks",
            "当前 hooksPath 不是项目的 .githooks：{}",
            configured.trim()
        );
        let hook = root.join(".githooks/pre-commit");
        ensure!(hook.is_file(), "缺少 .githooks/pre-commit");
        ensure!(
            fs::metadata(hook)?.permissions().mode() & 0o111 != 0,
            "提交前测试钩子没有执行权限"
        );
        Ok(())
    })();
    match result {
        Ok(()) => {
            println!("[通过] 已启用提交前测试钩子，测试通过后才允许提交。");
            true
        }
        Err(error) => {
            println!("[未通过] 提交前测试钩子：{error:#}");
            println!("  运行 ./scripts/install-git-hooks 启用；已有其他钩子时按安装提示处理。");
            false
        }
    }
}

fn check_rust_lints(root: &Path) -> bool {
    let result = (|| -> Result<()> {
        let manifest = fs::read_to_string(root.join("Cargo.toml"))?.parse::<toml::Table>()?;
        let lint = manifest
            .get("lints")
            .and_then(|lints| lints.get("rust"))
            .and_then(|rust| rust.get("unsafe_code"));
        let level = lint.and_then(|lint| {
            lint.as_str()
                .or_else(|| lint.get("level").and_then(toml::Value::as_str))
        });
        ensure!(
            level == Some("forbid"),
            "Cargo.toml 必须设置 [lints.rust] unsafe_code = \"forbid\""
        );
        Ok(())
    })();
    match result {
        Ok(()) => {
            println!("[通过] 已配置 unsafe_code = \"forbid\"，编译器禁止项目自身使用 unsafe。");
            true
        }
        Err(error) => {
            println!("[未通过] Rust 安全约束：{error:#}");
            false
        }
    }
}

fn check(root: &Path, rules: &Rules) -> Result<bool> {
    let mut passed = check_worktree(root)?;
    if let Err(error) = validate_current_version(root, rules) {
        passed = false;
        println!("[未通过] 版本规则：{error:#}");
    }
    if !passed {
        print_commit_guidance();
    }
    Ok(passed)
}

fn check_worktree(root: &Path) -> Result<bool> {
    let changes = status(root)?;
    let passed = changes.is_empty();
    if passed {
        println!("[通过] 工作区干净，没有暂存、未暂存或未跟踪的变更。");
    } else {
        println!("[未通过] 工作区存在未提交的变更：");
        for entry in changes
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            println!("  {:?}", String::from_utf8_lossy(entry));
        }
        println!("  保留有用改动，检查差异后提交；不要通过删除文件或扩大忽略规则掩盖问题。");
    }
    Ok(passed)
}

fn validate_current_version(root: &Path, rules: &Rules) -> Result<()> {
    let history = validate_history(root, rules)?;
    let (name, current) = package(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    ensure!(
        locked_version(&fs::read_to_string(root.join("Cargo.lock"))?, &name)? == current,
        "Cargo.toml 与 Cargo.lock 的当前版本不一致"
    );
    ensure!(
        history.tip.is_some(),
        "仓库尚无提交；已发布基线为 v{}，首次提交必须为 v{}",
        history.version,
        history.version.next_patch()?
    );
    ensure!(
        current == history.version,
        "当前文件版本 v{current} 尚未形成合规提交，HEAD 为 v{}",
        history.version
    );
    println!(
        "[通过] {} 次提交的版本步进正确，当前 v{current}。",
        history.count
    );
    Ok(())
}

fn print_commit_guidance() {
    println!("\n处理顺序：");
    println!("  1. 运行 ./scripts/check --prepare-commit，准备本次提交的补丁版本。");
    println!(
        "  2. 用 git status --short 和 git diff 检查改动，仅暂存应提交文件，再用 git diff --cached 审阅。"
    );
    println!(
        "  3. 运行 ./scripts/check --pre-commit；通过并获得提交授权后才提交，优先使用 agent-git commit。"
    );
    println!("  4. 提交后重新运行 ./scripts/check，直到全部检查通过。");
    println!(
        "仅在用户明确宣布阶段完成时，使用 --prepare-commit --feature-complete '用户决定的说明'。"
    );
    println!("检查不会自动提交、推送、删除文件、修改忽略规则或升级正在运行的服务。");
}

fn version_span(text: &str, header: &str, name: Option<&str>) -> Result<Range<usize>> {
    let headers = Regex::new(r"(?m)^\[[^\n]+\][ \t]*(?:#[^\n]*)?$")?;
    let versions = Regex::new(
        r#"(?m)^[ \t]*version[ \t]*=[ \t]*["'](?P<version>[0-9]+\.[0-9]+\.[0-9]+)["'][ \t]*(?:#[^\n]*)?$"#,
    )?;
    let sections = headers.find_iter(text).collect::<Vec<_>>();
    let mut found = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if section.as_str().split('#').next().unwrap_or("").trim() != header {
            continue;
        }
        let start = section.end();
        let end = sections
            .get(index + 1)
            .map_or(text.len(), |next| next.start());
        let body = &text[start..end];
        if let Some(name) = name {
            let section = body.parse::<toml::Table>()?;
            if section.get("name").and_then(toml::Value::as_str) != Some(name)
                || section.contains_key("source")
            {
                continue;
            }
        }
        let capture = versions
            .captures(body)
            .context("版本须以单行 version = \"0.1.1\" 形式声明")?;
        let value = capture.name("version").context("版本正则缺少命名捕获")?;
        found.push(start + value.start()..start + value.end());
    }
    ensure!(found.len() == 1, "找不到唯一的本地包版本字段");
    Ok(found.remove(0))
}

fn prepare(root: &Path, rules: &mut Rules, decision: Option<&str>) -> Result<Version> {
    let history = validate_history(root, rules)?;
    let manifest_path = root.join("Cargo.toml");
    let lock_path = root.join("Cargo.lock");
    let mut manifest = fs::read_to_string(&manifest_path)?;
    let mut lockfile = fs::read_to_string(&lock_path)?;
    let (name, current) = package(&manifest)?;
    let target = prepare_target(&history, rules, current, decision)?;
    let manifest_span = version_span(&manifest, "[package]", None)?;
    let lock_span = version_span(&lockfile, "[[package]]", Some(&name))?;
    manifest.replace_range(manifest_span, &target.to_string());
    lockfile.replace_range(lock_span, &target.to_string());
    if decision.is_some() {
        fs::write(
            root.join(RULES_FILE),
            format!("{}\n", serde_json::to_string_pretty(rules)?),
        )?;
    }
    fs::write(manifest_path, manifest)?;
    fs::write(lock_path, lockfile)?;
    println!("已准备 v{target}，并同步 Cargo.toml 与 Cargo.lock；重复准备不会重复加号。");
    println!("尚未暂存或提交。检查差异并提交后，重新运行 ./scripts/check。");
    Ok(target)
}

fn prepare_target(
    history: &History,
    rules: &mut Rules,
    current: Version,
    decision: Option<&str>,
) -> Result<Version> {
    let patch = history.version.next_patch()?;
    let minor = history.version.next_minor()?;
    let target = if let Some(decision) = decision {
        approve_feature_release(history, rules, current, decision)?
    } else if current == minor
        && history
            .tip
            .as_deref()
            .is_some_and(|parent| rules.approves(parent, history.version, minor))
    {
        minor
    } else {
        ensure!(
            current == history.version || current == patch,
            "普通提交只能从 v{} 步进到 v{patch}；不能自动认可 v{current}",
            history.version
        );
        patch
    };
    Ok(target)
}

fn approve_feature_release(
    history: &History,
    rules: &mut Rules,
    current: Version,
    decision: &str,
) -> Result<Version> {
    let patch = history.version.next_patch()?;
    let minor = history.version.next_minor()?;
    ensure!(!decision.trim().is_empty(), "用户决定的说明不能为空");
    let parent = history
        .tip
        .as_deref()
        .context("须先有普通提交，才能结束阶段")?;
    ensure!(
        [history.version, patch, minor].contains(&current),
        "当前版本 v{current} 不是本次允许准备的版本"
    );
    if !rules.approves(parent, history.version, minor) {
        rules.versioning.feature_releases.push(FeatureRelease {
            after_commit: parent.to_string(),
            from: history.version.to_string(),
            to: minor.to_string(),
            decision: decision.to_string(),
        });
    }
    Ok(minor)
}

fn check_comments(root: &Path, repository: &Path, rules: &Rules) -> Result<bool> {
    let policy = rules
        .rust_comments
        .as_ref()
        .context("缺少 rust_comments.baseline_commit；请核对既有注释基线配置")?;
    comment_policy::check(root, repository, &policy.baseline_commit)
}

fn run(cli: Cli) -> Result<bool> {
    if cli.pre_commit {
        return staged_check::check(cli.baseline_repository.as_deref());
    }
    let root = if cli.comments_only {
        std::env::current_dir()?
    } else {
        root_directory()?
    };
    let mut rules = Rules::parse(&fs::read_to_string(root.join(RULES_FILE))?)?;
    if cli.prepare_commit {
        prepare(&root, &mut rules, cli.feature_complete.as_deref())?;
        return Ok(true);
    }
    let repository = cli.baseline_repository.as_deref().unwrap_or(&root);
    let comments_passed = check_comments(&root, repository, &rules)?;
    if cli.comments_only {
        return Ok(comments_passed);
    }
    let documents_passed = document_policy::check(&root)?;
    let hooks_passed = check_hooks(&root);
    let rust_lints_passed = check_rust_lints(&root);
    Ok(check(&root, &rules)?
        && hooks_passed
        && rust_lints_passed
        && comments_passed
        && documents_passed)
}

fn main() {
    match run(Cli::parse()) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("[检查错误] {error:#}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture_git(root: &Path, arguments: &[&str]) -> Result<()> {
        let output = Command::new("git")
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args([
                "-c",
                "user.name=Project Check Test",
                "-c",
                "user.email=project-check@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn fixture() -> Result<(TempDir, Rules)> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        fixture_git(root, &["init", "--initial-branch=main"])?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
        )?;
        fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"sample\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"other\"\nversion = \"9.8.7\"\nsource = \"registry+example\"\n",
        )?;
        let rules = Rules::parse(
            r#"{"schema_version":1,"versioning":{"baseline":"0.1.0","feature_releases":[]}}"#,
        )?;
        fs::write(root.join(RULES_FILE), serde_json::to_vec_pretty(&rules)?)?;
        fs::write(root.join(".gitignore"), "/target/\n/data/\n")?;
        Ok((directory, rules))
    }

    fn commit(root: &Path) -> Result<()> {
        fixture_git(root, &["add", "--all"])?;
        fixture_git(root, &["commit", "-m", "test fixture"])
    }

    fn rewrite_version(root: &Path, version: &str) -> Result<()> {
        for (filename, header, name) in [
            ("Cargo.toml", "[package]", None),
            ("Cargo.lock", "[[package]]", Some("sample")),
        ] {
            let path = root.join(filename);
            let mut text = fs::read_to_string(&path)?;
            let span = version_span(&text, header, name)?;
            text.replace_range(span, version);
            fs::write(path, text)?;
        }
        Ok(())
    }

    #[test]
    fn comment_mode_excludes_preparation_and_controls_repository_override() {
        assert!(Cli::try_parse_from(["check", "--comments-only"]).is_ok());
        assert!(Cli::try_parse_from(["check", "--comments-only", "--prepare-commit"]).is_err());
        assert!(Cli::try_parse_from(["check", "--baseline-repository", "/tmp"]).is_err());
        assert!(
            Cli::try_parse_from(["check", "--comments-only", "--baseline-repository", "/tmp"])
                .is_ok()
        );
    }

    #[test]
    fn comment_checks_require_an_explicit_baseline_configuration() -> Result<()> {
        let (directory, rules) = fixture()?;
        let error = check_comments(directory.path(), directory.path(), &rules).unwrap_err();
        assert!(error.to_string().contains("rust_comments.baseline_commit"));
        Ok(())
    }

    #[test]
    fn initial_commit_steps_from_the_existing_release_and_prepare_is_idempotent() -> Result<()> {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        assert!(!check(root, &rules)?);
        assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.1.1");
        let lockfile = fs::read_to_string(root.join("Cargo.lock"))?;
        assert!(lockfile.contains("version = \"9.8.7\""));
        assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.1.1");
        assert_eq!(lockfile, fs::read_to_string(root.join("Cargo.lock"))?);
        commit(root)?;
        assert!(check(root, &rules)?);
        Ok(())
    }

    #[test]
    fn docs_and_empty_commits_also_require_exactly_one_patch_step() -> Result<()> {
        for empty in [false, true] {
            let (directory, mut rules) = fixture()?;
            let root = directory.path();
            prepare(root, &mut rules, None)?;
            commit(root)?;
            if empty {
                fixture_git(root, &["commit", "--allow-empty", "-m", "empty fixture"])?;
            } else {
                fs::write(root.join("README.md"), "documentation")?;
                commit(root)?;
            }
            assert!(!check(root, &rules)?);
            assert!(prepare(root, &mut rules, None).is_err());
        }
        Ok(())
    }

    #[test]
    fn merged_branch_commits_are_checked_in_addition_to_the_first_parent() -> Result<()> {
        for invalid_branch in [false, true] {
            let (directory, mut rules) = fixture()?;
            let root = directory.path();
            prepare(root, &mut rules, None)?;
            commit(root)?;
            fixture_git(root, &["checkout", "-b", "feature"])?;
            fs::write(root.join("feature.txt"), "feature")?;
            if !invalid_branch {
                prepare(root, &mut rules, None)?;
            }
            commit(root)?;
            fixture_git(root, &["checkout", "main"])?;
            fixture_git(root, &["merge", "--no-ff", "--no-commit", "feature"])?;
            rewrite_version(root, "0.1.2")?;
            commit(root)?;
            assert_eq!(check(root, &rules)?, !invalid_branch);
        }
        Ok(())
    }

    #[test]
    fn checks_do_not_change_files_index_or_head() -> Result<()> {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        let head = git(root, &["rev-parse", "HEAD"])?;
        let index = fs::read(root.join(".git/index"))?;
        let manifest = fs::read(root.join("Cargo.toml"))?;
        assert!(check(root, &rules)?);
        fs::write(root.join("new.txt"), "retain this content")?;
        assert!(!check(root, &rules)?);
        assert_eq!(head, git(root, &["rev-parse", "HEAD"])?);
        assert_eq!(index, fs::read(root.join(".git/index"))?);
        assert_eq!(manifest, fs::read(root.join("Cargo.toml"))?);
        assert_eq!(
            fs::read_to_string(root.join("new.txt"))?,
            "retain this content"
        );
        Ok(())
    }

    #[test]
    fn malformed_package_metadata_returns_errors_without_panicking() {
        assert!(package("[package]\nversion='0.1.0'\n").is_err());
        assert!(package("[package]\nname='sample'\n").is_err());
        assert!(locked_version("version=4\n", "sample").is_err());
        assert!(locked_version("[[package]]\nversion='0.1.0'\n", "sample").is_err());
        assert!(locked_version("[[package]]\nname='sample'\n", "sample").is_err());
    }

    #[test]
    fn hook_configuration_and_executable_are_required() -> Result<()> {
        let (directory, _) = fixture()?;
        let root = directory.path();
        assert!(!check_hooks(root));
        fixture_git(root, &["config", "--local", "core.hooksPath", ".githooks"])?;
        assert!(!check_hooks(root));
        fs::create_dir(root.join(".githooks"))?;
        let hook = root.join(".githooks/pre-commit");
        fs::write(&hook, include_str!("../.githooks/pre-commit"))?;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o644))?;
        assert!(!check_hooks(root));
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        assert!(check_hooks(root));
        fixture_git(
            root,
            &["config", "--local", "core.hooksPath", "other-hooks"],
        )?;
        assert!(!check_hooks(root));
        Ok(())
    }

    #[test]
    fn unsafe_policy_cannot_be_removed_or_weakened() -> Result<()> {
        let (directory, _) = fixture()?;
        let root = directory.path();
        assert!(!check_rust_lints(root));
        let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
        for level in ["allow", "warn", "deny", "forbid"] {
            fs::write(
                root.join("Cargo.toml"),
                format!("{manifest}\n[lints.rust]\nunsafe_code = '{level}'\n"),
            )?;
            assert_eq!(check_rust_lints(root), level == "forbid");
        }
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "{manifest}\n[lints.rust]\nunsafe_code = {{ level = 'forbid', priority = 0 }}\n"
            ),
        )?;
        assert!(check_rust_lints(root));
        Ok(())
    }

    #[test]
    fn clean_but_invalid_older_commit_is_not_hidden_by_a_valid_tip() -> Result<()> {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        rewrite_version(root, "0.1.4")?;
        commit(root)?;
        rewrite_version(root, "0.1.5")?;
        commit(root)?;
        assert!(!check(root, &rules)?);
        Ok(())
    }

    #[test]
    fn untracked_staged_unstaged_and_deleted_files_fail_but_runtime_data_is_ignored() -> Result<()>
    {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        fs::create_dir(root.join("data"))?;
        fs::write(root.join("data/state.json"), "{}")?;
        assert!(check(root, &rules)?);
        fs::write(root.join("odd\nname.txt"), "new")?;
        assert!(!check(root, &rules)?);
        fixture_git(root, &["add", "--all"])?;
        assert!(!check(root, &rules)?);
        fs::write(root.join("odd\nname.txt"), "changed")?;
        assert!(!check(root, &rules)?);
        fs::remove_file(root.join("Cargo.lock"))?;
        assert!(!check(root, &rules)?);
        Ok(())
    }

    #[test]
    fn every_regular_commit_prepares_the_next_patch_and_updates_only_the_root_package() -> Result<()>
    {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        for expected in ["0.1.1", "0.1.2", "0.1.3"] {
            assert_eq!(prepare(root, &mut rules, None)?.to_string(), expected);
            commit(root)?;
            assert!(check(root, &rules)?);
        }
        assert!(fs::read_to_string(root.join("Cargo.lock"))?.contains("version = \"9.8.7\""));
        Ok(())
    }

    #[test]
    fn minor_release_needs_a_record_bound_to_the_exact_parent_and_then_resumes_patch_steps()
    -> Result<()> {
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        rewrite_version(root, "0.2.0")?;
        assert!(prepare(root, &mut rules, None).is_err());
        assert_eq!(
            prepare(root, &mut rules, Some("用户决定此阶段功能完成"))?.to_string(),
            "0.2.0"
        );
        assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.2.0");
        commit(root)?;
        assert!(check(root, &rules)?);
        assert_eq!(prepare(root, &mut rules, None)?.to_string(), "0.2.1");
        commit(root)?;
        assert!(check(root, &rules)?);
        rewrite_version(root, "0.3.0")?;
        commit(root)?;
        assert!(!check(root, &rules)?);
        Ok(())
    }

    #[test]
    fn missing_approval_major_jump_and_lock_mismatch_are_rejected() -> Result<()> {
        for version in ["0.2.0", "1.0.0", "0.1.9"] {
            let (directory, mut rules) = fixture()?;
            let root = directory.path();
            prepare(root, &mut rules, None)?;
            commit(root)?;
            rewrite_version(root, version)?;
            commit(root)?;
            assert!(!check(root, &rules)?);
        }
        let (directory, mut rules) = fixture()?;
        let root = directory.path();
        prepare(root, &mut rules, None)?;
        commit(root)?;
        fs::write(
            root.join("Cargo.lock"),
            "[[package]]\nname='sample'\nversion='0.1.0'\n",
        )?;
        commit(root)?;
        assert!(!check(root, &rules)?);
        Ok(())
    }
}
