use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod capture;
mod continuity;
mod input;
mod legacy;
mod membership;
pub mod paging;
mod projects;
pub(crate) mod preparation;
mod validation;
pub(crate) mod view;
mod worker;
pub(crate) use worker::start;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod view_tests;

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
        validation::validate(self, host)?;
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
    pub last_check: f64,
    pub last_attempt: f64,
    pub restarts: usize,
}

impl State {
    pub(crate) fn load(store: &crate::store::Store, host: &str) -> Self {
        let result = (|| -> Result<Self> {
            let snapshot = store
                .catalog_value("catalog_snapshot_v1")?
                .map(serde_json::from_value::<Snapshot>)
                .transpose()?;
            if let Some(snapshot) = &snapshot {
                snapshot.validate(host)?;
            }
            Ok(Self {
                snapshot,
                progress: store.catalog_value("catalog_progress_v1")?,
                ..Self::default()
            })
        })();
        result.unwrap_or_else(|error| Self {
            error: Some(format!("目录缓存不可用：{error}")),
            ..Self::default()
        })
    }

    pub(crate) fn health(&self) -> Value {
        let snapshot = self.snapshot.as_ref();
        let complete = snapshot.is_some_and(|snapshot| validation::complete(&snapshot.coverage))
            && self.live
            && self.error.is_none()
            && self.progress.is_none();
        serde_json::json!({"complete":complete,"cached":!self.live,
            "capturedAt":snapshot.map(|snapshot| snapshot.captured_at),
            "coverage":snapshot.map(|snapshot| &snapshot.coverage),
            "issues":snapshot.map(|snapshot| &snapshot.issues),
            "error":self.error,"pending":self.progress.is_some()})
    }
}
