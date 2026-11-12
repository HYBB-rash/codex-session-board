use super::*;
use crate::{app::App, config::Config};
use std::sync::{Arc, mpsc};

    fn app(path: &std::path::Path) -> Result<Arc<App>> {
        Ok(Arc::new(App::new(Config {
            data_dir: path.into(),
            sync_titles: false,
            ..Config::default()
        })?))
    }

    fn sleeper(app: &Arc<App>, finished: Arc<std::sync::atomic::AtomicUsize>) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        let worker = Arc::clone(app);
        app.workers.spawn(move || {
            sender.send(())?;
            pause(&worker.enabled, Duration::from_secs(3600), None);
            finished.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })?;
        receiver.recv_timeout(Duration::from_secs(1))?;
        Ok(())
    }

#[test]
    fn shutdown_interrupts_long_waits_joins_root_and_remote_and_releases_arcs() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = app(&directory.path().join("root"))?;
        let remote = app(&directory.path().join("remote"))?;
        let finished = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..4 {
            sleeper(&root, finished.clone())?;
            sleeper(&remote, finished.clone())?;
        }
        root.install_remote("remote:a".into(), remote.clone(), false)?;
        let weak_root = Arc::downgrade(&root);
        let weak_remote = Arc::downgrade(&remote);
        let started = Instant::now();
        root.shutdown()?;
        assert!(started.elapsed() < Duration::from_millis(600));
        assert_eq!(finished.load(Ordering::SeqCst), 8);
        assert!(root.workers.spawn(|| Ok(())).is_err());
        assert!(remote.workers.spawn(|| Ok(())).is_err());
        root.shutdown()?;
        drop(remote);
        drop(root);
        assert!(weak_root.upgrade().is_none());
        assert!(weak_remote.upgrade().is_none());
        Ok(())
    }

#[test]
    fn shutdown_rejects_remote_registration_and_closes_rejected_remote() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = app(&directory.path().join("root"))?;
        let remote = app(&directory.path().join("remote"))?;
        root.shutdown()?;
        assert!(
            root.install_remote("late".into(), remote.clone(), false)
                .is_err()
        );
        assert!(root.remotes.lock().unwrap().is_empty());
        assert!(!remote.enabled.load(Ordering::SeqCst));
        assert!(remote.workers.spawn(|| Ok(())).is_err());
        Ok(())
    }

#[test]
    fn deactivate_from_a_registered_worker_preserves_operations_without_self_join() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let remote = app(directory.path())?;
        let worker = remote.clone();
        let (sender, receiver) = mpsc::channel();
        let operation = remote.operation()?;
        remote.workers.spawn(move || {
            worker.deactivate()?;
            sender.send(())?;
            Ok(())
        })?;
        while remote.enabled.load(Ordering::SeqCst) {
            thread::yield_now();
        }
        assert!(remote.operation().is_err());
        assert!(receiver.recv_timeout(Duration::from_millis(20)).is_err());
        drop(operation);
        receiver.recv_timeout(Duration::from_secs(1))?;
        remote.shutdown()?;
        Ok(())
    }

#[test]
    fn concurrent_shutdowns_both_wait_for_the_last_worker() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = app(directory.path())?;
        let (release, wait) = mpsc::channel();
        root.workers.spawn(move || {
            wait.recv_timeout(Duration::from_secs(2))?;
            Ok(())
        })?;
        let (sender, receiver) = mpsc::channel();
        let mut owners = Vec::new();
        for _ in 0..2 {
            let root = root.clone();
            let sender = sender.clone();
            owners.push(thread::spawn(move || sender.send(root.shutdown()).unwrap()));
        }
        assert!(receiver.recv_timeout(Duration::from_millis(20)).is_err());
        release.send(())?;
        for _ in 0..2 {
            receiver.recv_timeout(Duration::from_secs(1))??;
        }
        for owner in owners {
            owner.join().unwrap();
        }
        Ok(())
    }

#[test]
    fn shutdown_closes_registration_before_draining_inflight_operations() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = app(&directory.path().join("root"))?;
        let late = app(&directory.path().join("late"))?;
        let operation = root.operation()?;
        let owner = root.clone();
        let shutdown = thread::spawn(move || owner.shutdown());
        let deadline = Instant::now() + Duration::from_secs(1);
        while root.workers.while_open(|| Ok(())).is_ok() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(root.install_remote("late".into(), late, false).is_err());
        assert!(root.workers.spawn(|| Ok(())).is_err());
        assert!(root.remotes.lock().unwrap().is_empty());
        drop(operation);
        shutdown.join().unwrap()?;
        Ok(())
    }
