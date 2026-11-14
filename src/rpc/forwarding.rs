use super::*;
use crate::{config::Config, remote::hosts};

pub(super) struct Guard {
    settings: Config,
    shared: Option<PathBuf>,
}

impl Guard {
    pub(super) fn new(settings: &Config) -> Self {
        Self {
            settings: settings.clone(),
            shared: std::env::var_os("CODEX_SESSION_BOARD_SHARED_AUTHORITY").map(PathBuf::from),
        }
    }

    pub(super) fn verify_source(&self, transport: &pipe::Transport) -> Result<()> {
        let pipe::Transport::Unix(stream) = transport else {
            bail!("侧栏写入需要可证明来源的本地连接");
        };
        let backend = self.settings.local_authority.as_ref().context("侧栏写入缺少客户端后台归属")?;
        authority::sidebar::verify(backend, stream, self.shared.as_deref())
    }

    pub(super) fn verify(&self, params: &Value, deadline: Instant, transport: &pipe::Transport) -> Result<()> {
        self.verify_source(transport)?;
        let host = params["hostId"].as_str().unwrap_or("local");
        if let Some(binding) = &self.settings.remote_binding {
            ensure!(host == binding.host_id, "转发目标与已核验的远端来源不匹配");
            binding.require_confirmed()?;
            return framing::verify_current(binding, deadline);
        }
        if host == "local" || self.settings.sidebar_state_file.as_os_str().is_empty() {
            return Ok(());
        }
        let state: Value =
            serde_json::from_slice(&std::fs::read(&self.settings.sidebar_state_file)?)?;
        if state.get("codex-managed-remote-connections").is_none() {
            return Ok(());
        }
        let descriptor = hosts::registered(&self.settings)?
            .into_iter()
            .find(|entry| entry.host_id == host)
            .context("转发主机已移除或未登记")?;
        let binding = Binding::existing(&descriptor, &self.settings)?;
        framing::verify_current(&binding, deadline)
    }
}

#[cfg(test)]
#[path = "forwarding_tests.rs"]
mod tests;
