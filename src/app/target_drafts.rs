use super::*;
use crate::catalog::preparation::{Fresh, roots};
use crate::rpc::Rpc;

struct TargetRequest<'a> {
    key: &'a str,
    operation: &'a str,
    host: String,
    origin: String,
    identifier: String,
}

impl<'a> TargetRequest<'a> {
    fn parse(body: &'a Value) -> Result<Self> {
        let key = body["projectId"].as_str().context("缺少目标项目 ID")?;
        let (host, origin, identifier) = target_identity(key)?;
        ensure!(
            body["hostId"].is_null() || body["hostId"] == host,
            "新建目标主机不一致"
        );
        let operation = body["operationId"].as_str().context("缺少新建操作 ID")?;
        ensure!(
            uuid::Uuid::parse_str(operation)?.to_string() == operation,
            "新建操作 ID 无效"
        );
        Ok(Self {
            key,
            operation,
            host,
            origin,
            identifier,
        })
    }
}

impl App {
    pub(crate) fn prepare_target_native_draft(&self, body: &Value) -> Result<Value> {
        ensure!(!self.config.legacy_mode, "六状态兼容模式不支持目标项目新建");
        let request = TargetRequest::parse(body)?;
        let operation = request.operation;
        crate::native_new::target_capability(&self.config)?;
        let operation_lock = self.thread_lock(&format!("native-target:{operation}"))?;
        let _guard = operation_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("新建操作锁已损坏"))?;
        let record = self.store.native_draft_begin(request.key, operation)?;
        let remote = self.target_draft_host(&request.host)?;
        let _active = remote.operation()?;
        let mut reader = remote.connection(
            Duration::from_secs(remote.config.rpc_timeout_seconds),
            Role::TargetProjectPreparer,
        )?;
        let mut source = reader.catalog_source()?;
        source["hostId"] = json!(request.host);
        let fresh = Fresh::read(&mut reader, &source)?;
        validate_catalog_source(&remote, &fresh.source)?;
        self.store.native_draft_bound(operation, &fresh.source)?;
        let target = prepare_target(
            &mut reader,
            &fresh,
            &source,
            &record,
            &request,
            remote.config.max_pages,
        )?;
        self.store.native_draft_prepared(operation, &target)?;
        self.open_target_draft(&request, &target)
    }

    fn target_draft_host(&self, host: &str) -> Result<Arc<App>> {
        let (descriptor, _) = self.remote_entry(host)?;
        if !descriptor.enabled
            && !self
                .remote_requested
                .lock()
                .map_err(|_| anyhow::anyhow!("连接请求锁已损坏"))?
                .contains(host)
        {
            self.connect_remote(&json!({"hostId":host}))?;
        }
        self.remote_app(host)
    }

    fn open_target_draft(&self, request: &TargetRequest<'_>, target: &Value) -> Result<Value> {
        let operation = request.operation;
        let canonical = target["projectId"].as_str().context("规范项目缺少 ID")?;
        let url = crate::native_new::target_url(&self.config, &request.host, canonical, operation)?;
        match open_client_url(url, &self.config) {
            Ok(url) => {
                self.store.native_draft_opened(operation)?;
                Ok(
                    json!({"ok":true,"native":true,"url":url,"operationId":operation,"target":target}),
                )
            }
            Err(error) => Ok(json!({"ok":false,"pending":true,"operationId":operation,
                "error":error.to_string()})),
        }
    }
}

fn validate_catalog_source(remote: &App, source: &Value) -> Result<()> {
    let catalog = remote
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("目录锁已损坏"))?;
    let snapshot = catalog
        .snapshot
        .as_ref()
        .context("目标目录尚未就绪，请刷新")?;
    for field in [
        "hostId",
        "codexHome",
        "peerFingerprint",
        "accountFingerprint",
    ] {
        ensure!(
            snapshot.source[field] == source[field],
            "目标目录来源或账户已改变，请刷新后重新新建"
        );
    }
    Ok(())
}

fn prepare_target(
    reader: &mut dyn Rpc,
    fresh: &Fresh,
    source: &Value,
    record: &Value,
    request: &TargetRequest<'_>,
    pages: usize,
) -> Result<Value> {
    let legacy = (request.origin == "targetLegacy").then_some(request.identifier.as_str());
    let project = if let Some(canonical) = record["target"]["projectId"].as_str() {
        fresh.server(reader, canonical)?
    } else if legacy.is_some() {
        fresh.prepare_legacy(reader, &request.identifier, pages)?
    } else {
        fresh.server(reader, &request.identifier)?
    };
    fresh.verify_after(reader, source, legacy, &project)?;
    let canonical = project["id"].as_str().context("规范项目缺少 ID")?;
    let roots = roots(&project)?;
    let cwd = roots.first().context("项目没有根目录")?["path"].clone();
    Ok(
        json!({"hostId":request.host,"projectId":canonical,"roots":roots,
        "cwd":cwd,"source":fresh.source}),
    )
}

fn target_identity(key: &str) -> Result<(String, String, String)> {
    let fields: Vec<String> =
        serde_json::from_str(key.strip_prefix("target:").context("目标项目 ID 无效")?)?;
    ensure!(
        fields.len() == 3
            && !fields[0].trim().is_empty()
            && fields[0] != "local"
            && matches!(fields[1].as_str(), "targetServer" | "targetLegacy")
            && !fields[2].trim().is_empty(),
        "目标项目 ID 无效"
    );
    Ok((fields[0].clone(), fields[1].clone(), fields[2].clone()))
}
