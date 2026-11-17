use super::*;
use crate::rpc::Rpc;

impl App {
    pub(super) fn target_project(&self, identifier: &str) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "六状态兼容模式不支持远程新建或项目补登记");
        let fields: Vec<String> = serde_json::from_str(
            identifier.strip_prefix("target:").context("目标项目 ID 无效")?)?;
        ensure!(fields.len() == 3 && matches!(fields[1].as_str(), "targetServer" | "targetLegacy")
            && identifier == format!("target:{}", json!(fields)), "目标项目 ID 无效");
        let remote = self.remote_app(&fields[0])?;
        let state = remote.catalog.lock().map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
        ensure!(state.projects_ready(), "项目目录尚未实时读回，请刷新后重试");
        let project = state.snapshot.as_ref().context("目标目录尚未加载")?.projects.iter()
            .find(|project| project["id"] == identifier).context("目标项目已移除，请刷新")?;
        ensure!(project["hostId"] == fields[0] && project["origin"] == fields[1]
            && project["nativeId"] == fields[2], "目标目录的项目身份不一致");
        ensure!(project["pendingDeletion"] != true, "项目正在删除，不能新建或补登记");
        crate::native_new::directory(project)?;
        Ok(project.clone())
    }

    pub(super) fn assign_project(&self, body: &Value) -> Result<Value> {
        ensure!(!self.config.legacy_mode && body["confirmed"] == true, "请先确认实际新会话和目标项目");
        let identifier = body["id"].as_str().context("缺少实际会话 ID")?;
        let (host, thread) = super::remote::remote_identity(identifier)?;
        let project = self.target_project(body["projectId"].as_str().context("缺少目标项目")?)?;
        ensure!(project["hostId"] == host && project["origin"] == "targetServer",
            "只能补登记到同一主机的后台项目");
        let remote = self.remote_app(&host)?;
        let _operation = remote.operation()?;
        let lock = remote.thread_lock(&thread)?;
        let _guard = lock.lock().map_err(|_| anyhow::anyhow!("会话锁已损坏"))?;
        let rows = crate::catalog::view::read(&remote, &[])?.rows;
        ensure!(rows.iter().any(|row| row["id"] == thread && row["ordinary"] == true
            && row["catalogCurrent"] == true && row["archived"] != true),
            "请选择已读回的活动原生会话");
        let mut connection = remote.connection(Duration::from_secs(
            remote.config.rpc_timeout_seconds), Role::ProjectWriter)?;
        remote.assignment_account(&mut connection)?;
        let result = assign(&mut connection, &thread, &project);
        remote.catalog_refresh.store(true, Ordering::SeqCst);
        result.map(|()| json!({"ok":true,"id":identifier,"projectId":project["id"]}))
    }

    fn assignment_account(&self, reader: &mut dyn Rpc) -> Result<()> {
        let expected = self.catalog.lock().map_err(|_| anyhow::anyhow!("目录锁已损坏"))?
            .snapshot.as_ref().context("目录尚未加载")?.source["accountFingerprint"].clone();
        let account = reader.call("account/read", json!({"refreshToken":false}))?;
        let actual = crate::catalog::account_fingerprint(&account)?;
        ensure!(expected.is_string() && json!(actual) == expected,
            "后台账号与项目目录尚未确认一致，请刷新目录后重试");
        Ok(())
    }
}

fn native_thread(reader: &mut dyn Rpc, identifier: &str, project: &Value) -> Result<Value> {
    let thread = reader.call("thread/read",
        json!({"threadId":identifier,"includeTurns":true}))?["thread"].clone();
    ensure!(thread["id"] == identifier && thread["cwd"] == crate::native_new::directory(project)?,
        "会话身份或执行目录与所选项目不一致");
    ensure!(thread["source"] == "vscode" && thread["originator"] == "Codex Desktop"
        && thread["ephemeral"] == false && thread["archived"] != true,
        "仅支持持久原生桌面会话，请先在 Codex 中核对");
    ensure!(thread.get("projectId").is_some() && (thread["projectId"].is_null()
        || thread["projectId"] == project["nativeId"]), "会话已归属其他项目或后台未提供归属字段");
    ensure!(thread["turns"].as_array().is_some_and(|turns| turns.iter().any(|turn|
        turn["items"].as_array().is_some_and(|items| items.iter().any(|item|
            item["type"] == "userMessage")))), "尚未读回用户首条输入，请先发送后刷新");
    Ok(thread)
}

fn assign(reader: &mut dyn Rpc, identifier: &str, project: &Value) -> Result<()> {
    let current = reader.call("project/read", json!({"projectId":project["nativeId"]}))?;
    let mut actual = project.clone();
    actual["paths"] = json!(current["project"]["roots"].as_array().context("项目缺少目录")?
        .iter().map(|root| root["path"].as_str().or_else(|| root.as_str()))
        .collect::<Vec<_>>());
    ensure!(current["project"]["id"] == project["nativeId"]
        && actual["paths"] == project["paths"], "项目目录已改变，请刷新后重新选择");
    let before = native_thread(reader, identifier, project)?;
    if before["projectId"] != project["nativeId"] {
        reader.call("thread/metadata/update",
            json!({"threadId":identifier,"projectId":project["nativeId"]}))
            .context("项目补登记未确认；保留同一会话，可刷新后重试。后台需支持 thread/metadata/update")?;
    }
    let after = native_thread(reader, identifier, project)?;
    ensure!(after["projectId"] == project["nativeId"],
        "项目补登记尚未读回；请对同一会话重试，不要重新新建");
    Ok(())
}

#[cfg(test)]
mod tests;
