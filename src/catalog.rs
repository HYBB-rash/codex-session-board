use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod paging;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub source: Value,
    pub captured_at: f64,
    pub projects: Vec<Value>,
    pub active: Vec<Value>,
    pub archived: Vec<Value>,
    pub coverage: Value,
    pub issues: Vec<String>,
}

impl Snapshot {
    pub fn validate(&self, host: &str) -> Result<()> {
        ensure!(self.source["hostId"] == host, "目录缓存主机不匹配");
        ensure!(
            self.source["codexHome"]
                .as_str()
                .is_some_and(|home| home.starts_with('/'))
                && self.source["peerFingerprint"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty()),
            "目录缓存缺少来源证据"
        );
        ensure!(
            self.captured_at.is_finite() && self.captured_at > 0.0,
            "目录时间无效"
        );
        Ok(())
    }

    pub fn contains(&self, thread: &str) -> bool {
        self.active
            .iter()
            .chain(&self.archived)
            .any(|row| row["id"] == thread)
    }
}

#[derive(Default)]
pub struct State {
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
    pub live: bool,
    pub progress: Option<Value>,
}
