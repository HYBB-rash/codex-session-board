use super::*;

pub struct Catalog {
    sections: HashMap<String, Value>,
    logical: HashMap<String, String>,
}

fn sections(reader: &mut impl Rpc, config: &Config) -> Result<HashMap<String, Value>> {
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
    let candidates: Vec<_> = accounts
        .values()
        .filter_map(|account| account["sections"].as_array())
        .filter(|entries| {
            config
                .status_sections
                .values()
                .all(|identifier| entries.iter().any(|entry| entry["id"] == *identifier))
        })
        .collect();
    ensure!(candidates.len() == 1, "无法唯一确定原生分区所属账号");
    let mut logical = HashMap::new();
    let mut identifiers = HashSet::new();
    for entry in candidates[0] {
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

impl Catalog {
    pub fn load(reader: &mut impl Rpc, config: &Config) -> Result<Self> {
        let catalog = Self {
            sections: sections(reader, config)?,
            logical: logical(config)?,
        };
        catalog.validate(config)?;
        Ok(catalog)
    }

    fn validate(&self, config: &Config) -> Result<()> {
        for (state, identifier) in &config.status_sections {
            let backend = self
                .logical
                .iter()
                .find(|(_, logical)| *logical == identifier)
                .map(|(backend, _)| backend);
            if config.host_id != "local" && backend.is_none() {
                continue;
            }
            let backend = backend.context("状态分区缺少主机映射")?;
            let section = self.sections.get(backend).context("原生分区已移除")?;
            let label = model::STATES
                .iter()
                .find(|item| item.0 == state)
                .context("未知状态分区")?
                .1;
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
    let target = config
        .status_sections
        .get(model::board_state(state))
        .context("目标原生分区未配置")?;
    if before.version == *target {
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
    let after = read(reader, config, identifier).context(UnconfirmedMove)?;
    ensure!(after.version == *target, "原生分区写入未确认，请刷新核对");
    Ok(after)
}
