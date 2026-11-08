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
    pub(crate) fn reset_remote_draft(&self, body: &Value) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "六状态兼容模式不支持远程新建");
        ensure!(body["confirmed"] == true, "请先在客户端核对新建结果");
        let request = body["requestId"].as_str().context("缺少新建请求 ID")?;
        let record = self.store.draft(request)?;
        ensure!(record["project"] == body["projectId"], "新建请求项目不一致");
        match record["stage"].as_str() {
            Some("starting" | "created") => {
                self.store.draft_update(request, None, "abandoned")?;
            }
            Some("complete" | "abandoned") => {}
            _ => bail!("新建请求状态异常"),
        }
        Ok(json!({"ok":true}))
    }

    pub(crate) fn new_remote_session(&self, body: &Value) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "六状态兼容模式不支持远程新建");
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
        self.connect_draft_host(host)?;
        let remote = self.remote_app(host)?;
        let _operation = remote.operation()?;
        let operation = remote.thread_lock(&format!("draft:{}", project["nativeId"]))?;
        let _guard = operation
            .lock()
            .map_err(|_| anyhow::anyhow!("新建会话锁已损坏"))?;
        let mut reader = RuntimeConnection::draft(&remote)?;
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
        match self.finish_remote_draft(&draft, &mut reader, &caller) {
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
        let projects = crate::remote::project_catalog(caller, || {
            let mut catalog = SidebarConnection::reader(caller, timeout)?;
            crate::remote::projects(&mut catalog)
        })?;
        let project = projects.iter()
            .find(|project| project["id"] == body["projectId"])
            .cloned().context("远程项目已移除，请刷新")?;
        ensure!(unique_project_root(&projects,&project),
            "此主机有多个项目使用同一目录，无法安全确定新建归属，请先在客户端区分项目目录");
        draft_params(&project)?;
        ensure!(
            body["hostId"].is_null() || body["hostId"] == project["hostId"],
            "新建目标主机不一致"
        );
        Ok(project)
    }

    fn connect_draft_host(&self, host: &str) -> Result<()> {
        let (descriptor, _) = self.remote_entry(host)?;
        if !descriptor.enabled && !self.remote_requested.lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?.contains(host) {
            self.connect_remote(&json!({"hostId":host}))?;
        }
        Ok(())
    }

    fn finish_remote_draft(
        &self,
        draft: &Draft<'_>,
        reader: &mut dyn Rpc,
        caller: &Config,
    ) -> Result<Value> {
        ensure!(
            draft.record["stage"] != "abandoned",
            "此前的新建请求已放弃，请重新新建"
        );
        let thread = draft.thread(&self.store, reader)?;
        let identifier = thread["id"].as_str().context("新建会话缺少 ID")?;
        if draft.record["stage"] != "complete" {
            move_draft_project(draft,reader,caller,identifier)?;
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

fn move_draft_project(draft: &Draft<'_>, reader: &mut dyn Rpc, caller: &Config,
    identifier: &str) -> Result<()> {
    let mut writer = SidebarConnection::project_writer(caller,
        Duration::from_secs(caller.rpc_timeout_seconds))?;
    let moved = writer.call("move_thread_to_project",json!({"threadId":identifier,
        "source":"codex","hostId":draft.remote.config.host_id,
        "projectId":draft.project["nativeId"]}))?;
    ensure!(moved["source"]=="codex" && moved["hostId"]==draft.remote.config.host_id
        && moved["threadId"]==identifier && moved["projectId"]==draft.project["nativeId"],
        "原生新建项目回执目标不一致");
    let current = reader.call("thread/read",json!({"threadId":identifier,
        "includeTurns":false}))?["thread"].clone();
    ensure!(current["id"]==identifier,"新建会话读回 ID 不一致");
    confirm_project(caller,&draft.project,&current)
}

impl Draft<'_> {
    fn thread(&self, store: &Store, reader: &mut dyn Rpc) -> Result<Value> {
        let thread = if self.fresh {
            self.start(store, reader)?
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
        empty_thread(&thread)?;
        Ok(thread)
    }

    fn start(&self, store: &Store, reader: &mut dyn Rpc) -> Result<Value> {
        let thread = reader.call("thread/start", draft_params(&self.project)?)?["thread"].clone();
        let identifier = thread["id"].as_str().context("新建回执缺少会话 ID")?;
        ensure!(uuid::Uuid::parse_str(identifier)?.to_string()==identifier,"新建会话 ID 无效");
        store.draft_update(&self.request, Some(identifier), "created")?;
        empty_thread(&thread)?;
        ensure!(thread["cwd"]==self.project["paths"][0],"新建会话目录不一致");
        ensure!(thread["historyMode"]=="legacy","空会话历史模式尚不支持安全读回");
        let name = thread["name"].as_str().filter(|name| !name.is_empty()).unwrap_or("新会话");
        reader.call("thread/name/set",json!({"threadId":identifier,"name":name}))?;
        let current = reader.call("thread/read",json!({
            "threadId":identifier,"includeTurns":true}))?["thread"].clone();
        ensure!(current["id"]==identifier && current["name"]==name,
            "初始标题尚未确认，请在客户端核对");
        Ok(current)
    }
}

fn empty_thread(thread: &Value) -> Result<()> {
    ensure!(thread["id"].is_string(), "会话读回缺少 ID");
    ensure!(thread["status"]["type"] != "active", "会话正在执行，请在客户端核对");
    ensure!(thread["turns"].as_array().is_some_and(Vec::is_empty),
        "会话执行记录未确认为空，请在客户端核对后继续");
    Ok(())
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
    Ok(json!({"cwd":paths[0],"runtimeWorkspaceRoots":paths,
        "ephemeral":false,"historyMode":"legacy"}))
}

fn unique_project_root(projects: &[Value], target: &Value) -> bool {
    projects.iter().filter(|project|project["hostId"]==target["hostId"]
        && project["paths"]==target["paths"]).count()==1
}

fn project_bound(state: &Value, project: &Value, thread: &Value, unique: bool) -> bool {
    let identifier = thread["id"].as_str().unwrap_or("");
    let indexed = &state["thread-project-membership-host-ids"][identifier];
    if !indexed.is_null() && *indexed!=project["hostId"] {return false;}
    let assignment = &state["thread-project-assignments"][identifier];
    if assignment.is_object() {
        return assignment["projectKind"]=="remote" && assignment["projectId"]==project["nativeId"]
            && (assignment["hostId"]==project["hostId"]
                || assignment["hostId"].is_null() && *indexed==project["hostId"]);
    }
    assignment.is_null() && unique && thread["cwd"]==project["paths"][0]
}

fn confirm_project(config: &Config, project: &Value, thread: &Value) -> Result<()> {
    ensure!(thread["cwd"]==project["paths"][0],"新建会话目录与目标项目不一致");
    let fresh = crate::remote::project_catalog(config, || {
        let mut reader = SidebarConnection::reader(config,
            Duration::from_secs(config.rpc_timeout_seconds))?;
        crate::remote::projects(&mut reader)
    })?;
    ensure!(fresh.iter().any(|current| current["id"]==project["id"]
        && current["paths"]==project["paths"]),"新建期间项目已改变，请在客户端核对");
    let unique = unique_project_root(&fresh,project);
    let deadline =
        std::time::Instant::now() + Duration::from_secs(config.rpc_timeout_seconds.min(4));
    loop {
        let state: Value = serde_json::from_slice(&std::fs::read(&config.sidebar_state_file)?)?;
        if project_bound(&state,project,thread,unique) {
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
