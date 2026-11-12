use super::*;
use std::thread::JoinHandle;

pub(crate) struct RunningBoard {
    pub app: Arc<App>,
    server: Option<Arc<Server>>,
    listener: Option<JoinHandle<Result<()>>>,
    lease: Option<(File, File)>,
}

impl OwnedBoard {
    pub(crate) fn start(self) -> Result<RunningBoard> {
        let mut running = RunningBoard {
            app: self.app,
            server: Some(Arc::new(self.server)),
            listener: None,
            lease: Some((self.lock, self.instance)),
        };
        crate::observer::start(&running.app)?;
        #[cfg(target_os = "macos")]
        macos::listen(&running.app)?;
        let server = Arc::clone(running.server.as_ref().context("缺少监听器")?);
        let app = Arc::clone(&running.app);
        running.listener = Some(thread::Builder::new().name("board-http".into())
            .spawn(move || crate::http::run_shared(server, self.port, app))?);
        Ok(running)
    }

    pub fn run(self) -> Result<()> {
        self.start()?.wait()
    }
}

impl RunningBoard {
    fn join_listener(&mut self) -> Result<()> {
        match self.listener.take() {
            Some(listener) => listener.join()
                .map_err(|_| anyhow::anyhow!("看板监听线程异常退出"))?,
            None => Ok(()),
        }
    }

    fn wait(mut self) -> Result<()> {
        let listening = self.join_listener();
        let shutdown = self.shutdown();
        listening.and(shutdown)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if self.lease.is_none() {
            return Ok(());
        }
        self.app.enabled.store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(server) = &self.server {
            server.unblock();
        }
        let workers = self.app.shutdown();
        let listening = self.join_listener();
        self.server.take();
        self.lease.take();
        workers.and(listening)
    }
}

impl Drop for RunningBoard {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("插件运行时关闭失败：{error:#}");
        }
    }
}
