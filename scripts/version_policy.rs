use super::{RULES_FILE, Rules, Version, committed};
use anyhow::{Context, Result, ensure};
use std::path::Path;

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

pub(super) fn validate_transition(root: &Path, rules: &Rules, parent: Option<&str>) -> Result<()> {
    let inherited = parent.map(|parent| recorded(root, parent)).transpose()?;
    let previous = inherited
        .as_ref()
        .and_then(|rules| rules.versioning.rollover_after.as_deref());
    let current = rules.versioning.rollover_after.as_deref();
    if previous.is_some() {
        ensure!(current == previous, "不能移动或撤销自动进位规则切换点");
    } else if current.is_some() {
        ensure!(
            current == parent,
            "自动进位切换点必须是直接父提交的完整提交号"
        );
        ensure!(parent.is_some(), "自动进位切换须有既有提交");
    }
    Ok(())
}

pub(super) fn validate_merge(root: &Path, rules: &Rules, parents: &[&str]) -> Result<()> {
    for parent in parents.iter().skip(1) {
        let inherited = recorded(root, parent)?;
        if let Some(boundary) = inherited.versioning.rollover_after {
            ensure!(
                rules.versioning.rollover_after.as_deref() == Some(boundary.as_str()),
                "合并提交不能撤销或替换已继承的自动进位规则"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "version_policy_tests.rs"]
mod tests;
