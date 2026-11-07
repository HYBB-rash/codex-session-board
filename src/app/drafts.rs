use super::*;
use crate::rpc::{Rpc, runtime::RuntimeConnection};

struct Draft<'app> {
    remote: &'app App,
    project: Value,
    record: Value,
    request: String,
    fresh: bool,
}

impl App {
    pub(crate) fn new_remote_session(&self, body: &Value) -> Result<Value> {
        let request = body["requestId"].as_str().context("缺少新建请求 ID")?;
        ensure!(
            uuid::Uuid::parse_str(request)?.to_string() == request,
            "新建请求 ID 无效"
        );
        let mut caller = self.config.clone();
        caller.host_id = "local".into();
        let timeout = Duration::from_secs(caller.rpc_timeout_seconds);
        let project = self.remote_draft_project(body, &caller, timeout)?;
        let host = project["hostId"].as_str().context("项目缺少主机")?;
        let remote = self.remote_app(host)?;
        let _operation = remote.operation()?;
        let operation = remote.thread_lock(&format!("draft:{}", project["nativeId"]))?;
        let _guard = operation
            .lock()
            .map_err(|_| anyhow::anyhow!("新建会话锁已损坏"))?;
        let mut reader = RuntimeConnection::draft(&remote)?;
        let mut writer = SidebarConnection::project_writer(&caller, timeout)?;
        let (record, fresh) = self
            .store
            .draft_begin(project["id"].as_str().context("项目 ID 无效")?, request)?;
        let draft = Draft {
            remote: &remote,
            project,
            record,
            request: request.into(),
            fresh,
        };
        match self.finish_remote_draft(&draft, &mut reader, &mut writer) {
            Ok(result) => Ok(result),
            Err(error) => Ok(json!({"ok":false,"pending":true,"requestId":request,
                "error":format!("{error:#}")})),
        }
    }

    fn remote_draft_project(
        &self,
        body: &Value,
        caller: &Config,
        timeout: Duration,
    ) -> Result<Value> {
        let mut catalog = SidebarConnection::reader(caller, timeout)?;
        let project = crate::remote::projects(&mut catalog)?
            .into_iter()
            .find(|project| project["id"] == body["projectId"])
            .context("远程项目已移除，请刷新")?;
        draft_params(&project)?;
        ensure!(
            body["hostId"].is_null() || body["hostId"] == project["hostId"],
            "新建目标主机不一致"
        );
        Ok(project)
    }

    fn finish_remote_draft(
        &self,
        draft: &Draft<'_>,
        reader: &mut dyn Rpc,
        writer: &mut dyn Rpc,
    ) -> Result<Value> {
        ensure!(
            draft.record["stage"] != "abandoned",
            "此前的新建请求已放弃，请重新新建"
        );
        let thread = draft.thread(&self.store, reader)?;
        let identifier = thread["id"].as_str().context("新建会话缺少 ID")?;
        if draft.record["stage"] != "complete" {
            writer.call(
                "move_thread_to_project",
                json!({"threadId":identifier,
                "source":"codex","hostId":draft.remote.config.host_id,
                "projectId":draft.project["nativeId"]}),
            )?;
            confirm_project(&self.config, &draft.project, identifier)?;
            self.cache_draft(draft, &thread)?;
            self.store.draft_update(&draft.request, None, "complete")?;
        }
        let url = open_client_url(
            conversation_url(identifier, &draft.remote.config.host_id)?,
            &self.config,
        )?;
        Ok(
            json!({"ok":true,"url":url,"id":crate::store::remote::identity(
            &draft.remote.config.host_id,identifier)}),
        )
    }

    fn cache_draft(&self, draft: &Draft<'_>, thread: &Value) -> Result<()> {
        draft.remote.store.discover(std::slice::from_ref(thread))?;
        let mut metadata = thread.clone();
        metadata["hostId"] = json!(draft.remote.config.host_id);
        metadata["projectId"] = draft.project["nativeId"].clone();
        metadata["title"] = json!(thread["name"].as_str().unwrap_or("新会话"));
        self.store.discover_remote(
            &draft.remote.config.host_id,
            draft.project["hostLabel"].as_str().unwrap_or(""),
            &[metadata],
        )?;
        self.remote_refresh.store(true, Ordering::Relaxed);
        Ok(())
    }
}

impl Draft<'_> {
    fn thread(&self, store: &Store, reader: &mut dyn Rpc) -> Result<Value> {
        let thread = if self.fresh {
            let result = reader.call("thread/start", draft_params(&self.project)?)?;
            let thread = result["thread"].clone();
            store.draft_update(
                &self.request,
                Some(thread["id"].as_str().context("新建回执缺少会话 ID")?),
                "created",
            )?;
            thread
        } else {
            let identifier = self.record["threadId"]
                .as_str()
                .context("新建结果未确认，请先在客户端核对；不会再次发送创建请求")?;
            if self.record["stage"] == "complete" {
                return Ok(json!({"id":identifier}));
            }
            let thread = reader.call(
                "thread/read",
                json!({"threadId":identifier,"includeTurns":true}),
            )?["thread"]
                .clone();
            ensure!(thread["id"] == identifier, "新建会话读回 ID 不一致");
            thread
        };
        ensure!(thread["id"].is_string(), "会话读回缺少 ID");
        ensure!(
            thread["status"]["type"] != "active",
            "会话正在执行，请在客户端核对"
        );
        ensure!(
            thread["turns"].as_array().is_none_or(Vec::is_empty),
            "该会话已有执行记录，请在客户端核对后继续"
        );
        Ok(thread)
    }
}

fn draft_params(project: &Value) -> Result<Value> {
    let paths = project["paths"].as_array().context("远程项目路径无效")?;
    ensure!(
        !paths.is_empty()
            && paths
                .iter()
                .all(|path| path.as_str().is_some_and(|path| path.starts_with('/'))),
        "远程项目路径必须为绝对路径"
    );
    Ok(json!({"cwd":paths[0],"runtimeWorkspaceRoots":paths,"ephemeral":false}))
}

fn confirm_project(config: &Config, project: &Value, thread: &str) -> Result<()> {
    let deadline =
        std::time::Instant::now() + Duration::from_secs(config.rpc_timeout_seconds.min(4));
    loop {
        let state: Value = serde_json::from_slice(&std::fs::read(&config.sidebar_state_file)?)?;
        let assignment = &state["thread-project-assignments"][thread];
        if assignment["projectKind"] == "remote"
            && assignment["projectId"] == project["nativeId"]
            && state["thread-project-membership-host-ids"][thread] == project["hostId"]
        {
            return Ok(());
        }
        ensure!(
            std::time::Instant::now() < deadline,
            "项目绑定结果尚未确认，请重试打开同一会话"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod tests;
