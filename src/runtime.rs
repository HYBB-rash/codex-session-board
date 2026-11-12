use crate::{config::Config, service};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::{Path, PathBuf};

mod configuration;

enum Board {
    Owned(service::RunningBoard),
    Shared(String),
}

pub(crate) struct RuntimeHandle {
    path: PathBuf,
    settings: Config,
    client: reqwest::blocking::Client,
    board: Option<Board>,
}

impl RuntimeHandle {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let settings = configuration::load(path)?;
        let path = path.to_path_buf();
        let timeout = if settings.sidebar_socket.as_os_str().is_empty() {
            8
        } else {
            settings.rpc_timeout_seconds.saturating_mul(5).saturating_add(10)
        };
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(timeout))
            .build()?;
        Ok(Self { path, settings, client, board: None })
    }

    pub(crate) fn legacy(&self) -> bool {
        self.settings.legacy_mode
    }

    fn acquire(&mut self) -> Result<()> {
        if matches!(self.board, Some(Board::Owned(_))) {
            return Ok(());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let opened = loop {
            match service::open_loaded(&self.path, self.settings.clone()) {
                Ok(board) => break board,
                Err(error) if error.downcast_ref::<service::OccupiedInstance>().is_some()
                    && std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(error) => return Err(error),
            }
        };
        self.board = Some(match opened {
            service::OpenedBoard::Owned(board) => Board::Owned(board.start()?),
            service::OpenedBoard::Reused { url, .. } => Board::Shared(url),
        });
        Ok(())
    }

    pub(crate) fn request(&mut self, path: &str, body: Option<&Value>) -> Result<Value> {
        self.acquire()?;
        match self.board.as_ref().context("插件运行时未取得所有权")? {
            Board::Owned(board) => board.app.request(path, body),
            Board::Shared(url) => self.forward(url, path, body),
        }
    }

    fn forward(&self, url: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let response = if let Some(body) = body {
            let identity: Value = self.client.get(format!("{url}/api/info"))
                .send()?.error_for_status()?.json()?;
            let token = identity["requestToken"].as_str().context("运行时未提供请求令牌")?;
            self.client.post(format!("{url}{path}"))
                .header("X-CSRF-Token", token).json(body).send()?
        } else {
            self.client.get(format!("{url}{path}")).send()?
        };
        let status = response.status();
        let result: Value = response.json()?;
        ensure!(status.is_success(), "{}", result["error"].as_str().unwrap_or("看板请求失败"));
        Ok(result)
    }

    pub(crate) fn shutdown(&mut self) -> Result<()> {
        match self.board.take() {
            Some(Board::Owned(mut board)) => board.shutdown(),
            _ => Ok(()),
        }
    }
}
