use anyhow::{Context, Result, ensure};
use codex_session_board::{
    archive,
    config::Config,
    native_sections::{self, Observation},
    rpc::Rpc,
    store::Store,
    titles,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub trait HostRpc: Rpc {
    fn host(&self) -> &str;
    fn execution_authority(&self) -> bool;
}

pub struct RuntimeEvidence<'value> {
    pub generation: u64,
    pub status: &'value Value,
    pub turn: &'value Value,
    pub since: f64,
}

pub struct Partition {
    pub host: String,
    pub config: Config,
    pub store: Store,
    pub generation: AtomicU64,
}

impl Partition {
    pub fn open(root: &Path, host: &str) -> Result<Self> {
        ensure!(!host.is_empty(), "Empty host");
        let directory = root.join(format!("{:x}", Sha256::digest(host.as_bytes())));
        std::fs::create_dir_all(&directory)?;
        let mut store = Store::open(&directory, true)?;
        store.native_sections = true;
        let config = Config {
            host_id: host.into(),
            native_sections: true,
            status_sections: codex_session_board::model::board_states()
                .map(|(state, _)| (state.into(), format!("{state}-id")))
                .collect(),
            ..Config::default()
        };
        Ok(Self {
            host: host.into(),
            config,
            store,
            generation: AtomicU64::new(0),
        })
    }

    pub fn discover(&self, host: &str, threads: &[Value]) -> Result<()> {
        ensure!(host == self.host, "Wrong host");
        let threads = threads
            .iter()
            .map(|thread| {
                let identifier = thread["id"].as_str().context("Missing thread ID")?;
                uuid::Uuid::parse_str(identifier)?;
                ensure!(thread["hostId"] == self.host, "Wrong thread host");
                let mut thread = thread.clone();
                thread["name"] = thread["title"].clone();
                Ok(thread)
            })
            .collect::<Result<Vec<_>>>()?;
        self.store.discover(&threads)
    }

    pub fn refresh(&self, reader: &mut impl HostRpc, identifier: &str) -> Result<Observation> {
        ensure!(reader.host() == self.host, "Wrong connection host");
        let generation = self.generation.load(Ordering::SeqCst);
        let observed = native_sections::read(reader, &self.config, identifier)?;
        self.cache_snapshot(generation, identifier, &observed)?;
        Ok(observed)
    }

    pub fn cache_snapshot(
        &self,
        generation: u64,
        identifier: &str,
        native: &Observation,
    ) -> Result<()> {
        ensure!(
            generation == self.generation.load(Ordering::SeqCst),
            "Obsolete connection snapshot"
        );
        self.store.cache_native(identifier, native)
    }

    pub fn reconnect(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn row(&self, identifier: &str) -> Result<Value> {
        self.store
            .sessions()?
            .into_iter()
            .find(|row| row["id"] == identifier)
            .context("Missing thread")
    }

    pub fn save(
        &self,
        authority: &mut impl HostRpc,
        identifier: &str,
        state: &str,
        note: &str,
        expected: &Value,
    ) -> Result<()> {
        ensure!(authority.host() == self.host, "Wrong connection host");
        let revision = revisions(expected)?;
        self.store
            .validate_state(identifier, Some(state), note, revision)?;
        let section = expected["sectionVersion"]
            .as_str()
            .context("Missing native section")?;
        let confirmed = native_sections::move_checked(
            authority,
            &self.config,
            identifier,
            section,
            Some(state),
        )?;
        self.store
            .set_native_state(identifier, Some(state), note, revision, &confirmed)
    }

    pub fn sync_title(&self, target: &mut impl HostRpc, identifier: &str) -> Result<()> {
        ensure!(target.host() == self.host, "Wrong connection host");
        titles::sync(&self.store, target, identifier)
    }

    pub fn runtime(
        &self,
        authority: &mut impl HostRpc,
        identifier: &str,
        evidence: RuntimeEvidence<'_>,
    ) -> Result<()> {
        ensure!(authority.host() == self.host, "Wrong connection host");
        ensure!(
            authority.execution_authority(),
            "Execution authority unknown"
        );
        ensure!(
            evidence.generation == self.generation.load(Ordering::SeqCst),
            "Obsolete runtime evidence"
        );
        if let Some(change) = self.store.prepare_native_runtime(
            identifier,
            evidence.status,
            evidence.turn,
            evidence.since,
        )? {
            if !self
                .config
                .status_sections
                .values()
                .any(|section| *section == change.section_version)
            {
                return self.store.consume_native_runtime(identifier, &change);
            }
            let confirmed = native_sections::move_checked(
                authority,
                &self.config,
                identifier,
                &change.section_version,
                Some(change.state),
            )?;
            self.store
                .commit_native_runtime(identifier, &change, &confirmed)?;
        }
        Ok(())
    }

    pub fn archive(&self, target: &mut impl HostRpc, identifier: &str, row: &Value) -> Result<()> {
        ensure!(target.host() == self.host, "Wrong connection host");
        ensure!(target.execution_authority(), "Execution authority unknown");
        let native = native_sections::read(target, &self.config, identifier)?;
        ensure!(
            native.state == "to_archive" && row["sectionVersion"] == native.version,
            "Native archive eligibility changed"
        );
        let revision = revisions(row)?;
        let expected = self.store.prepare_archive(identifier, revision)?;
        archive::archive(target, identifier, self.config.max_pages, &expected)?;
        self.store.confirm_archive(identifier, revision)
    }
}

fn revisions(row: &Value) -> Result<(i64, i64)> {
    Ok((
        row["version"].as_i64().context("Missing version")?,
        row["inputVersion"]
            .as_i64()
            .context("Missing input version")?,
    ))
}

pub fn seed(host: &str, identifier: &str) -> Value {
    json!({"id":identifier,"hostId":host,"title":"完整标题","updatedAt":1})
}
