use super::*;
use crate::catalog::preparation::{Fresh, roots};

impl App {
    pub(crate) fn prepare_target_native_draft(&self, body: &Value) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "六状态兼容模式不支持目标项目新建");
        let key = body["projectId"].as_str().context("缺少目标项目 ID")?;
        let (host, origin, identifier) = target_identity(key)?;
        ensure!(body["hostId"].is_null() || body["hostId"] == host, "新建目标主机不一致");
        let operation = body["operationId"].as_str().context("缺少新建操作 ID")?;
        ensure!(uuid::Uuid::parse_str(operation)?.to_string() == operation, "新建操作 ID 无效");
        crate::native_new::target_capability(&self.config)?;
        let operation_lock = self.thread_lock(&format!("native-target:{operation}"))?;
        let _guard = operation_lock.lock().map_err(|_| anyhow::anyhow!("新建操作锁已损坏"))?;
        let record = self.store.native_draft_begin(key, operation)?;
        let (descriptor, _) = self.remote_entry(&host)?;
        if !descriptor.enabled && !self.remote_requested.lock()
            .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?.contains(&host) {
            self.connect_remote(&json!({"hostId":host}))?;
        }
        let remote = self.remote_app(&host)?;
        let _active = remote.operation()?;
        let mut reader = remote.connection(Duration::from_secs(remote.config.rpc_timeout_seconds),
            Role::TargetProjectPreparer)?;
        let mut source = reader.catalog_source()?;
        source["hostId"] = json!(host);
        let fresh = Fresh::read(&mut reader, &source)?;
        {
            let catalog = remote.catalog.lock().map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
            let snapshot = catalog.snapshot.as_ref().context("目标目录尚未就绪，请刷新")?;
            for field in ["hostId","codexHome","peerFingerprint","accountFingerprint"] {
                ensure!(snapshot.source[field] == fresh.source[field],
                    "目标目录来源或账户已改变，请刷新后重新新建");
            }
        }
        self.store.native_draft_bound(operation, &fresh.source)?;
        let legacy = (origin == "targetLegacy").then_some(identifier.as_str());
        let project = if let Some(canonical) = record["target"]["projectId"].as_str() {
            fresh.server(&mut reader, canonical)?
        } else if legacy.is_some() {
            fresh.prepare_legacy(&mut reader, &identifier, remote.config.max_pages)?
        } else {
            fresh.server(&mut reader, &identifier)?
        };
        fresh.verify_after(&mut reader, &source, legacy, &project)?;
        let canonical = project["id"].as_str().context("规范项目缺少 ID")?;
        let roots = roots(&project)?;
        let cwd = roots.first().context("项目没有根目录")?["path"].clone();
        let target = json!({"hostId":host,"projectId":canonical,"roots":roots,
            "cwd":cwd,"source":fresh.source});
        self.store.native_draft_prepared(operation, &target)?;
        let url = crate::native_new::target_url(&self.config, &host, canonical, operation)?;
        match open_client_url(url, &self.config) {
            Ok(url) => {
                self.store.native_draft_opened(operation)?;
                Ok(json!({"ok":true,"native":true,"url":url,"operationId":operation,"target":target}))
            }
            Err(error) => Ok(json!({"ok":false,"pending":true,"operationId":operation,
                "error":error.to_string()})),
        }
    }
}

fn target_identity(key: &str) -> Result<(String, String, String)> {
    let fields: Vec<String> = serde_json::from_str(key.strip_prefix("target:")
        .context("目标项目 ID 无效")?)?;
    ensure!(fields.len() == 3 && !fields[0].trim().is_empty() && fields[0] != "local"
        && matches!(fields[1].as_str(), "targetServer" | "targetLegacy")
        && !fields[2].trim().is_empty(), "目标项目 ID 无效");
    Ok((fields[0].clone(), fields[1].clone(), fields[2].clone()))
}
