use super::{RULES_FILE, Rules, Version, committed, git};
use anyhow::{Context, Result, ensure};
use std::path::Path;
use std::{fs, io};

impl Version {
    fn next_rollover(self) -> Result<Self> {
        ensure!(
            self.minor < 10 && self.patch < 100,
            "自动进位版本越界：{self}"
        );
        if self.patch < 99 {
            return self.next_patch();
        }
        if self.minor < 9 {
            return self.next_minor();
        }
        Ok(Self {
            major: self.major.checked_add(1).context("主版本溢出")?,
            minor: 0,
            patch: 0,
        })
    }
}

impl Rules {
    pub(super) fn next_version(&self, previous: Version) -> Result<Version> {
        if self.versioning.rollover_after.is_some() {
            previous.next_rollover()
        } else {
            previous.next_patch()
        }
    }

    pub(super) fn validate_step(
        &self,
        parent: Option<&str>,
        previous: Version,
        current: Version,
    ) -> Result<()> {
        let expected = self.next_version(previous)?;
        let approved = self.versioning.rollover_after.is_none()
            && current == previous.next_minor()?
            && parent.is_some_and(|parent| self.approves(parent, previous, current));
        ensure!(
            current == expected || approved,
            "版本 v{current} 不合规，下一次提交须为 v{expected}；仅旧规则接受匹配的阶段决定"
        );
        Ok(())
    }
}

fn recorded(root: &Path, commit: &str) -> Result<Rules> {
    Rules::parse(&committed(root, commit, RULES_FILE)?)
}

pub(super) fn validate_transition(root: &Path, rules: &Rules, parents: &[&str]) -> Result<()> {
    let current = rules.versioning.rollover_after.as_deref();
    let mut inherited_policy = false;
    for parent in parents {
        let inherited = recorded(root, parent)?;
        if let Some(boundary) = inherited.versioning.rollover_after {
            ensure!(
                current == Some(boundary.as_str()),
                "不能移动或撤销任何父提交已启用的自动进位规则"
            );
            inherited_policy = true;
        }
    }
    if !inherited_policy && current.is_some() {
        ensure!(
            current == parents.first().copied(),
            "自动进位切换点必须是直接父提交的完整提交号"
        );
    }
    Ok(())
}

pub(super) fn candidate_parents(root: &Path, tip: &str) -> Result<Vec<String>> {
    let path = root.join(git(root, &["rev-parse", "--git-path", "MERGE_HEAD"])?.trim());
    let mut parents = vec![tip.to_string()];
    match fs::read_to_string(path) {
        Ok(merges) => parents.extend(merges.lines().map(str::to_string)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(parents)
}

pub(super) fn reachable(root: &Path, parents: &[String]) -> Result<String> {
    let mut arguments = vec!["rev-list", "--topo-order", "--reverse", "--parents"];
    arguments.extend(parents.iter().map(String::as_str));
    git(root, &arguments)
}

#[cfg(test)]
#[path = "version_policy_tests.rs"]
mod tests;
