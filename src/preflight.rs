use crate::{config, native_sections::direct::Catalog, rpc::{DesktopConnection, Role}};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub fn sections(path: &Path) -> Result<Value> {
    let config = config::load(Some(path))?;
    ensure!(config.native_sections && config.host_id == "local",
        "分区预检要求本机原生五状态根配置");
    let mut reader = DesktopConnection::connect(
        &config.desktop_socket, Duration::from_secs(config.rpc_timeout_seconds), Role::NativeReader
    )?;
    Catalog::load(&mut reader, &config)?;
    let bindings: Vec<_> = config.section_bindings()
        .map(|(state, label, identifier)| json!({"state":state,"label":label,"sectionId":identifier}))
        .collect();
    Ok(json!({
        "app":"codex-session-board","version":env!("CARGO_PKG_VERSION"),
        "build":config::build_id()?,"configId":config::config_id(&config)?,
        "dataDir":config.data_dir,"hostId":config.host_id,
        "nativeCatalogVerified":true,"bindings":bindings
    }))
}
