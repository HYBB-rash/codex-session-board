use super::*;
use crate::native_sections::Observation;

impl App {
    pub(crate) fn native_runtime_since(&self, identifier: &str, since: f64) -> Result<f64> {
        if self.imports_native_placement() {
            self.store.native_backfill_since(identifier,since)
        } else { Ok(since) }
    }

    pub(crate) fn baseline_native_backfill(&self, identifier: &str, status: &Value,
        turn: &Value, since: f64) -> Result<bool> {
        Ok(self.imports_native_placement()
            && self.store.baseline_native_backfill(identifier,status,turn,since)?)
    }

    pub(crate) fn needs_native_backfill(&self, identifier: &str) -> Result<bool> {
        Ok(self.imports_native_placement() && self.store.native_backfill_pending(identifier)?)
    }

    fn imports_native_placement(&self) -> bool {
        !self.config.four_columns && self.config.native_sections && self.config.host_id != "local"
    }

    pub(super) fn native_backfill_scope(&self) -> Result<Value> {
        let checked = self.native_mapping_scope.lock()
            .map_err(|_| anyhow::anyhow!("分区校验锁已损坏"))?
            .clone().context("远程分区映射尚未校验")?;
        let saved = self.mapping_store.native_mapping(&self.config.host_id)?
            .context("远程分区映射尚未保存")?;
        ensure!(saved.scope == checked, "远程分区映射来源已改变");
        Ok(checked)
    }

    pub(super) fn cache_native_observation(&self, identifier: &str,
        observed: &Observation) -> Result<()> {
        if self.is_native_probe(identifier)? { return Ok(()); }
        if self.needs_native_backfill(identifier)?
            && self.store.cache_native_backfill(identifier, observed,
                &self.native_backfill_scope()?)? {
            return Ok(());
        }
        self.store.cache_native(identifier, observed)
    }

    pub(crate) fn backfill_native(&self, identifier: &str, status: &Value) -> Result<()> {
        if !self.needs_native_backfill(identifier)? { return Ok(()); }
        let operation = self.thread_lock(identifier)?;
        let _thread = operation.lock().map_err(|_| anyhow::anyhow!("会话操作锁已损坏"))?;
        let mut reader = self.native_reader()?;
        let catalog = self.native_catalog(&mut reader)?;
        let observed = catalog.read(&mut reader, &self.config, identifier)?;
        self.cache_native_observation(identifier, &observed)?;
        let scope = self.native_backfill_scope()?;
        let Some(change) = self.store.prepare_native_backfill(identifier,
            &self.config, status, &scope)? else { return Ok(()); };
        self.store.validate_native_backfill(identifier, &change)?;
        let confirmed = self.move_native_scoped(identifier, &observed.version,
            Some(&change.state), Some(&scope))?;
        self.store.finish_native_backfill(identifier, &change, &confirmed)
    }
}
