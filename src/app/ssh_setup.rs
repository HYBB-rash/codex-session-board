mod files;
mod registry;
mod target;
#[cfg(test)]
mod tests;

use super::{App, open_client_url};
use anyhow::{Context, Result, bail, ensure};
use files::Files;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::{Path, PathBuf}};
use target::{Probe, Target, host};

#[derive(Clone, Serialize, Deserialize)]
struct Plan {
    target: Target,
    alias: String,
    generated: bool,
    before: [Option<String>; 2],
    after: [Option<String>; 2],
    proof: Value,
    registry: String,
    files: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Decision {
    id: String,
    state: State,
    plan: Plan,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum State {
    Preview,
    Declined,
    Approved,
    FilesWritten,
    LinkDelivered,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl App {
    pub(super) fn ssh_setup_enabled(&self) -> bool {
        self.config.host_id == "local" && !self.config.legacy_mode
            && self.config.ssh_command == ["ssh"] && self.config.sidebar_state_file.is_absolute()
    }

    pub(super) fn ssh_setup(&self, body: &Value) -> Result<Value> {
        ensure!(self.ssh_setup_enabled(), "当前配置不支持 SSH 自动设置");
        self.ssh_setup_at(body, &ssh_directory()?)
    }

    fn ssh_setup_at(&self, body: &Value, directory: &Path) -> Result<Value> {
        let operation = self.thread_lock("ssh-setup")?;
        let _guard = operation.lock().map_err(|_| anyhow::anyhow!("SSH 配置操作锁已损坏"))?;
        match body["action"].as_str().context("缺少 SSH 设置操作")? {
            "status" => self.ssh_setup_status(directory),
            "preview" => {
                let target = Target::parse(&body["target"])?;
                if let Some(decision) = self.ssh_decision(&target.key()?)? {
                    return self.ssh_decision_status(&decision, directory);
                }
                self.preview_ssh_setup(target, directory, body["alias"].as_str())
            }
            "restore" => {
                let decision = self.ssh_decision(key(body, "targetId")?)?.context("找不到拒绝记录")?;
                ensure!(decision.state == State::Declined, "拒绝状态已改变，请重新打开连接设置");
                self.preview_ssh_setup(decision.plan.target, directory, body["alias"].as_str())
            }
            "recheck" => {
                let decision = self.ssh_decision(key(body, "targetId")?)?.context("找不到配置记录")?;
                ensure!(decision.state != State::Declined, "请先在连接设置中重新启用");
                self.preview_ssh_setup(decision.plan.target, directory, body["alias"].as_str())
            }
            "decline" | "apply" => self.decide_ssh_setup(body, directory),
            _ => bail!("未知 SSH 设置操作"),
        }
    }

    fn ssh_setup_status(&self, directory: &Path) -> Result<Value> {
        let records = self.store.ssh_setup_records()?;
        let mut rows = Vec::new();
        for record in records {
            let decision: Decision = serde_json::from_value(record)?;
            rows.push(self.ssh_decision_status(&decision, directory)?);
        }
        Ok(json!({"state":"form","saved":rows}))
    }

    fn ssh_decision_status(&self, decision: &Decision, directory: &Path) -> Result<Value> {
        if decision.state.may_be_registered()
            && decision.plan.applied(self, directory).unwrap_or(false)
        {
            return Ok(json!({"state":"registered","summary":decision.summary(),
                "message":"已读回 Codex 连接和目录项目。"}));
        }
        let declined = decision.state == State::Declined;
        let message = if declined {
            "该连接的一键远程新建已关闭。已有会话操作仍可使用。"
        } else if decision.state == State::LinkDelivered {
            "已交付 Codex 打开请求。请核对客户端中的主机和目录。"
        } else {
            "配置操作尚未完成，可以续做已同意的改动。"
        };
        Ok(json!({"state":decision.state,"summary":decision.summary(),"message":message,
            "targetId":decision.plan.target.key()?,"previewId":decision.id}))
    }

    fn preview_ssh_setup(&self, target: Target, directory: &Path, selection: Option<&str>) -> Result<Value> {
        let files = Files::read(directory)?;
        let plan = match build_plan(self, target, &files, selection)? {
            PlanChoice::Ready(plan) => *plan,
            PlanChoice::Aliases(aliases) => return Ok(json!({"state":"selectAlias","aliases":aliases})),
        };
        let result = plan.preview(&files);
        let decision = Decision { id: uuid::Uuid::new_v4().to_string(), state: State::Preview, plan };
        self.remember_ssh_preview(decision.clone())?;
        Ok(json!({"state":"preview","previewId":decision.id,"summary":decision.summary(),
            "changes":result,"canApply":true}))
    }

    fn remember_ssh_preview(&self, decision: Decision) -> Result<()> {
        let mut previews = self.ssh_previews.lock().map_err(|_| anyhow::anyhow!("SSH 预览缓存锁已损坏"))?;
        if previews.len() >= 32 {
            previews.clear();
        }
        previews.insert(decision.plan.target.key()?, decision);
        Ok(())
    }

    fn decide_ssh_setup(&self, body: &Value, directory: &Path) -> Result<Value> {
        let id = body["previewId"].as_str().context("缺少配置预览 ID")?;
        ensure!(uuid::Uuid::parse_str(id)?.to_string() == id, "配置预览 ID 无效");
        let preview = self.ssh_previews.lock().map_err(|_| anyhow::anyhow!("SSH 预览缓存锁已损坏"))?
            .values().find(|record| record.id == id).cloned();
        let mut decision = match preview {
            Some(preview) => preview,
            None => self.store.ssh_setup_records()?.into_iter()
                .map(serde_json::from_value::<Decision>)
                .collect::<std::result::Result<Vec<_>, _>>()?.into_iter()
                .find(|record| record.id == id).context("配置预览已失效，请重新预览")?,
        };
        if body["action"] == "decline" {
            ensure!(decision.state == State::Preview, "配置决定已记录");
            decision.state = State::Declined;
            self.save_ssh_decision(&decision)?;
            return self.ssh_decision_status(&decision, directory);
        }
        ensure!(decision.state != State::Declined, "请先在连接设置中重新启用");
        self.apply_ssh_setup(&mut decision, directory)
    }

    fn apply_ssh_setup(&self, decision: &mut Decision, directory: &Path) -> Result<Value> {
        let files = Files::read(directory)?;
        if decision.state.may_be_registered() && decision.plan.applied(self, directory)? {
            decision.plan.verify_target(self, &files)?;
            decision.plan.verify_installed(self)?;
            return Ok(json!({"state":"registered","summary":decision.summary(),
                "message":"已核对 SSH 目标并读回 Codex 连接和目录项目。"}));
        }
        if decision.state == State::Preview {
            ensure!(digest(&serde_json::to_vec(&files)?) == decision.plan.files,
                "SSH 配置已改变，请关闭后重新预览");
            decision.state = State::Approved;
        }
        decision.plan.verify_target(self, &files)?;
        self.save_ssh_decision(decision)?;
        let changes = decision.plan.remaining(&files)?;
        ensure!(Files::read(directory)? == files, "SSH 配置已改变，请重新预览");
        registry::ready(self, &decision.plan, decision.state.may_be_registered())?;
        files.publish(&changes)?;
        decision.state = State::FilesWritten;
        self.save_ssh_decision(decision)?;
        decision.plan.verify_target(self, &Files::read(directory)?)?;
        decision.plan.verify_installed(self)?;
        if registry::ready(self, &decision.plan, true)? {
            return self.ssh_decision_status(decision, directory);
        }
        open_client_url(decision.plan.url()?, &self.config)?;
        decision.state = State::LinkDelivered;
        self.save_ssh_decision(decision)?;
        Ok(json!({"state":"applied","message":"已交付 Codex 打开请求。请核对主机和目录，再发送第一句话。"}))
    }

    fn ssh_decision(&self, key: &str) -> Result<Option<Decision>> {
        self.store.ssh_setup_record(key)?.map(serde_json::from_value).transpose().map_err(Into::into)
    }

    fn save_ssh_decision(&self, decision: &Decision) -> Result<()> {
        let target = decision.plan.target.key()?;
        self.store.save_ssh_setup(&target, &serde_json::to_value(decision)?)?;
        self.ssh_previews.lock().map_err(|_| anyhow::anyhow!("SSH 预览缓存锁已损坏"))?.remove(&target);
        Ok(())
    }
}

impl Decision {
    fn summary(&self) -> String {
        self.plan.target.summary(&self.plan.alias)
    }
}

impl State {
    fn may_be_registered(self) -> bool {
        matches!(self, Self::FilesWritten | Self::LinkDelivered)
    }
}

impl Plan {
    fn verify_installed(&self, app: &App) -> Result<()> {
        if app.config.ssh_command != ["ssh"] {
            return Ok(());
        }
        let probe = Probe::installed(&app.config)?;
        let alias = host(&self.target, Some(&self.alias))?;
        let direct = host(&self.target, None)?;
        let settings = probe.settings(&alias)?;
        ensure!(settings == probe.settings(&direct)? && settings["proxycommand"].is_null()
            && settings["proxyjump"].is_null(), "OpenSSH 载入的别名与直连目标不同");
        ensure!(probe.proof(&alias, &self.target)? == self.proof,
            "OpenSSH 实际加载的别名指向不同的远端");
        Ok(())
    }

    fn applied(&self, app: &App, directory: &Path) -> Result<bool> {
        Ok(registry::ready(app, self, true)? && Files::read(directory)?.hashes() == self.after)
    }

    fn remaining(&self, files: &Files) -> Result<[Option<String>; 2]> {
        let block = self.generated.then(|| self.target.block(&self.alias));
        files.remaining(&self.before, &self.after, block.as_deref())
    }

    fn verify_target(&self, app: &App, files: &Files) -> Result<()> {
        let changes = self.remaining(files)?;
        files.check_writable(&changes)?;
        let original = Probe::new(&app.config, &files.expanded()?)?;
        let direct = host(&self.target, None)?;
        let proof = original.proof(&direct, &self.target)?;
        ensure!(proof == self.proof, "远端身份已改变，请关闭后重新预览");
        let desired = Probe::new(&app.config, &files.effective_after(&changes))?;
        let alias = host(&self.target, Some(&self.alias))?;
        ensure!(desired.settings(&alias)? == original.settings(&direct)? &&
            desired.proof(&alias, &self.target)? == proof, "SSH 别名与授权目标不同");
        Ok(())
    }

    fn preview(&self, files: &Files) -> Vec<Value> {
        let mut result = Vec::new();
        let paths = files.paths();
        if self.generated {
            result.push(json!({"label":format!("配置 {}",paths[1].display()),
                "detail":self.target.block(&self.alias).trim_start().lines()
                    .map(|line| format!("+ {line}")).collect::<Vec<_>>().join("\n")}));
            if self.before[0] != self.after[0] {
                result.push(json!({"label":format!("在 {} 首行加入 Include",paths[0].display()),
                    "detail":"+ Include ~/.ssh/codex-session-board.conf\n+ Host *"}));
            }
        }
        result.push(json!({"label":format!("新增 Codex 连接 {}，登记 {} 并开启自动连接",
            self.alias,self.target.directory)}));
        result
    }

    fn url(&self) -> Result<String> {
        crate::native_new::alias_url(&self.alias, &self.target.directory)
    }
}

enum PlanChoice {
    Ready(Box<Plan>),
    Aliases(Vec<String>),
}

fn build_plan(app: &App, target: Target, files: &Files, selected: Option<&str>) -> Result<PlanChoice> {
    let expanded = files.expanded()?;
    let probe = Probe::new(&app.config, &expanded)?;
    let direct = host(&target, None)?;
    let settings = probe.settings(&direct)?;
    let proof = probe.proof(&direct, &target)?;
    let state = registry::read(app)?;
    let mut reserved = Files::declares_aliases(&expanded);
    reject_existing(app, &probe, &target, &proof, &mut reserved)?;
    let matches = matching_aliases(&probe, &target, &settings, &reserved)?;
    let generated = matches.is_empty();
    let alias = match (matches.as_slice(), selected) {
        (_, Some(selected)) => matches.iter().find(|alias| alias.as_str() == selected)
            .cloned().context("所选别名已改变，请重新检查")?,
        ([alias], None) => alias.clone(),
        ([], None) => target.alias(&reserved)?,
        (_, None) => return Ok(PlanChoice::Aliases(matches)),
    };
    registry::reserve(&state, &alias)?;
    let registry_hash = registry::fingerprint(&state, &format!("remote-ssh-discovered:{alias}"))?;
    let changes = if generated { files.changes(&target.block(&alias)) } else { [None,None] };
    files.check_writable(&changes)?;
    let desired = Probe::new(&app.config, &files.effective_after(&changes))?;
    let candidate = host(&target, Some(&alias))?;
    ensure!(desired.settings(&candidate)? == settings && desired.proof(&candidate, &target)? == proof,
        "拟用别名与远端目标不同，请在 Codex 中手动配置");
    let before = files.hashes();
    let after = std::array::from_fn(|index| changes[index].as_ref()
        .map(|value| digest(value.as_bytes())).or_else(|| before[index].clone()));
    Ok(PlanChoice::Ready(Box::new(Plan { target, alias, generated, before, after, proof,
        registry:registry_hash, files:digest(&serde_json::to_vec(files)?) })))
}

fn reject_existing(app: &App, probe: &Probe, target: &Target, proof: &Value, reserved: &mut BTreeSet<String>) -> Result<()> {
    for registered in crate::remote::hosts::registered(&app.config)? {
        if let Some(alias) = &registered.alias {
            ensure!(target::name(alias), "既有 Codex 连接的 SSH 别名需要手动核对");
            reserved.insert(alias.clone());
        }
        let settings = probe.settings(&registered)?;
        let endpoint = target.endpoint()?;
        let same_target = settings["hostname"][0] == endpoint.hostname
            && settings["user"][0] == endpoint.user && settings["port"][0] == target.port;
        let binding = crate::remote::hosts::source::Binding::existing(&registered, &app.config);
        let identity = binding.and_then(|binding| binding.mapping_identity());
        ensure!(!same_target && !identity.as_ref().is_ok_and(|record| record["peer"] == proof["peer"]),
            "Codex 已登记同一连接，请使用原连接和项目");
        ensure!(identity.is_ok(),
            "无法排除既有 Codex 连接指向同一环境，请先核对现有连接");
    }
    Ok(())
}

fn matching_aliases(probe: &Probe, target: &Target, direct: &Value, names: &BTreeSet<String>) -> Result<Vec<String>> {
    let mut matches = Vec::new();
    ensure!(names.len() <= 100, "SSH 别名过多，请手动选择和登记");
    for name in names {
        if probe.settings(&host(target, Some(name))?)? == *direct {
            matches.push(name.clone());
        }
    }
    Ok(matches)
}

fn ssh_directory() -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").context("无法定位 SSH 配置目录")?);
    ensure!(home.is_absolute() && home.canonicalize()? == home, "SSH 主目录需要手动核对");
    Ok(home.join(".ssh"))
}

fn key<'body>(body: &'body Value, field: &str) -> Result<&'body str> {
    let key = body[field].as_str().context("缺少 SSH 目标 ID")?;
    ensure!(key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()), "SSH 目标 ID 无效");
    Ok(key)
}
