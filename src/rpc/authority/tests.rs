use super::*;
use crate::rpc::{DesktopConnection, Role};
use anyhow::bail;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn shared_url_accepts_only_local_unix_transport() -> Result<()> {
    assert_eq!(socket_url("ws+unix://localhost/private/tmp/backend.sock:/rpc")?,
        PathBuf::from("/private/tmp/backend.sock"));
    for url in ["ws://localhost:4500/rpc", "ws+unix://other/tmp/server:/rpc",
        "ws+unix://localhostrelative:/rpc", "ws+unix://localhost/tmp/server:/other",
        "ws+unix://localhost/tmp/%2Fserver:/rpc"] {
        assert!(socket_url(url).is_err(), "{url}");
    }
    Ok(())
}

#[test]
fn listener_capability_and_stale_process_epoch_are_checked() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let listener = UnixListener::bind(directory.path().join("server"))?;
    let source = LocalAuthority::owned_listener(&listener)?;
    let stream = UnixStream::connect(listener.local_addr()?.as_pathname().context("缺少路径")?)?;
    source.verify(&stream)?;
    let stale = LocalAuthority { started: source.started.wrapping_add(1), ..source };
    assert!(stale.verify(&stream).is_err());
    Ok(())
}

#[test]
fn same_user_wrong_backend_is_rejected_before_rpc_handshake_for_every_role() -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let socket = directory.path().join("wrong.sock");
    let script = "import os,socket,sys\nserver=socket.socket(socket.AF_UNIX);server.bind(sys.argv[1]);os.chmod(sys.argv[1],0o600);server.listen();print('ready',flush=True)\nwhile True:\n stream,_=server.accept();assert stream.recv(1)==b'';stream.close()\n";
    let mut child = Command::new("python3").args(["-B", "-c", script]).arg(&socket)
        .stdout(Stdio::piped()).spawn()?;
    let result = (|| {
        let mut line = String::new();
        BufReader::new(child.stdout.take().context("缺少子进程输出")?).read_line(&mut line)?;
        ensure!(line == "ready\n", "测试后台未就绪");
        let listener = UnixListener::bind(directory.path().join("owned.sock"))?;
        let config = crate::config::Config {
            desktop_socket: socket,
            local_authority: Some(LocalAuthority::owned_listener(&listener)?),
            ..crate::config::Config::default()
        };
        for role in [Role::Reader, Role::NativeReader, Role::DraftWriter,
            Role::TitleWriter, Role::ArchiveWriter, Role::RemoteObserver] {
            let error = match DesktopConnection::from_config(&config, Duration::from_secs(1), role) {
                Err(error) => error,
                Ok(_) => bail!("错误后台被接受"),
            };
            ensure!(error.to_string().contains("不是启动本看板"), "{error:#}");
            ensure!(child.try_wait()?.is_none(), "错误后台收到了 RPC 数据");
        }
        Ok(())
    })();
    child.kill()?;
    child.wait()?;
    result
}

#[test]
fn unloaded_or_unknown_history_projection_cannot_create_blocked_runtime() {
    for kind in ["notLoaded", "newStatus", ""] {
        for phase in ["inProgress", "interrupted", "failed"] {
            assert!(crate::model::runtime_state(
                &serde_json::json!({"type":kind}),
                &serde_json::json!({"id":"new-task","status":phase,"completedAt":null}),
            ).is_none(), "{kind}/{phase}");
        }
    }
}

#[test]
fn genuine_terminal_evidence_survives_unloading() {
    let unloaded = serde_json::json!({"type":"notLoaded"});
    for (phase, expected) in [("completed", "review"), ("failed", "blocked"), ("interrupted", "blocked")] {
        let turn = serde_json::json!({"id":"turn-1","status":phase,"completedAt":10.0});
        assert_eq!(crate::model::runtime_state(&unloaded, &turn).map(|state| state.0), Some(expected));
    }
    assert_eq!(crate::model::runtime_state(&unloaded,
        &serde_json::json!({"id":"turn-1","status":"completed"})).map(|state| state.0), Some("review"));
    for kind in ["newStatus", ""] {
        assert!(crate::model::runtime_state(&serde_json::json!({"type":kind}),
            &serde_json::json!({"id":"turn-1","status":"completed","completedAt":10.0})).is_none());
    }
}
