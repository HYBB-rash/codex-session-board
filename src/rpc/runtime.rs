use super::*;
use crate::{app::App, model::now};

struct Clock {
    offset: f64,
    uncertainty: f64,
    sampled: Instant,
    local_time: f64,
}

impl Clock {
    fn sample(command: &[String], timeout: Duration) -> Result<Self> {
        let started = now();
        let elapsed = Instant::now();
        let remote = pipe::first_line(command, timeout)?.parse::<f64>()?;
        let finished = now();
        Self::bounds(started, finished, elapsed.elapsed().as_secs_f64(), remote)
    }

    fn bounds(started: f64, finished: f64, elapsed: f64, remote: f64) -> Result<Self> {
        ensure!(
            [started, finished, elapsed, remote]
                .iter()
                .all(|value| value.is_finite())
                && remote > 0.0
                && remote <= 253402300799.0
                && (0.0..=4.0).contains(&elapsed)
                && ((finished - started) - elapsed).abs() < 0.25,
            "无法确定远程时钟范围，保留人工选择"
        );
        Ok(Self {
            offset: (started + finished) / 2.0 - remote,
            uncertainty: elapsed / 2.0,
            sampled: Instant::now(),
            local_time: now(),
        })
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            (now() - self.local_time - self.sampled.elapsed().as_secs_f64()).abs() < 0.25,
            "本机时钟跳变，需要重新核对远程时钟"
        );
        Ok(())
    }

    fn project(&self, response: &mut Value) -> Result<()> {
        self.validate()?;
        if let Some(turns) = response["data"].as_array_mut() {
            for turn in turns {
                for field in ["startedAt", "completedAt"] {
                    if let Some(time) = turn[field].as_f64() {
                        let earliest = time + self.offset - self.uncertainty;
                        ensure!(
                            earliest.is_finite() && (0.0..=253402300799.0).contains(&earliest),
                            "远程时间无效"
                        );
                        turn[field] = json!(earliest);
                    }
                }
            }
        }
        Ok(())
    }

    fn project_metadata(&self, response: &mut Value) -> Result<()> {
        self.validate()?;
        if let Some(thread) = response.get_mut("thread") {
            return self.metadata(thread);
        }
        if let Some(threads) = response["data"].as_array_mut() {
            for thread in threads {
                self.metadata(thread)?;
            }
        }
        Ok(())
    }

    fn metadata(&self, thread: &mut Value) -> Result<()> {
        for field in ["createdAt", "updatedAt"] {
            if !thread[field].is_null() {
                let time = thread[field].as_i64().context("远程会话时间格式无效")?;
                let local = time as f64 + self.offset;
                ensure!(
                    local.is_finite() && (0.0..=253402300799.0).contains(&local),
                    "远程会话时间越界"
                );
                thread[field] = json!(local.round() as i64);
            }
        }
        Ok(())
    }
}

pub(crate) struct RuntimeConnection {
    connection: DesktopConnection,
    command: Vec<String>,
    clock: Option<Clock>,
    timeout: Duration,
}

impl RuntimeConnection {
    pub(crate) fn connect(app: &App) -> Result<Self> {
        Self::connect_role(
            app,
            if app.config.proxy_command.is_empty() {
                Role::Reader
            } else {
                Role::RemoteObserver
            },
        )
    }

    pub(crate) fn draft(app: &App) -> Result<Self> {
        Self::connect_role(app, Role::DraftWriter)
    }

    fn connect_role(app: &App, role: Role) -> Result<Self> {
        let timeout = Duration::from_secs(app.config.rpc_timeout_seconds);
        let command = app.config.clock_command.clone();
        ensure!(
            app.config.proxy_command.is_empty() || !command.is_empty(),
            "远程执行观察缺少时钟探针"
        );
        let clock = (!command.is_empty())
            .then(|| Clock::sample(&command, timeout))
            .transpose()?;
        app.update_health(
            json!({"clockUncertaintySeconds":clock.as_ref().map(|clock| clock.uncertainty)}),
        )?;
        Ok(Self {
            connection: app.connection(timeout, role)?,
            command,
            clock,
            timeout,
        })
    }
}

impl Rpc for RuntimeConnection {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        if self
            .clock
            .as_ref()
            .is_some_and(|clock| clock.sampled.elapsed() > Duration::from_secs(30))
        {
            self.clock = Some(Clock::sample(&self.command, self.timeout)?);
        }
        let mut response = self.connection.call(method, params)?;
        if method == "thread/turns/list"
            && let Some(clock) = &self.clock
        {
            clock.project(&mut response)?;
        }
        if matches!(method, "thread/list" | "thread/start" | "thread/read")
            && let Some(clock) = &self.clock
        {
            clock.project_metadata(&mut response)?;
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_observer_cannot_start_resume_rename_move_or_archive() {
        for method in [
            "initialize",
            "thread/list",
            "thread/turns/list",
            "threadSection/list",
        ] {
            assert!(Role::RemoteObserver.check(method).is_ok());
        }
        for method in [
            "thread/start",
            "thread/resume",
            "turn/start",
            "turn/interrupt",
            "thread/name/set",
            "thread/section/move",
            "thread/archive",
            "thread/read",
        ] {
            assert!(Role::RemoteObserver.check(method).is_err());
        }
    }

    #[test]
    fn remote_clock_uses_conservative_bounds_and_rejects_unbounded_samples() -> Result<()> {
        let clock = Clock::bounds(100.0, 100.2, 0.2, 600.1)?;
        let mut turn = json!({"data":[{"startedAt":600.1,"completedAt":601.1}]});
        clock.project(&mut turn)?;
        assert!((turn["data"][0]["startedAt"].as_f64().unwrap() - 100.0).abs() < 0.001);
        assert!((turn["data"][0]["completedAt"].as_f64().unwrap() - 101.0).abs() < 0.001);
        let mut thread = json!({"data":[{"createdAt":600,"updatedAt":601}]});
        clock.project_metadata(&mut thread)?;
        assert_eq!(thread["data"][0]["createdAt"], 100);
        assert_eq!(thread["data"][0]["updatedAt"], 101);
        assert!(Clock::bounds(100.0, 102.0, 0.2, 600.1).is_err());
        assert!(Clock::bounds(100.0, 110.0, 10.0, 600.1).is_err());
        assert!(Clock::bounds(100.0, 100.2, 0.2, f64::NAN).is_err());
        Ok(())
    }

    #[test]
    fn runtime_clock_probe_is_bounded_and_reaps_the_owned_child() -> Result<()> {
        let started = Instant::now();
        let command = vec![
            "python3".into(),
            "-c".into(),
            "import time;time.sleep(30)".into(),
        ];
        assert!(pipe::first_line(&command, Duration::from_millis(50)).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        let value = pipe::first_line(
            &["python3".into(), "-c".into(), "print(123.25)".into()],
            Duration::from_secs(2),
        )?;
        assert_eq!(value, "123.25");
        Ok(())
    }
}
