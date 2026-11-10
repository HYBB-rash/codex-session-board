use super::*;
use crate::{native_sections, rpc::Rpc};

struct Mutation<'app>(&'app App);

impl Drop for Mutation<'_> {
    fn drop(&mut self) {
        self.0.native_generation.fetch_add(1, Ordering::SeqCst);
    }
}

impl App {
    pub(crate) fn native_epoch(&self) -> usize {
        self.native_generation.load(Ordering::SeqCst)
    }

    pub(crate) fn cache_native_page(
        &self,
        generation: usize,
        reader: &mut dyn Rpc,
        threads: &[Value],
    ) -> Result<()> {
        let catalog = native_sections::direct::Catalog::load(reader, &self.config)?;
        let _operation = self.operation()?;
        for thread in threads {
            let identifier = thread["id"].as_str().context("会话缺少 ID")?;
            let operation = self.thread_lock(identifier)?;
            let Ok(_guard) = operation.try_lock() else {continue;};
            ensure!(self.enabled.load(Ordering::SeqCst) && generation == self.native_epoch(),
                "原生位置观察已过期，请重新读取");
            self.store.cache_native(identifier, &catalog.observe(&self.config,thread)?)?;
        }
        self.update_health(json!({"nativeVerified":true,"nativeError":null,"nativeLastAt":now()}))
    }

    pub(crate) fn native_reader(&self) -> Result<DesktopConnection> {
        self.connection(
            Duration::from_secs(self.config.rpc_timeout_seconds),
            Role::NativeReader,
        )
    }

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
        let mut reader = self.native_reader()?;
        let mut caller = self.config.clone();
        caller.host_id = "local".into();
        let mut writer = SidebarConnection::connect(
            &caller,
            Duration::from_secs(self.config.rpc_timeout_seconds),
        )?;
        let result = native_sections::direct::move_checked(
            &mut reader,
            &mut writer,
            &self.config,
            identifier,
            expected,
            state,
        );
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
        let target = self.config.section_target(state)?;
        let mut reader = self.native_reader()?;
        native_sections::direct::confirm_move(&mut reader, &self.config, identifier, target)
    }

    pub fn refresh_native(&self) -> Result<()> {
        let mut reader = self.native_reader()?;
        self.read_native(&mut reader)
    }

    pub fn read_native(&self, reader: &mut impl Rpc) -> Result<()> {
        let generation = self.native_generation.load(Ordering::SeqCst);
        let snapshot = native_sections::direct::snapshot(reader, &self.config)?;
        let _operation = self.operation()?;
        ensure!(self.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
        let Some(unresolved) = self.import_native(generation, reader, &snapshot)? else {
            return Ok(());
        };
        self.update_health(json!({"nativeVerified":true,"nativeError":if unresolved==0 {Value::Null}else{json!(format!("{unresolved} 个会话的原生位置未读回，保留上次位置"))},"nativeLastAt":now()}))
    }

    fn import_native(
        &self,
        generation: usize,
        reader: &mut impl Rpc,
        snapshot: &std::collections::HashMap<String, native_sections::Observation>,
    ) -> Result<Option<usize>> {
        let archived = self.archived_native_ids(reader, snapshot)?;
        let mut unresolved = 0;
        for session in self.store.sessions()? {
            let identifier = session["id"].as_str().context("会话缺少 ID")?;
            let operation = self.thread_lock(identifier)?;
            let Ok(_guard) = operation.try_lock() else {
                continue;
            };
            if generation != self.native_generation.load(Ordering::SeqCst) {
                return Ok(None);
            }
            if let Some(observed) = snapshot.get(identifier) {
                self.store.cache_native(identifier, observed)?;
            } else if !archived.contains(identifier) {
                unresolved += 1;
            }
        }
        self.restore_visible(snapshot)?;
        Ok(Some(unresolved))
    }

    fn archived_native_ids(
        &self,
        reader: &mut impl Rpc,
        snapshot: &std::collections::HashMap<String, native_sections::Observation>,
    ) -> Result<std::collections::HashSet<String>> {
        let missing = self.store.sessions()?.into_iter().any(|session| {
            session["id"]
                .as_str()
                .is_some_and(|identifier| !snapshot.contains_key(identifier))
        });
        if !missing {
            return Ok(std::collections::HashSet::new());
        }
        native_sections::direct::archived_ids(reader, &self.config)
    }

    fn restore_visible(
        &self,
        snapshot: &std::collections::HashMap<String, native_sections::Observation>,
    ) -> Result<()> {
        for identifier in self.store.archived_ids()? {
            let Some(observed) = snapshot.get(&identifier) else {
                continue;
            };
            let operation = self.thread_lock(&identifier)?;
            let Ok(_guard) = operation.try_lock() else {
                continue;
            };
            self.store.restore_native(&identifier, observed)?;
        }
        Ok(())
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
        if self.store.has_pending_native_runtime(identifier)? {
            self.check_native_title(identifier)?;
        }
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
            .section_bindings()
            .any(|(_, _, identifier)| identifier == change.section_version);
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
        let mut reader = self.native_reader()?;
        let observed = native_sections::direct::read(&mut reader, &self.config, identifier)?;
        self.store.cache_native(identifier, &observed)
    }
}
