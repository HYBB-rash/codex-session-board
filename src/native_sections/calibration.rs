use super::{
    mapping::{self, Probe, Record},
    *,
};
use crate::store::Store;
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "calibration_tests.rs"]
mod tests;

pub(crate) struct Calibration<'source> {
    pub config: &'source Config,
    pub store: &'source Store,
    pub record: &'source mut Record,
    pub backend: &'source mut dyn Rpc,
    pub sidebar: &'source mut dyn Rpc,
}

impl Calibration<'_> {
    pub(crate) fn complete(&mut self, mut sections: HashMap<String, Value>)
        -> Result<HashMap<String, Value>> {
        let missing: Vec<_> = self
            .config
            .section_bindings()
            .filter(|(_, _, target)| !self.record.logical.values().any(|value| value == target))
            .map(|(_, label, target)| (label.to_owned(), target.to_owned()))
            .collect();
        if !missing.is_empty() {
            self.prepare()?;
            for (label, target) in missing {
                self.ensure_scope()?;
                let identifier = self.identifier()?.to_owned();
                self.empty()?;
                let receipt = self.sidebar.call("move_thread_to_sidebar_section", json!({
                    "threadId":identifier,"sectionId":target,"source":"codex","hostId":self.config.host_id
                }));
                let (backend, updated) = self
                    .confirm(&identifier, &target, &label)
                    .with_context(|| format!("校准位置未确认；移动回执：{receipt:?}"))?;
                ensure!(
                    self.record
                        .logical
                        .get(&backend)
                        .is_none_or(|value| value == &target),
                    "原生分区映射出现冲突"
                );
                self.record.logical.insert(backend, target);
                self.ensure_scope()?;
                self.save()?;
                sections = updated;
            }
        }
        self.cleanup()?;
        Ok(sections)
    }

    fn save(&self) -> Result<()> {
        self.store
            .save_native_mapping(&self.config.host_id, self.record)
    }

    fn identifier(&self) -> Result<&str> {
        self.record
            .probe
            .as_ref()
            .and_then(|probe| probe.thread.as_deref())
            .context("空会话创建回执未确认；已停止重复创建，请先在客户端核对")
    }

    fn prepare(&mut self) -> Result<()> {
        self.retire_missing()?;
        if self.record.probe.is_none() {
            self.record.probe = Some(Probe {
                request: uuid::Uuid::new_v4().to_string(),
                thread: None,
                prepared: false,
                initial_name: Value::Null,
                history_mode: Value::Null,
            });
            self.save()?;
            self.start()?;
        }
        let thread = self.empty()?;
        let probe = self.record.probe.as_ref().context("缺少校准记录")?;
        let name = format!("分区校准 · {}", probe.request);
        if probe.prepared {
            if thread["name"] != name {
                self.store
                    .release_native_probe(&self.config.host_id, self.record)?;
                anyhow::bail!("校准会话已被改名，已保留并停止操作此会话");
            }
            return Ok(());
        }
        if thread["name"] != name {
            self.ensure_scope()?;
            self.backend.call("thread/name/set",
                json!({"threadId":self.identifier()?,"name":name}))?;
        }
        let current = self.empty()?;
        ensure!(current["name"] == name, "校准会话初始标题尚未确认");
        self.record.probe.as_mut().context("缺少校准记录")?.prepared = true;
        self.save()
    }

    fn retire_missing(&mut self) -> Result<()> {
        let Some(identifier) = self.record.probe.as_ref().and_then(|probe| probe.thread.clone()) else {
            return Ok(());
        };
        if let Err(error) = self.empty() {
            if !missing(&error,&identifier) { return Err(error); }
            self.confirm_absent(&identifier)?;
            self.store.release_native_probe(&self.config.host_id,self.record)?;
        }
        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        self.ensure_scope()?;
        let home = self.record.scope["source"]["peer"]["home"]
            .as_str()
            .context("校准缺少已核验的远程目录")?;
        let response = self.backend.call(
            "thread/start",
            json!({
            "cwd":home,"ephemeral":false,"historyMode":"paginated"}),
        )?;
        let thread = &response["thread"];
        let identifier = thread["id"].as_str().context("空会话创建回执缺少 ID")?;
        ensure!(
            uuid::Uuid::parse_str(identifier)?.to_string() == identifier,
            "校准会话 ID 无效"
        );
        self.record.probe.as_mut().context("缺少创建记录")?.thread = Some(identifier.into());
        self.record.probe.as_mut().context("缺少创建记录")?.initial_name = thread["name"].clone();
        self.record.probe.as_mut().context("缺少创建记录")?.history_mode = thread["historyMode"].clone();
        self.save()?;
        if let Err(error) = self.verify_empty(thread) {
            self.store.release_native_probe(&self.config.host_id, self.record)?;
            return Err(error);
        }
        Ok(())
    }

    fn verify_empty(&self, thread: &Value) -> Result<()> {
        ensure!(thread["id"] == self.identifier()?, "校准会话 ID 不匹配");
        ensure!(
            thread["turns"].as_array().is_some_and(Vec::is_empty)
                && matches!(
                    thread["status"]["type"].as_str(),
                    Some("idle" | "notLoaded")
                ),
            "校准会话已被使用或执行状态不明确"
        );
        let mode = &self.record.probe.as_ref().context("缺少校准记录")?.history_mode;
        let expected_mode = if mode.is_null() { "legacy" } else { mode.as_str().unwrap_or("") };
        ensure!(
            thread["cwd"] == self.record.scope["source"]["peer"]["home"]
                && matches!(expected_mode,"legacy" | "paginated")
                && thread["historyMode"] == expected_mode,
            "校准会话目录或历史模式已改变"
        );
        let probe = self.record.probe.as_ref().context("缺少校准记录")?;
        let expected = format!("分区校准 · {}", probe.request);
        ensure!(thread["name"] == expected
            || !probe.prepared && thread["name"] == probe.initial_name,
            "校准会话已被改名接手");
        if let Some(binding) = &self.config.remote_binding {
            ensure!(binding.mapping_identity()? == self.record.scope["source"], "校准来源已改变");
        }
        Ok(())
    }

    fn ensure_scope(&self) -> Result<()> {
        let mut current = mapping::seed(self.config)?.scope;
        current["source"] = self.record.scope["source"].clone();
        ensure!(current == self.record.scope, "校准过程中账号或分区配置已改变");
        Ok(())
    }

    fn empty(&mut self) -> Result<Value> {
        let identifier = self.identifier()?.to_owned();
        let response = self.backend.call(
            "thread/read",
            json!({"threadId":identifier,"includeTurns":true}),
        ).or_else(|error| {
            if unmaterialized(&error,&identifier) {
                self.backend.call("thread/read",json!({"threadId":identifier,"includeTurns":false}))
            } else { Err(error) }
        })?;
        let thread = response["thread"].clone();
        if let Err(error) = self.verify_empty(&thread) {
            self.store
                .release_native_probe(&self.config.host_id, self.record)?;
            return Err(error.context("已保留并停止操作此会话"));
        }
        Ok(thread)
    }

    fn confirm(
        &mut self,
        identifier: &str,
        target: &str,
        label: &str,
    ) -> Result<(String, HashMap<String, Value>)> {
        let deadline = Instant::now() + Duration::from_secs(self.config.rpc_timeout_seconds.min(4));
        loop {
            let result = self.confirm_once(identifier, target, label);
            if result.is_ok() || self.record.probe.is_none() || Instant::now() >= deadline {
                return result;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn confirm_once(
        &mut self,
        identifier: &str,
        target: &str,
        label: &str,
    ) -> Result<(String, HashMap<String, Value>)> {
        let thread = self.empty()?;
        ensure!(thread["id"] == identifier, "校准读回的会话 ID 不匹配");
        let backend = thread["section"]["id"]
            .as_str()
            .context("后台位置尚未确认")?;
        let sections = super::direct::sections(self.backend, self.config)?;
        ensure!(
            sections
                .get(backend)
                .is_some_and(|section| section == &thread["section"] && section["name"] == label),
            "后台分区身份尚未确认"
        );
        let live = mapping::live_catalog(self.sidebar, self.config)?;
        mapping::confirm_membership(&live, target, identifier)?;
        let after = self.empty()?;
        ensure!(after["section"] == thread["section"], "校准会话的位置在读取期间改变");
        self.ensure_scope()?;
        Ok((backend.into(), sections))
    }

    pub(crate) fn cleanup(&mut self) -> Result<()> {
        if self.record.probe.is_none() {
            return Ok(());
        }
        let identifier = self.identifier()?.to_owned();
        if let Err(error) = self.empty() {
            if missing(&error,&identifier) {
                self.confirm_absent(&identifier)?;
                return self.store.release_native_probe(&self.config.host_id,self.record);
            }
            return Err(error);
        }
        let receipt = self
            .backend
            .call("thread/archive", json!({"threadId":identifier}));
        self.confirm_archived(&identifier)
            .with_context(|| format!("校准会话归档未确认，稍后继续清理；回执：{receipt:?}"))?;
        self.record.probe = None;
        self.save()
    }

    fn confirm_absent(&mut self, identifier: &str) -> Result<()> {
        for archived in [false,true] {
            let (threads,complete) = crate::rpc::collect_threads(self.backend,self.config.page_size,
                self.config.max_pages,archived)?;
            ensure!(complete && !threads.iter().any(|thread| thread["id"] == identifier),
                "专用空会话仍存在，保留清理记录");
        }
        match self.backend.call("thread/read",json!({"threadId":identifier,"includeTurns":false})) {
            Err(error) if missing(&error,identifier) => Ok(()),
            _ => anyhow::bail!("专用空会话的缺失尚未独立确认"),
        }
    }

    fn confirm_archived(&mut self, identifier: &str) -> Result<()> {
        let mut cursor = Value::Null;
        let mut seen = HashSet::new();
        for _ in 0..self.config.max_pages {
            let mut params = crate::rpc::thread_list_params(self.config.page_size, cursor);
            params["archived"] = json!(true);
            let page = self.backend.call("thread/list", params)?;
            if page["data"]
                .as_array()
                .context("归档会话列表无效")?
                .iter()
                .any(|thread| thread["id"] == identifier)
            {
                return Ok(());
            }
            cursor = page["nextCursor"].clone();
            if cursor.is_null() || cursor == "" {
                break;
            }
            ensure!(
                seen.insert(cursor.as_str().context("归档游标无效")?.to_owned()),
                "归档分页游标重复"
            );
        }
        anyhow::bail!("专用校准会话尚未在归档列表确认")
    }
}

fn unmaterialized(error: &anyhow::Error, identifier: &str) -> bool {
    error.downcast_ref::<crate::rpc::RequestError>().is_some_and(|error|
        error.method == "thread/read" && error.details["code"] == -32600
            && error.details["message"] == format!("thread {identifier} is not materialized yet; includeTurns is unavailable before first user message"))
}

fn missing(error: &anyhow::Error, identifier: &str) -> bool {
    error.downcast_ref::<crate::rpc::RequestError>().is_some_and(|error|
        error.method == "thread/read" && error.details["code"] == -32600
            && error.details["message"] == format!("thread not loaded: {identifier}"))
}
