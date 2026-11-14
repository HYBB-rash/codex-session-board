use super::*;
use crate::{native_sections::{calibration::Calibration, direct::Catalog, mapping}, rpc::Rpc};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::time::Instant;

struct CalibrationSidebar<'app>(&'app App);
struct CalibrationBackend<'app> {
    app: &'app App,
    connection: Option<crate::rpc::DesktopConnection>,
}

impl Rpc for CalibrationSidebar<'_> {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.0.native_sidebar()?.call(method,params)
            .with_context(|| format!("分区校准客户端操作失败：{method}"))
    }
}

impl Rpc for CalibrationBackend<'_> {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        if self.connection.is_none() {
            self.connection = Some(self.app.connection(self.app.native_timeout(),Role::CalibrationWriter)?);
        }
        let result = self.connection.as_mut().context("校准连接未建立")?.call(method,params);
        if result.as_ref().is_err_and(|error| error.downcast_ref::<crate::rpc::RequestError>().is_none()) {
            self.connection = None;
        }
        result.with_context(|| format!("分区校准后端操作失败：{method}"))
    }
}

impl App {
    pub(crate) fn cleanup_native_probe(&self, identifier: &str) -> Result<()> {
        ensure!(self.config.host_id != "local","清理仅用于指定远程来源");
        let _checked = self.native_mapping_scope.lock()
            .map_err(|_| anyhow::anyhow!("分区校验锁已损坏"))?;
        let _lock = self.native_mapping_lock()?;
        let seed = self.mapping_seed()?;
        let mut record = self.mapping_record(&seed)?;
        ensure!(record.probe.as_ref().and_then(|probe| probe.thread.as_deref()) == Some(identifier),
            "只能清理本来源已记录的专用空会话");
        let mut backend = CalibrationBackend {app:self,connection:None};
        let mut sidebar = CalibrationSidebar(self);
        Calibration {config:&self.config,store:&self.mapping_store,record:&mut record,
            backend:&mut backend,sidebar:&mut sidebar}.cleanup()
    }

    pub(crate) fn native_catalog(&self, reader: &mut (impl Rpc + ?Sized)) -> Result<Catalog> {
        if self.config.host_id == "local" {
            return Catalog::load(reader, &self.config);
        }
        let mut checked = self.native_mapping_scope.lock()
            .map_err(|_| anyhow::anyhow!("分区校验锁已损坏"))?;
        let _lock = self.native_mapping_lock()?;
        let seed = self.mapping_seed()?;
        let mut record = self.mapping_record(&seed)?;
        let mut sections = crate::native_sections::direct::sections(reader, &self.config)?;
        record.logical = mapping::usable(&self.config, &sections, record.logical);
        if checked.as_ref() == Some(&record.scope) && record.probe.is_none()
            && mapping::complete(&self.config, &record) {
            return Catalog::mapped(sections, record.logical, &self.config);
        }
        let mut sidebar = CalibrationSidebar(self);
        mapping::live_catalog(&mut sidebar, &self.config)?;
        if checked.as_ref() != Some(&record.scope) {
            record.logical = mapping::merge_saved(record.logical,
                mapping::usable(&self.config, &sections, seed.logical));
        }
        if !mapping::complete(&self.config, &record) || record.probe.is_some() {
            let mut backend = CalibrationBackend {app:self,connection:None};
            sections = Calibration {config:&self.config,store:&self.mapping_store,
                record:&mut record,backend:&mut backend,sidebar:&mut sidebar}.complete(sections)?;
        }
        let catalog = Catalog::mapped(sections, record.logical.clone(), &self.config)?;
        self.mapping_store.save_native_mapping(&self.config.host_id, &record)?;
        *checked = Some(record.scope);
        Ok(catalog)
    }

    fn mapping_record(&self, seed: &mapping::Seed) -> Result<mapping::Record> {
        if let Some(record) = self.mapping_store.native_mapping(&self.config.host_id)? {
            if record.scope == seed.scope { return Ok(record); }
            ensure!(record.probe.is_none(),
                "分区校准来源或配置已改变；需先在原来源完成专用空会话清理");
        }
        Ok(mapping::Record {scope:seed.scope.clone(),logical:seed.logical.clone(),probe:None})
    }

    fn mapping_seed(&self) -> Result<mapping::Seed> {
        let source = self.config.remote_binding.as_ref()
            .context("远程分区缺少已核验的来源")?.mapping_identity()?;
        let seed = mapping::seed(&self.config).map(|mut seed| {
            seed.scope["source"] = source.clone();
            seed
        });
        if let Some(mut previous) = self.mapping_store.native_mapping(&self.config.host_id)?
            && previous.probe.is_some() {
            ensure!(previous.scope["source"] == source,
                "校准清理仍属于原来的远程来源，不能在新来源执行");
            if !seed.as_ref().is_ok_and(|seed| seed.scope == previous.scope) {
                let mut backend = CalibrationBackend {app:self,connection:None};
                let mut sidebar = CalibrationSidebar(self);
                Calibration {config:&self.config,store:&self.mapping_store,record:&mut previous,
                    backend:&mut backend,sidebar:&mut sidebar}.cleanup()?;
            }
        }
        seed
    }

    pub(crate) fn reset_native_calibration(&self, body: &Value) -> Result<Value> {
        ensure!(body["confirmed"] == true, "请先在客户端核对可能遗留的空会话");
        let host = body["hostId"].as_str().context("缺少主机 ID")?;
        let request = body["requestId"].as_str().context("缺少校准请求 ID")?;
        ensure!(uuid::Uuid::parse_str(request)?.to_string() == request, "校准请求 ID 无效");
        let remote = self.remote_app(host)?;
        let _operation = remote.operation()?;
        let mut checked = remote.native_mapping_scope.lock()
            .map_err(|_| anyhow::anyhow!("分区校验锁已损坏"))?;
        let _lock = remote.native_mapping_lock()?;
        let source = remote.config.remote_binding.as_ref()
            .context("远程来源尚未登记")?.mapping_identity()?;
        remote.mapping_store.abandon_unknown_native_probe(host, request, &source)?;
        *checked = None;
        remote.refresh.store(true, Ordering::Relaxed);
        Ok(json!({"ok":true}))
    }

    pub(crate) fn native_timeout(&self) -> Duration {
        Duration::from_secs(self.config.rpc_timeout_seconds)
    }

    pub(crate) fn native_sidebar(&self) -> Result<SidebarConnection> {
        let mut caller = self.config.clone();
        caller.host_id = "local".into();
        SidebarConnection::connect(&caller, self.native_timeout())
    }

    pub(crate) fn invalidate_native_mapping(&self) -> Result<()> {
        *self.native_mapping_scope.lock()
            .map_err(|_| anyhow::anyhow!("分区校验锁已损坏"))? = None;
        Ok(())
    }

    fn native_mapping_lock(&self) -> Result<File> {
        use fs2::FileExt;
        let path = self.config.data_dir.join("native-mapping.lock");
        ensure!(!std::fs::symlink_metadata(&path)
            .is_ok_and(|entry| entry.file_type().is_symlink()), "分区校准锁不能是符号链接");
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .mode(0o600).open(path)?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file() && metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o077 == 0, "分区校准锁所有者或权限无效");
        let deadline = Instant::now() + self.native_timeout();
        loop {
            ensure!(self.enabled.load(Ordering::SeqCst), "主机连接登记已失效");
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let remaining = deadline.checked_duration_since(Instant::now())
                        .context("等待分区校准超时")?;
                    std::thread::sleep(remaining.min(Duration::from_millis(2)));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}
