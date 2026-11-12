use super::Guard;
use crate::{app::App, workers::pause};
use anyhow::Result;
use serde_json::json;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

pub(crate) fn start(app: &Arc<App>) -> Result<()> {
    let Some(profile) = app.config.client_singleton_profile.clone() else {
        return Ok(());
    };
    let worker = Arc::clone(app);
    app.workers.spawn(move || {
        let mut guard = None;
        while worker.enabled.load(Ordering::SeqCst) {
            let (next, error) = match Guard::acquire(&profile) {
                Ok(next) => {
                    let error = next
                        .as_ref()
                        .and_then(|guard| match guard.cookie_present() {
                            Ok(true) => None,
                            Ok(false) => {
                                Some("singleton cookie 缺失，将在下次跳转前校验恢复".to_string())
                            }
                            Err(error) => Some(format!("{error:#}")),
                        });
                    (next, error)
                }
                Err(error) => (None, Some(format!("{error:#}"))),
            };
            worker.update_health(
                json!({"singletonProtected":next.is_some(),"singletonError":error}),
            )?;
            drop(std::mem::replace(&mut guard, next));
            pause(&worker.enabled, Duration::from_secs(1), None);
        }
        Ok(())
    })
}

pub(crate) fn navigation(config: &crate::config::Config) -> Result<Option<Guard>> {
    let guard = config
        .client_singleton_profile
        .as_deref()
        .map(Guard::acquire)
        .transpose()?
        .flatten();
    if let Some(guard) = &guard {
        guard.prepare()?;
    }
    Ok(guard)
}
