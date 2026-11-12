use anyhow::{Result, ensure};
use std::sync::{
    Condvar, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct WorkerGroup {
    state: Mutex<State>,
    finished: Condvar,
}

#[derive(Default)]
struct State {
    closed: bool,
    joining: bool,
    joined: bool,
    handles: Vec<JoinHandle<Result<()>>>,
    errors: Vec<String>,
}

impl WorkerGroup {
    pub(crate) fn spawn(&self, work: impl FnOnce() -> Result<()> + Send + 'static) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("worker 登记锁已损坏"))?;
        ensure!(!state.closed, "worker 登记已关闭");
        state.handles.push(thread::Builder::new().spawn(work)?);
        Ok(())
    }

    fn recover(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.errors.push("worker 登记锁已损坏".into());
                state
            }
        }
    }

    pub(crate) fn close(&self) {
        self.recover().closed = true;
    }

    pub(crate) fn join(&self) -> Result<()> {
        let mut state = self.recover();
        ensure!(state.closed, "必须先关闭 worker 登记");
        while state.joining {
            state = match self.finished.wait(state) {
                Ok(state) => state,
                Err(poison) => {
                    let mut state = poison.into_inner();
                    state.errors.push("worker 等待锁已损坏".into());
                    state
                }
            };
        }
        if state.joined {
            return finish(state.errors.clone());
        }
        state.joining = true;
        let handles = std::mem::take(&mut state.handles);
        drop(state);
        let mut errors = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => errors.push(format!("worker 失败：{error:#}")),
                Err(_) => errors.push("worker panic".into()),
            }
        }
        let mut state = self.recover();
        state.errors.extend(errors);
        state.joining = false;
        state.joined = true;
        self.finished.notify_all();
        finish(state.errors.clone())
    }
}

pub(crate) fn finish(errors: Vec<String>) -> Result<()> {
    ensure!(errors.is_empty(), "{}", errors.join("; "));
    Ok(())
}

pub(crate) fn pause(enabled: &AtomicBool, duration: Duration, refresh: Option<&AtomicBool>) {
    let started = Instant::now();
    while enabled.load(Ordering::SeqCst) && started.elapsed() < duration {
        if refresh.is_some_and(|refresh| refresh.swap(false, Ordering::Relaxed)) {
            break;
        }
        thread::sleep(
            duration
                .saturating_sub(started.elapsed())
                .min(Duration::from_millis(100)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn join_reports_panics_and_errors_after_joining_every_handle() -> Result<()> {
        let group = WorkerGroup::default();
        let finished = Arc::new(AtomicBool::new(false));
        group.spawn(|| panic!("worker failure"))?;
        group.spawn(|| anyhow::bail!("returned failure"))?;
        let completed = finished.clone();
        group.spawn(move || {
            thread::sleep(Duration::from_millis(20));
            completed.store(true, Ordering::SeqCst);
            Ok(())
        })?;
        group.close();
        let error = group.join().unwrap_err().to_string();
        assert!(error.contains("panic"));
        assert!(error.contains("returned failure"));
        assert!(finished.load(Ordering::SeqCst));
        assert!(group.join().is_err());
        Ok(())
    }

    #[test]
    fn poisoned_registration_is_reported_and_existing_handles_are_joined() -> Result<()> {
        let group = Arc::new(WorkerGroup::default());
        let finished = Arc::new(AtomicBool::new(false));
        let completed = finished.clone();
        group.spawn(move || {
            completed.store(true, Ordering::SeqCst);
            Ok(())
        })?;
        let broken = group.clone();
        assert!(
            thread::spawn(move || {
                let _guard = broken.state.lock().unwrap();
                panic!("poison");
            })
            .join()
            .is_err()
        );
        assert!(group.spawn(|| Ok(())).is_err());
        group.close();
        assert!(group.join().unwrap_err().to_string().contains("锁已损坏"));
        assert!(finished.load(Ordering::SeqCst));
        Ok(())
    }

}
