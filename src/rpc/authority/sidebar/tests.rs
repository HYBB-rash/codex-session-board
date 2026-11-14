use super::*;
use crate::rpc::ManagedChild;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

fn peer(root: &Path, name: &str) -> Result<(ManagedChild, UnixStream)> {
    let socket = root.join(name);
    let script = "import os,socket,sys\nserver=socket.socket(socket.AF_UNIX);server.bind(sys.argv[1]);server.listen();print('ready',flush=True)\nwhile True:\n stream,_=server.accept();assert stream.recv(1)==b'';stream.close()\n";
    let mut process = ManagedChild(Command::new("python3").args(["-B", "-c", script]).arg(&socket)
        .stdout(Stdio::piped()).spawn()?);
    let mut line = String::new();
    BufReader::new(process.0.stdout.take().context("缺少 fixture 输出")?).read_line(&mut line)?;
    ensure!(line == "ready\n", "侧栏 fixture 未就绪");
    Ok((process, UnixStream::connect(socket)?))
}

#[test]
fn shared_sibling_client_requires_explicit_backend_client_and_launcher_epochs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let (mut backend_process, backend_socket) = peer(directory.path(), "backend")?;
    let (mut client_process, client_socket) = peer(directory.path(), "client")?;
    let (mut other_process, other_socket) = peer(directory.path(), "other")?;
    let backend = LocalAuthority::capture(backend_process.0.id())?;
    let client = LocalAuthority::capture(client_process.0.id())?;
    let launcher = LocalAuthority::capture(std::process::id())?;
    let original = serde_json::json!({"backend":backend,"client":client,"launcher":launcher});
    let path = directory.path().join("proof.json");
    std::fs::write(&path, serde_json::to_vec(&original)?)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    verify(&backend, &client_socket, Some(&path))?;
    assert!(verify(&backend, &other_socket, Some(&path)).is_err());
    assert!(verify(&backend, &client_socket, None).is_err());
    verify(&backend, &backend_socket, None)?;
    for subject in ["backend", "client", "launcher"] {
        let mut stale = original.clone();
        stale[subject]["started"] = serde_json::json!(
            stale[subject]["started"].as_u64().context("缺少 epoch")?.wrapping_add(1));
        std::fs::write(&path, serde_json::to_vec(&stale)?)?;
        assert!(verify(&backend, &client_socket, Some(&path)).is_err(), "{subject}");
    }
    std::fs::write(&path, serde_json::to_vec(&original)?)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    assert!(verify(&backend, &client_socket, Some(&path)).is_err());
    assert!(other_process.0.try_wait()?.is_none());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    client_process.0.kill()?;
    client_process.0.wait()?;
    std::fs::remove_file(directory.path().join("client"))?;
    let (replacement, replacement_socket) = peer(directory.path(), "client")?;
    assert!(verify(&backend, &replacement_socket, Some(&path)).is_err());
    let mut renewed = original;
    renewed["client"] = serde_json::to_value(LocalAuthority::capture(replacement.0.id())?)?;
    std::fs::write(&path, serde_json::to_vec(&renewed)?)?;
    verify(&backend, &replacement_socket, Some(&path))?;
    backend_process.0.kill()?;
    backend_process.0.wait()?;
    assert!(verify(&backend, &replacement_socket, Some(&path)).is_err());
    drop((backend_socket, client_socket, other_socket));
    Ok(())
}
