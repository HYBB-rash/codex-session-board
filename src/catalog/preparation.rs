use super::{input, legacy::Projection, membership, paging::PageScan};
use crate::rpc::Rpc;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

mod metadata;

pub(crate) struct Fresh {
    account: Value,
    state: Value,
    projection: Projection,
    pub(crate) source: Value,
}

impl Fresh {
    pub(crate) fn read(reader: &mut dyn Rpc, source: &Value) -> Result<Self> {
        let account = reader.call("account/read", json!({"refreshToken":false}))?;
        for value in [&account["userId"], &account["workspaceRouting"]["chatgptAccountId"]] {
            ensure!(value.as_str().is_some_and(|text| !text.is_empty()), "目标账户身份无法确认");
        }
        let home = source["codexHome"].as_str().context("目标缺少 Codex Home")?;
        let (bytes, _) = input::state_file(reader, &format!("{home}/.codex-global-state.json"))?;
        let state: Value = serde_json::from_slice(&bytes)?;
        let projection = Projection::parse(&state, home, &account)?;
        ensure!(projection.account_status == "verified" && projection.issues.is_empty(),
            "目标项目来源、账户或迁移状态不完整，请刷新后重试");
        let identity = json!([account["userId"],account["workspaceRouting"]["chatgptAccountId"]]);
        let stable = json!({"hostId":source["hostId"],"codexHome":home,
            "peerFingerprint":source["peerFingerprint"],
            "accountFingerprint":input::digest(&serde_json::to_vec(&identity)?)});
        Ok(Self { account, state, projection, source: stable })
    }

    pub(crate) fn server(&self, reader: &mut dyn Rpc, identifier: &str) -> Result<Value> {
        ensure!(!membership::deleting(identifier, &self.projection), "项目正在删除，不能新建会话");
        let project = reader.call("project/read", json!({"projectId":identifier}))?["project"].clone();
        ensure!(project["id"] == identifier, "目标项目读取回执不匹配");
        metadata::validate_canonical(&project["metadata"], &self.account, None)?;
        roots(&project)?;
        Ok(project)
    }

    fn legacy(&self, identifier: &str) -> Result<Value> {
        ensure!(!identifier.starts_with("g-p-"), "需要可用的原生本地项目");
        ensure!(!self.projection.pending_deletions.contains_key(identifier), "旧项目正在删除");
        let project = &self.state["local-projects"][identifier];
        ensure!(project["id"] == identifier, "旧项目已移除");
        ensure!(project["name"].as_str().is_some_and(|name| !name.trim().is_empty()),
            "旧项目名称无效");
        let paths = project["rootPaths"].as_array().context("旧项目缺少根目录")?;
        let roots: Vec<Value> = paths.iter().map(|path| json!({"path":path})).collect();
        roots_valid(&roots)?;
        let metadata = metadata::convert(identifier, project,
            &self.state["project-appearances"][identifier], &self.account)?;
        Ok(json!({"name":project["name"],"roots":roots,"metadata":metadata,
            "idempotencyKey":identifier}))
    }

    pub(crate) fn prepare_legacy(&self, reader: &mut dyn Rpc, identifier: &str, pages: usize) -> Result<Value> {
        let payload = self.legacy(identifier)?;
        let mut scan = PageScan::new("project/list", json!({"limit":100}));
        for _ in 0..pages.max(1) {
            scan.step(reader, self.source["hostId"].as_str().context("缺少目标主机")?)?;
            if scan.complete { break; }
        }
        ensure!(scan.complete, "目标项目目录尚未读完，不能导入旧项目");
        let mapped = self.projection.mappings.get(identifier);
        let candidates: Vec<_> = scan.items.iter().filter(|project|
            mapped == project.get("id") || project["metadata"]["codex.localProjectId"] == identifier).collect();
        ensure!(candidates.len() <= 1, "旧项目对应多个规范项目");
        let expected = mapped.or_else(|| candidates.first().and_then(|project| project.get("id")));
        if let Some(expected) = expected {
            let canonical = expected.as_str().context("旧项目映射无效")?;
            ensure!(!self.projection.mappings.iter().any(|(key,value)|
                key != identifier && value == expected), "多个旧项目映射到同一规范项目");
            let project = self.server(reader, canonical)?;
            self.validate_legacy_result(identifier, &payload, &project)?;
            return Ok(project);
        }
        ensure!(self.projection.checkpoint["version"] == 1
            && self.projection.checkpoint["projectsMigrated"] == false,
            "旧项目迁移状态已改变，请刷新");
        let project = reader.call("project/import", payload.clone())?["project"].clone();
        self.validate_legacy_result(identifier, &payload, &project)?;
        let canonical = project["id"].as_str().context("导入回执缺少项目 ID")?;
        let project = self.server(reader, canonical)?;
        self.validate_legacy_result(identifier, &payload, &project)?;
        Ok(project)
    }

    fn validate_legacy_result(&self, identifier: &str, payload: &Value, project: &Value) -> Result<()> {
        let canonical = project["id"].as_str().context("规范项目缺少 ID")?;
        ensure!(uuid::Uuid::parse_str(canonical)?.to_string() == canonical, "规范项目 ID 无效");
        metadata::validate_canonical(&project["metadata"], &self.account, Some(identifier))?;
        for key in ["codex.chatGptBacking","codex.chatGptBackingCreation","codex.retiredChatGptBacking"] {
            if let Some(expected) = payload["metadata"].get(key) {
                let actual = project["metadata"][key].as_str().context("规范项目关联缺失")?;
                ensure!(serde_json::from_str::<Value>(actual)?
                    == serde_json::from_str::<Value>(expected.as_str().context("旧项目关联无效")?)?,
                    "规范项目与旧项目 ChatGPT 关联冲突");
            }
        }
        roots(project)?;
        Ok(())
    }

    pub(crate) fn verify_after(&self, reader: &mut dyn Rpc, source: &Value, legacy: Option<&str>,
        project: &Value) -> Result<()> {
        let current = Self::read(reader, source)?;
        ensure!(current.source == self.source, "准备期间目标来源或账户发生变化");
        let canonical = project["id"].as_str().context("缺少规范项目 ID")?;
        ensure!(!membership::deleting(canonical, &current.projection), "准备期间项目开始删除");
        if let Some(legacy) = legacy {
            let payload = current.legacy(legacy)?;
            ensure!(self.legacy(legacy)? == payload, "准备期间旧项目内容发生变化");
            ensure!(current.projection.mappings.get(legacy).is_none_or(|mapped| mapped == canonical),
                "准备期间旧项目映射发生冲突");
            ensure!(!current.projection.mappings.iter().any(|(key,value)| key != legacy && value == canonical),
                "规范项目已关联其他旧项目");
            current.validate_legacy_result(legacy, &payload, project)?;
        }
        Ok(())
    }
}

pub(crate) fn roots(project: &Value) -> Result<Vec<Value>> {
    let roots = project["roots"].as_array().context("规范项目缺少根目录")?;
    roots_valid(roots)?;
    Ok(roots.clone())
}

fn roots_valid(roots: &[Value]) -> Result<()> {
    ensure!(!roots.is_empty() && roots.iter().all(|root| root["path"].as_str()
        .is_some_and(|path| path.starts_with('/') && !path.chars().any(char::is_control))),
        "项目根目录为空或无效");
    Ok(())
}
