use super::*;
use super::transport::{Server, body};
use anyhow::{Context, ensure};
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct OtherClient(Child);

impl Drop for OtherClient {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn active_backend_b_never_writes_sections_or_projects_to_same_user_client_a() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut backend = Server::new(directory.path())?;
    let socket = directory.path().join("wrong-client.sock");
    let events = directory.path().join("wrong-client-events.jsonl");
    let script = "import os,socket,sys\nserver=socket.socket(socket.AF_UNIX);server.bind(sys.argv[1]);os.chmod(sys.argv[1],0o600);server.listen();print('ready',flush=True)\nwhile True:\n stream,_=server.accept();data=stream.recv(1);stream.close()\n with open(sys.argv[2],'a') as out:out.write(str(len(data))+'\\n')\n";
    let mut other = OtherClient(Command::new("python3").args(["-B", "-c", script])
        .arg(&socket).arg(&events).stdout(Stdio::piped()).spawn()?);
    let mut line = String::new();
    BufReader::new(other.0.stdout.take().context("缺少其他客户端输出")?).read_line(&mut line)?;
    ensure!(line == "ready\n", "其他客户端未就绪");
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    backend.config.sidebar_socket = socket;
    let app = backend.app()?;
    assert_eq!(app.snapshot()?["health"]["nativeVerified"], true);
    let before = app.snapshot()?["sessions"][0].clone();
    let error = app.request("/api/state", Some(&body(&app, "blocked", "不能发送")?))
        .err().context("错误侧栏接受了原生状态写入")?;
    ensure!(format!("{error:#}").contains("侧栏不属于"), "{error:#}");
    assert_eq!(app.snapshot()?["sessions"][0], before);
    let error = codex_session_board::rpc::SidebarConnection::project_writer(
        &app.config, Duration::from_secs(2))
        .err().context("错误侧栏接受了项目写入连接")?;
    ensure!(format!("{error:#}").contains("侧栏不属于"), "{error:#}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observed = std::fs::read_to_string(&events).unwrap_or_default();
        if observed.lines().count() == 2 {
            assert_eq!(observed, "0\n0\n");
            break;
        }
        ensure!(Instant::now() < deadline, "未确认侧栏收到零字节：{observed}");
        std::thread::sleep(Duration::from_millis(20));
    }
    ensure!(other.0.try_wait()?.is_none(), "错误客户端意外退出");
    Ok(())
}
