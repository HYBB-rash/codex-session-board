use super::{FeatureRelease, History, RULES_FILE, Rules, Version, committed, git, package};
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
        if self.versioning.release_only {
            ensure!(
                current == previous
                    || current == self.next_version(previous)?
                        && parent.is_some_and(|parent| self.approves(parent, previous, current)),
                "普通提交保持 v{previous}；升版须由 --prepare-release 准备发布记录"
            );
            return Ok(());
        }
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
        ensure!(
            !inherited.versioning.release_only || rules.versioning.release_only,
            "不能撤销任何父提交已启用的按发布升版规则"
        );
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

pub(super) fn inherited_version(
    root: &Path,
    rules: &Rules,
    parents: &[&str],
    previous: Version,
) -> Result<Version> {
    if !rules.versioning.release_only || parents.len() < 2 {
        return Ok(previous);
    }
    parents.iter().try_fold(previous, |version, parent| {
        Ok(version.max(package(&committed(root, parent, "Cargo.toml")?)?.1))
    })
}

pub(super) fn prepared_version(
    history: &History,
    rules: &Rules,
    current: Version,
    decision: Option<&str>,
) -> Result<Option<Version>> {
    if !rules.versioning.release_only {
        return Ok(None);
    }
    ensure!(decision.is_none(), "按发布升版使用 --prepare-release");
    let next = rules.next_version(history.version)?;
    let approved = history
        .tip
        .as_deref()
        .is_some_and(|parent| rules.approves(parent, history.version, next));
    let target = if approved { next } else { history.version };
    ensure!(
        current == history.version || current == target,
        "当前 v{current} 与本次允许的 v{target} 不一致"
    );
    Ok(Some(target))
}

pub(super) fn prepare_release(root: &Path, rules: &mut Rules) -> Result<()> {
    ensure!(rules.versioning.release_only, "须先启用按发布升版规则");
    let history = super::validate_history(root, rules)?;
    let parent = history.tip.as_deref().context("发布须有已提交的源码基线")?;
    ensure!(
        candidate_parents(root, parent)?.len() == 1,
        "先完成正在进行的合并，再冻结发布候选"
    );
    let target = rules.next_version(history.version)?;
    if !rules.approves(parent, history.version, target) {
        rules.versioning.feature_releases.push(FeatureRelease {
            after_commit: parent.to_string(),
            from: history.version.to_string(),
            to: target.to_string(),
            decision: "冻结发布候选".to_string(),
        });
    }
    fs::write(
        root.join(RULES_FILE),
        format!("{}\n", serde_json::to_string_pretty(rules)?),
    )?;
    super::prepare(root, rules, None)?;
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

pub(super) fn plugin_versions(
    root: &Path,
    target: Version,
) -> Result<Vec<(std::path::PathBuf, String)>> {
    let mut prepared = Vec::new();
    for relative in ["plugin.json", ".codex-plugin/plugin.json"] {
        let path = root.join(relative);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let mut manifest: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&text).with_context(|| format!("{relative} 无法解析"))?;
        let version = manifest.get_mut("version").context("插件缺少 version")?;
        Version::parse(version.as_str().context("插件 version 必须是字符串")?)?;
        *version = serde_json::Value::String(target.to_string());
        prepared.push((
            path,
            format!("{}\n", serde_json::to_string_pretty(&manifest)?),
        ));
    }
    ensure!(
        prepared.is_empty() || prepared.len() == 2,
        "plugin.json 与 .codex-plugin/plugin.json 必须同时存在"
    );
    Ok(prepared)
}

#[cfg(test)]
#[path = "version_policy_tests.rs"]
mod tests;
