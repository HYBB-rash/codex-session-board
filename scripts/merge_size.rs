use super::{Statistics, measure_against, statistics};
use crate::{Rules, committed, git, git_output, version_policy};
use anyhow::{Context, Result, ensure};
use std::path::Path;

fn merged_tree(repository: &Path, first: &str, second: &str) -> Result<String> {
    let output = git_output(repository, &["merge-tree", "--write-tree", first, second])?;
    ensure!(
        matches!(output.status.code(), Some(0 | 1)),
        "无法重建自动合并：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    let tree = text.lines().next().context("自动合并没有返回源码树")?;
    ensure!(
        matches!(tree.len(), 40 | 64) && tree.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "无效的自动合并源码树"
    );
    Ok(tree.to_string())
}

fn authored_base(repository: &Path, commit: &str, parents: &[&str]) -> Result<String> {
    if parents.len() == 2 {
        return merged_tree(repository, parents[0], parents[1]);
    }
    if parents.len() > 2 {
        let rules = Rules::parse(&committed(repository, commit, "project-rules.json")?)?;
        ensure!(!rules.versioning.release_only, "新集成记录只接受两个父提交");
    }
    Ok(parents
        .first()
        .context("带入的开发提交缺少父提交")?
        .to_string())
}

fn authored_statistics(repository: &Path, base: &str, commit: &str) -> Result<Statistics> {
    let output = git_output(
        repository,
        &[
            "-c",
            "core.attributesFile=/dev/null",
            "diff",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--find-renames=50%",
            base,
            commit,
            "--",
        ],
    )?;
    ensure!(output.status.success(), "无法检查带入提交的规模");
    statistics(&output.stdout)
}

fn audit_imported(repository: &Path, first: &str, second: &str) -> Result<()> {
    let range = format!("{first}..{second}");
    let entries = git(repository, &["rev-list", "--reverse", "--parents", &range])?;
    for entry in entries.lines() {
        let mut fields = entry.split_whitespace();
        let commit = fields.next().context("带入历史缺少提交号")?;
        let parents = fields.collect::<Vec<_>>();
        let base = authored_base(repository, commit, &parents)?;
        let total = authored_statistics(repository, &base, commit)?;
        ensure!(
            total.allowed(),
            "带入提交 {commit} 的开发改动超限：{} 文件、{} 行",
            total.files,
            total.added + total.deleted
        );
    }
    println!(
        "[通过] 本次带入的 {} 条记录，其开发或冲突改动均符合规模限制。",
        entries.lines().count()
    );
    Ok(())
}

pub(super) fn measure(repository: &Path, snapshot: &Path) -> Result<Option<Statistics>> {
    let head = git_output(repository, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    if !head.status.success() {
        ensure!(head.status.code() == Some(1), "无法读取 HEAD");
        return Ok(None);
    }
    let head = String::from_utf8(head.stdout)?.trim().to_string();
    let parents = version_policy::candidate_parents(repository, &head)?;
    if parents.len() == 1 {
        return Ok(None);
    }
    ensure!(parents.len() == 2, "每次记录一个来源分支，整批候选统一验证");
    audit_imported(repository, &parents[0], &parents[1])?;
    let baseline = merged_tree(repository, &parents[0], &parents[1])?;
    println!("[合并] 自动合并基线 {baseline}；以下只统计暂存区相对该基线的新冲突修改。");
    Ok(Some(measure_against(
        repository,
        snapshot,
        Some(&baseline),
    )?))
}
