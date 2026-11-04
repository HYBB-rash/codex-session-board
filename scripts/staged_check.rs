use super::{
    RULES_FILE, Rules, check_comments, check_hooks, check_rust_lints, commit_size, document_policy,
    git_output, locked_version, package, root_directory, validate_history,
};
use anyhow::{Context, Result, ensure};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

fn snapshot(repository: &Path) -> Result<TempDir> {
    let directory = tempfile::tempdir()?;
    let prefix = format!(
        "--prefix={}/",
        directory
            .path()
            .to_str()
            .context("暂存快照路径不是 UTF-8")?
    );
    let output = git_output(repository, &["checkout-index", "--all", &prefix])?;
    ensure!(
        output.status.success(),
        "无法导出暂存快照：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(directory)
}

fn version(root: &Path, repository: &Path, rules: &Rules) -> Result<bool> {
    let history = validate_history(repository, rules)?;
    let (name, current) = package(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    ensure!(
        locked_version(&fs::read_to_string(root.join("Cargo.lock"))?, &name)? == current,
        "暂存区 Cargo.toml 与 Cargo.lock 的版本不一致"
    );
    let patch = history.version.next_patch()?;
    let approved = current == history.version.next_minor()?
        && history
            .tip
            .as_deref()
            .is_some_and(|parent| rules.approves(parent, history.version, current));
    ensure!(
        current == patch || approved,
        "暂存区版本 v{current} 不合规，普通提交应为 v{patch}；阶段完成须有匹配的用户决定"
    );
    println!(
        "[通过] 暂存区版本 v{current}，历史 {} 次提交的版本步进正确。",
        history.count
    );
    Ok(true)
}

fn hook(root: &Path, repository: &Path) -> Result<bool> {
    let passed = check_hooks(repository);
    let path = root.join(".githooks/pre-commit");
    ensure!(
        path.is_file() && fs::metadata(path)?.permissions().mode() & 0o111 != 0,
        "暂存区必须保留可执行的 .githooks/pre-commit"
    );
    Ok(passed)
}

pub fn check(repository: Option<&Path>) -> Result<bool> {
    let repository = repository
        .map(Path::to_path_buf)
        .map_or_else(root_directory, Ok)?;
    let directory = snapshot(&repository)?;
    let root = directory.path();
    let size_passed = commit_size::check(&repository, root)?;
    let rules = Rules::parse(&fs::read_to_string(root.join(RULES_FILE))?)?;
    let comments_passed = check_comments(root, &repository, &rules)?;
    let documents_passed = document_policy::check(root)?;
    let hooks_passed = hook(root, &repository)?;
    let lints_passed = check_rust_lints(root);
    let version_passed = match version(root, &repository, &rules) {
        Ok(passed) => passed,
        Err(error) => {
            println!("[未通过] 暂存区版本规则：{error:#}");
            println!(
                "  核对历史后运行 ./scripts/check --prepare-commit，并重新暂存版本文件及适用的阶段决定。"
            );
            false
        }
    };
    Ok(size_passed
        && comments_passed
        && documents_passed
        && hooks_passed
        && lints_passed
        && version_passed)
}

#[cfg(test)]
#[path = "staged_check_tests.rs"]
mod tests;
