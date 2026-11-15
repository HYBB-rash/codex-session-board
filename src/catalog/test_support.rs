use super::Snapshot;
use crate::{app::App, config::Config};
use anyhow::Result;
use serde_json::{Value, json};

pub(super) const HOST: &str = "remote:catalog";
pub(super) const FIRST: &str = "019a8120-0000-7000-8000-000000000001";
pub(super) const SECOND: &str = "019a8120-0000-7000-8000-000000000002";
pub(super) const THIRD: &str = "019a8120-0000-7000-8000-000000000003";

pub(super) fn app() -> Result<(tempfile::TempDir, App)> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        host_id: HOST.into(),
        native_sections: false,
        sync_titles: false,
        ..Config::default()
    })?;
    Ok((directory, app))
}

pub(super) fn thread(identifier: &str, archived: bool, ordinary: bool) -> Value {
    json!({"id":identifier,"name":"远程目录标题","cwd":"/target/project","updatedAt":1,
        "ordinary":ordinary,"archived":archived,"catalogProjectId":null,
        "membership":{"state":"projectless","projectId":null,"basis":"backend",
            "pending":false,"projectIssue":null}})
}

pub(super) fn snapshot() -> Snapshot {
    Snapshot {
        source: json!({"hostId":HOST,"codexHome":"/target/.codex","version":"0.169.0",
            "peerFingerprint":"verified-peer","generation":"a".repeat(64),
            "stateHash":"b".repeat(64),"stateMetadata":{"isFile":true}}),
        captured_at: crate::model::now(),
        projects: Vec::new(),
        active: vec![thread(FIRST, false, true)],
        archived: Vec::new(),
        coverage: json!({"projects":"complete","ordinaryActive":"complete",
            "ordinaryArchived":"complete","publishedReferences":"complete",
            "membership":"complete","legacy":"complete"}),
        issues: Vec::new(),
    }
}

pub(super) fn install(app: &App, snapshot: Snapshot, live: bool) {
    let mut state = app.catalog.lock().unwrap();
    state.snapshot = Some(snapshot);
    state.live = live;
}
