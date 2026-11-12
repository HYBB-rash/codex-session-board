use crate::{app::App, archive, model::now};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

struct Schedule {
    interval: f64,
    next: f64,
}

impl Schedule {
    fn new(seconds: u64, time: f64) -> Self {
        let interval = seconds as f64;
        Self {
            interval,
            next: (time / interval).floor().mul_add(interval, interval),
        }
    }

    fn due(&mut self, time: f64) -> bool {
        if time < self.next {
            return false;
        }
        self.next = (time / self.interval)
            .floor()
            .mul_add(self.interval, self.interval);
        true
    }
}

pub fn sweep(app: &App) -> Result<Value> {
    ensure!(!app.config.legacy_mode, "兼容模式不执行真实归档");
    ensure!(app.config.auto_archive_seconds.is_some(), "自动归档未启用");
    if app.config.native_sections {
        app.refresh_native()?;
    }
    let report = sweep_with(app, |session| archive::request(app, session))?;
    let _operation = app.operation()?;
    crate::config::write_json(
        &app.config.data_dir.join("auto-archive.json"),
        &report,
        true,
    )?;
    Ok(report)
}

fn sweep_with(app: &App, mut operation: impl FnMut(&Value) -> Result<Value>) -> Result<Value> {
    let mut archived = Vec::new();
    let mut deferred = Vec::new();
    for session in app.store.sessions()? {
        let identifier = session["id"].as_str().unwrap_or("");
        if crate::model::board_state(session["state"].as_str().unwrap_or("")) != "to_archive"
            || identifier.starts_with("remote:")
            || app.config.native_sections && !app.is_active(identifier)?
        {
            continue;
        }
        let result = if session["followUp"] == true {
            Err(anyhow::anyhow!("会话有新输入，请确认待归档后重试"))
        } else {
            operation(&session)
        };
        match result {
            Ok(_) => archived.push(session["id"].clone()),
            Err(error) => deferred.push(json!({"id":identifier,"reason":error.to_string()})),
        }
    }
    Ok(json!({"at":now(),"archived":archived,"deferred":deferred}))
}

pub fn start(app: &Arc<App>) -> Result<()> {
    if app.config.legacy_mode {
        return Ok(());
    }
    let Some(seconds) = app.config.auto_archive_seconds else {
        return Ok(());
    };
    let worker = Arc::clone(app);
    app.workers.spawn(move || run(worker, seconds))
}

fn run(app: Arc<App>, seconds: u64) -> Result<()> {
    let mut schedule = Schedule::new(seconds, now());
    while app.enabled.load(std::sync::atomic::Ordering::SeqCst) {
        app.update_health(json!({
            "autoArchiveSeconds":seconds,"autoArchiveNextAt":schedule.next
        }))?;
        crate::workers::pause(
            &app.enabled,
            Duration::from_secs_f64((schedule.next - now()).clamp(0.01, 30.0)),
            None,
        );
        if !app.enabled.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        if schedule.due(now()) {
            let (report, error) = match sweep(&app) {
                Ok(report) => (report, Value::Null),
                Err(error) => (Value::Null, json!(error.to_string())),
            };
            app.update_health(json!({"autoArchiveLastRun":report,"autoArchiveError":error}))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn half_hour_boundaries_wait_on_start_and_do_not_replay_missed_ticks() {
        let mut schedule = Schedule::new(1800, 3600.0);
        assert_eq!(schedule.next, 5400.0);
        assert!(!schedule.due(5399.0));
        assert!(schedule.due(5400.0));
        assert_eq!(schedule.next, 7200.0);
        assert!(!schedule.due(5401.0));
        assert!(schedule.due(10801.0));
        assert_eq!(schedule.next, 12600.0);
        assert!(!schedule.due(10700.0));
    }

    #[test]
    fn sweep_continues_after_failure_and_preserves_state_notes_and_unreviewed_input() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            sync_titles: false,
            ..Config::default()
        })?;
        app.store.discover(&[
            json!({"id":"first"}),
            json!({"id":"second"}),
            json!({"id":"review"}),
            json!({"id":"follow"}),
        ])?;
        app.store
            .observe("follow", &json!({"id":"turn","userItems":["old"]}), None)?;
        for identifier in ["first", "second", "follow"] {
            app.store.set_state(
                identifier,
                Some("to_archive"),
                "保留备注",
                0,
                Some(i64::from(identifier == "follow")),
            )?;
        }
        app.store.observe(
            "follow",
            &json!({"id":"turn","userItems":["old","new"]}),
            None,
        )?;
        let mut attempts = Vec::new();
        let report = sweep_with(&app, |session| {
            attempts.push(session["id"].as_str().unwrap_or("").to_string());
            ensure!(session["id"] != "first", "连接失败");
            Ok(json!({"ok":true}))
        })?;
        assert_eq!(attempts, ["first", "second"]);
        assert_eq!(report["archived"], json!(["second"]));
        assert_eq!(report["deferred"][1]["id"], "follow");
        assert_eq!(app.store.note("first")?, "保留备注");
        assert_eq!(app.store.sessions()?.len(), 4);
        assert!(sweep(&app).is_err());
        Ok(())
    }
}
