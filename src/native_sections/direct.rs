use super::*;

pub struct Catalog {
    sections: HashMap<String, Value>,
    logical: HashMap<String, String>,
}

fn sections(reader: &mut (impl Rpc + ?Sized), config: &Config) -> Result<HashMap<String, Value>> {
    let mut sections = HashMap::new();
    let mut cursors = HashSet::new();
    let mut cursor = Value::Null;
    let mut complete = false;
    for _ in 0..config.max_pages {
        let page = reader.call(
            "threadSection/list",
            json!({"limit":config.page_size.min(100),"cursor":cursor}),
        )?;
        for section in page["data"].as_array().context("原生分区目录无效")? {
            let identifier = section["id"].as_str().context("原生分区缺少 ID")?;
            ensure!(
                sections
                    .insert(identifier.into(), section.clone())
                    .is_none(),
                "原生分区 ID 重复"
            );
        }
        cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() || cursor == "" {
            complete = true;
            break;
        }
        let next = cursor.as_str().context("分区分页游标无效")?;
        ensure!(cursors.insert(next.to_string()), "分区分页游标重复");
    }
    ensure!(complete, "原生分区目录未完整读取");
    Ok(sections)
}

fn logical(config: &Config) -> Result<HashMap<String, String>> {
    let state: Value = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(
        &config.sidebar_state_file,
    )?))?;
    let accounts = state["electron-persisted-atom-state"]["sidebar-custom-sections-v3"]
        .as_object()
        .context("客户端分区映射无效")?;
    let entries = account_sections(config, accounts)?;
    let mut logical = HashMap::new();
    let mut identifiers = HashSet::new();
    for entry in entries {
        let identifier = entry["id"].as_str().context("逻辑分区缺少 ID")?;
        ensure!(identifiers.insert(identifier), "逻辑分区 ID 重复");
        if let Some(backend) = entry["hostSectionIds"][&config.host_id].as_str() {
            ensure!(
                logical.insert(backend.into(), identifier.into()).is_none(),
                "主机分区映射重复"
            );
        }
    }
    Ok(logical)
}

fn account_sections<'a>(
    config: &Config,
    accounts: &'a serde_json::Map<String, Value>,
) -> Result<&'a Vec<Value>> {
    let candidates: Vec<_> = accounts
        .values()
        .filter_map(|account| account["sections"].as_array())
        .filter(|entries| {
            config
                .section_bindings()
                .all(|(_, _, identifier)| entries.iter().any(|entry| entry["id"] == identifier))
        })
        .collect();
    ensure!(!candidates.is_empty(), "未找到匹配账号；缺少状态分区：{}", missing(config, accounts));
    ensure!(candidates.len() == 1, "{} 个账号同时匹配状态分区，请核对账号和分区 ID", candidates.len());
    Ok(candidates[0])
}

fn missing(config: &Config, accounts: &serde_json::Map<String, Value>) -> String {
    accounts.values().filter_map(|account| account["sections"].as_array())
        .map(|entries| config.section_bindings()
            .filter(|(_, _, id)| !entries.iter().any(|entry| entry["id"] == *id))
            .map(|(_, label, _)| label).collect::<Vec<_>>())
        .min_by_key(Vec::len)
        .filter(|labels| !labels.is_empty())
        .map(|labels| labels.join("、"))
        .unwrap_or_else(|| "客户端没有可读取的账号分区记录".into())
}

impl Catalog {
    pub fn load(reader: &mut (impl Rpc + ?Sized), config: &Config) -> Result<Self> {
        let catalog = Self {
            sections: sections(reader, config)?,
            logical: logical(config)?,
        };
        catalog.validate(config)?;
        Ok(catalog)
    }

    fn validate(&self, config: &Config) -> Result<()> {
        for (_, label, identifier) in config.section_bindings() {
            let backend = self
                .logical
                .iter()
                .find(|(_, logical)| logical.as_str() == identifier)
                .map(|(backend, _)| backend);
            if config.host_id != "local" && backend.is_none() {
                continue;
            }
            let backend = backend.with_context(|| format!("状态分区缺少主机映射：{label}"))?;
            let section = self.sections.get(backend).with_context(|| format!("原生分区已移除：{label}"))?;
            ensure!(section["name"] == label, "原生分区名称不匹配：{label}");
        }
        Ok(())
    }

    pub fn observe(&self, config: &Config, thread: &Value) -> Result<Observation> {
        let section = thread.get("section").context("原生会话缺少分区字段")?;
        if section.is_null() {
            return super::observe(config, &Value::Null);
        }
        let backend = section["id"].as_str().context("原生会话分区无效")?;
        let current = self.sections.get(backend).context("会话原生分区未登记")?;
        ensure!(current == section, "原生分区在读取期间已更新");
        let identifier = self
            .logical
            .get(backend)
            .map(String::as_str)
            .unwrap_or(backend);
        super::observe(config, &json!({"sectionId":identifier}))
    }
}

pub fn read(reader: &mut impl Rpc, config: &Config, identifier: &str) -> Result<Observation> {
    uuid::Uuid::parse_str(identifier).context("无效的本机会话 ID")?;
    let catalog = Catalog::load(reader, config)?;
    let response = reader.call(
        "thread/read",
        json!({"threadId":identifier,"includeTurns":false}),
    )?;
    let thread = response.get("thread").context("原生会话读取失败")?;
    ensure!(thread["id"] == identifier, "原生会话 ID 不匹配");
    catalog.observe(config, thread)
}

pub fn archived_ids(reader: &mut impl Rpc, config: &Config) -> Result<HashSet<String>> {
    let (identifiers, complete) =
        crate::rpc::archived_thread_ids(reader, config.page_size, config.max_pages)?;
    ensure!(complete, "原生归档列表扫描未完成");
    Ok(identifiers)
}

pub fn snapshot(reader: &mut impl Rpc, config: &Config) -> Result<HashMap<String, Observation>> {
    let catalog = Catalog::load(reader, config)?;
    let (threads, complete) = crate::rpc::list_threads(reader, config.page_size, config.max_pages)?;
    ensure!(complete, "原生会话位置扫描未完成");
    threads
        .iter()
        .map(|thread| {
            let identifier = thread["id"].as_str().context("原生会话 ID 无效")?;
            Ok((identifier.into(), catalog.observe(config, thread)?))
        })
        .collect()
}

pub fn move_checked(
    reader: &mut impl Rpc,
    writer: &mut impl Rpc,
    config: &Config,
    identifier: &str,
    expected: &str,
    state: Option<&str>,
) -> Result<Observation> {
    let before = read(reader, config, identifier)?;
    ensure!(before.version == expected, "原生分区已更新，请刷新后重试");
    let Some(state) = state else {
        return Ok(before);
    };
    let target = config.section_target(state)?;
    if before.version == target {
        return Ok(before);
    }
    writer
        .call(
            "move_thread_to_sidebar_section",
            json!({
                "threadId":identifier,"sectionId":target,"source":"codex","hostId":config.host_id
            }),
        )
        .context(UnconfirmedMove)?;
    confirm_move(reader, config, identifier, target)
}

pub(crate) fn confirm_move(
    reader: &mut impl Rpc,
    config: &Config,
    identifier: &str,
    target: &str,
) -> Result<Observation> {
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(config.rpc_timeout_seconds.min(4));
    loop {
        let result = read(reader, config, identifier).and_then(|observed| {
            ensure!(observed.version == target, "原生分区写入未确认，请刷新核对");
            Ok(observed)
        });
        if result.is_ok() || std::time::Instant::now() >= deadline {
            return result;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
