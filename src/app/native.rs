use super::*;
use crate::{native_sections, rpc::Rpc};

struct Mutation<'app>(&'app App);

impl Drop for Mutation<'_> {
    fn drop(&mut self) {
        self.0.native_generation.fetch_add(1, Ordering::SeqCst);
    }
}

impl App {
    pub(crate) fn thread_lock(&self, identifier: &str) -> Result<Arc<Mutex<()>>> {
        Ok(self
            .thread_operations
            .lock()
            .map_err(|_| anyhow::anyhow!("会话操作锁已损坏"))?
            .entry(identifier.into())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone())
    }

    fn move_native(
        &self,
        identifier: &str,
        expected: &str,
        state: Option<&str>,
    ) -> Result<native_sections::Observation> {
        self.native_generation.fetch_add(1, Ordering::SeqCst);
        let _mutation = Mutation(self);
        let mut writer = SidebarConnection::connect(
            &self.config,
            Duration::from_secs(self.config.rpc_timeout_seconds.min(3)),
        )?;
        let result =
            native_sections::move_checked(&mut writer, &self.config, identifier, expected, state);
        if let Err(error) = &result
            && error
                .downcast_ref::<native_sections::UnconfirmedMove>()
                .is_some()
        {
            return self
                .confirm_native_move(identifier, state)
                .context("原生移动结果未确认，请刷新核对");
        }
        result
    }

    fn confirm_native_move(
        &self,
        identifier: &str,
        state: Option<&str>,
    ) -> Result<native_sections::Observation> {
        let state = state.context("缺少目标原生分区")?;
        let target = self
            .config
            .status_sections
            .get(board_state(state))
            .context("目标原生分区未配置")?;
        let mut reader = SidebarConnection::reader(
            &self.config,
            Duration::from_secs(self.config.rpc_timeout_seconds.min(3)),
        )?;
        let observed = native_sections::read(&mut reader, &self.config, identifier)?;
        ensure!(observed.version == *target, "原生位置与目标不一致");
        Ok(observed)
    }

    pub fn refresh_native(&self) -> Result<()> {
        let mut reader = SidebarConnection::reader(
            &self.config,
            Duration::from_secs(self.config.rpc_timeout_seconds),
        )?;
        self.read_native(&mut reader)
    }

    pub fn read_native(&self, reader: &mut impl Rpc) -> Result<()> {
        let generation = self.native_generation.load(Ordering::SeqCst);
        let mut snapshot = native_sections::snapshot(reader, &self.config)?;
        let unassigned = native_sections::observe(&self.config, &Value::Null)?;
        for session in self.store.sessions()? {
            let identifier = session["id"].as_str().context("会话缺少 ID")?;
            let operation = self.thread_lock(identifier)?;
            let Ok(_guard) = operation.try_lock() else {
                continue;
            };
            if generation != self.native_generation.load(Ordering::SeqCst) {
                return Ok(());
            }
            let observed = snapshot
                .remove(identifier)
                .unwrap_or_else(|| unassigned.clone());
            self.store.cache_native(identifier, &observed)?;
        }
        self.update_health(json!({"nativeVerified":true,"nativeError":null,"nativeLastAt":now()}))
    }

    pub(crate) fn save_native(
        &self,
        identifier: &str,
        state: Option<&str>,
        note: &str,
        revision: (i64, i64),
        expected: &str,
    ) -> Result<()> {
        let operation = self.thread_lock(identifier)?;
        let _guard = operation
            .lock()
            .map_err(|_| anyhow::anyhow!("会话操作锁已损坏"))?;
        ensure!(
            self.is_active(identifier)?,
            "等待完整原生会话发现，请刷新后重试"
        );
        self.store
            .validate_state(identifier, state, note, revision)?;
        let confirmed = self.move_native(identifier, expected, state)?;
        self.store
            .set_native_state(identifier, state, note, revision, &confirmed)
            .context("原生操作后本地确认未完成，请刷新核对")
    }

    pub(crate) fn observe_native_runtime(
        &self,
        identifier: &str,
        status: &Value,
        turn: &Value,
        since: f64,
    ) -> Result<()> {
        let operation = self.thread_lock(identifier)?;
        let guard = operation
            .lock()
            .map_err(|_| anyhow::anyhow!("会话操作锁已损坏"))?;
        let Some(change) = self
            .store
            .prepare_native_runtime(identifier, status, turn, since)?
        else {
            return Ok(());
        };
        let default =
            ["unsectioned", "chats", "threads"].contains(&change.section_version.as_str());
        let managed = self
            .config
            .status_sections
            .values()
            .any(|identifier| *identifier == change.section_version);
        if !default && !managed {
            return self.store.consume_native_runtime(identifier, &change);
        }
        let confirmed =
            self.move_native(identifier, &change.section_version, Some(change.state))?;
        self.store
            .commit_native_runtime(identifier, &change, &confirmed)?;
        drop(guard);
        self.sync_title(identifier)
    }

    pub(crate) fn check_native_title(&self, identifier: &str) -> Result<()> {
        let mut reader = SidebarConnection::reader(
            &self.config,
            Duration::from_secs(self.config.rpc_timeout_seconds.min(3)),
        )?;
        let observed = native_sections::read(&mut reader, &self.config, identifier)?;
        self.store.cache_native(identifier, &observed)
    }
}
