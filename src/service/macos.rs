use super::*;
use nix::sys::event::{EventFilter, EventFlag, FilterFlag, KEvent, Kqueue};
use sha2::Digest;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;

pub(crate) fn process_start(pid: u32) -> Result<Option<u64>> {
    ensure!(pid > 0, "无效进程 PID");
    let output = Command::new("/bin/ps")
        .current_dir("/")
        .env("LC_ALL", "C")
        .args(["-p", &pid.to_string(), "-o", "stat=", "-o", "lstart="])
        .output()?;
    if !output.status.success() || output.stdout.is_empty() {
        return Ok(None);
    }
    let text = String::from_utf8(output.stdout)?;
    let mut fields = text.split_whitespace();
    if fields.next().is_some_and(|state| state.starts_with('Z')) {
        return Ok(None);
    }
    let digest = sha2::Sha256::digest(fields.collect::<Vec<_>>().join(" "));
    Ok(Some(u64::from_be_bytes(digest[..8].try_into()?)))
}

fn connection(path: &Path) -> Result<UnixStream> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_socket()
            && metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o7777 == 0o600,
        "服务控制连接所有者或权限无效"
    );
    let stream = UnixStream::connect(path)?;
    ensure!(
        nix::unistd::getpeereid(&stream)?.0.as_raw() == rustix::process::getuid().as_raw(),
        "服务控制连接用户不符"
    );
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(stream)
}

fn read_line(stream: &UnixStream) -> Result<String> {
    use std::io::Read;
    let mut text = String::new();
    BufReader::new(stream).take(8193).read_line(&mut text)?;
    ensure!(
        text.len() <= 8192 && text.ends_with('\n'),
        "服务控制响应无效"
    );
    Ok(text)
}

fn handle(mut stream: UnixStream, identity: &Value) -> Result<()> {
    stream.set_nonblocking(false)?;
    ensure!(
        nix::unistd::getpeereid(&stream)?.0.as_raw() == rustix::process::getuid().as_raw(),
        "服务控制请求用户不符"
    );
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    writeln!(stream, "{identity}")?;
    ensure!(read_line(&stream)? == "stop\n", "服务控制命令无效");
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::INT)?;
    Ok(())
}

pub(super) fn listen(app: &Arc<App>) -> Result<()> {
    let config = &app.config;
    let path = config.data_dir.join("control.sock");
    if path.exists() {
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(metadata.file_type().is_socket(), "服务控制路径不是 socket");
        ensure!(UnixStream::connect(&path).is_err(), "服务控制连接仍在使用");
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let identity = info(config)?;
    let worker = Arc::clone(app);
    app.workers.spawn(move || {
        while worker.enabled.load(std::sync::atomic::Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Err(error) = handle(stream, &identity) {
                        eprintln!("服务控制请求失败：{error}");
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(error.into()),
            }
        }
        fs::remove_file(path)?;
        Ok(())
    })
}

pub(super) fn stop(pid: u32, start: u64, url: &str, config: &Config) -> Result<()> {
    let mut stream = connection(&config.data_dir.join("control.sock"))?;
    let identity: Value = serde_json::from_str(&read_line(&stream)?)?;
    verify_info(&identity, config, Some(pid))?;
    ensure!(
        identity["configId"] == config::config_id(config)?,
        "服务控制配置身份不符"
    );
    verify_info(
        &probe(url)?.context("进程仍存活但无法核验服务身份")?,
        config,
        Some(pid),
    )?;
    let queue = Kqueue::new()?;
    let event = KEvent::new(
        pid as usize,
        EventFilter::EVFILT_PROC,
        EventFlag::EV_ADD | EventFlag::EV_ONESHOT,
        FilterFlag::NOTE_EXIT,
        0,
        0,
    );
    queue.kevent(
        &[event],
        &mut [],
        Some(nix::libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
    )?;
    confirm_process(pid, start)?;
    stream.write_all(b"stop\n")?;
    wait_exit(queue, event, config)
}

fn wait_exit(queue: Kqueue, event: KEvent, config: &Config) -> Result<()> {
    let mut events = [event];
    let timeout = nix::libc::timespec {
        tv_sec: config.rpc_timeout_seconds.saturating_add(10).try_into()?,
        tv_nsec: 0,
    };
    let count = queue.kevent(&[], &mut events, Some(timeout))?;
    ensure!(
        count == 1
            && events[0].fflags().contains(FilterFlag::NOTE_EXIT)
            && !events[0].flags().contains(EventFlag::EV_ERROR),
        "服务进程退出尚未确认"
    );
    lock_file(&config.data_dir)?
        .try_lock_exclusive()
        .context("服务数据锁尚未释放")?;
    Ok(())
}
