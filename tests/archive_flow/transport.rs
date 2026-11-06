use super::*;
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::time::{Duration, Instant};

#[test]
fn archive_endpoint_uses_the_isolated_desktop_writer_and_hides_only_after_success() -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let socket = directory.path().join("desktop.sock");
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let server = std::thread::spawn(move || serve_desktop(listener));
    let app = App::new(Config {
        data_dir: directory.path().join("data"),
        desktop_socket: socket,
        sync_titles: false,
        ..Config::default()
    })?;
    populate(&app.store, "to_archive")?;
    let result = app.request(
        "/api/archive",
        Some(&json!({"id":FIRST,"version":1,"inputVersion":0})),
    )?;
    assert_eq!(result, json!({"ok":true,"archived":true}));
    assert!(app.snapshot()?["sessions"].as_array().unwrap().is_empty());
    assert_eq!(
        server.join().unwrap()?,
        [
            "initialize",
            "thread/list",
            "thread/read",
            "thread/turns/list",
            "thread/archive"
        ]
    );
    Ok(())
}

fn accept_desktop(listener: UnixListener) -> Result<UnixStream> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn serve_desktop(listener: UnixListener) -> Result<Vec<String>> {
    let stream = accept_desktop(listener)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut connection = tungstenite::accept(stream)?;
    let mut methods = Vec::new();
    loop {
        let message = connection.read()?;
        if message.is_close() {
            break;
        }
        let request: Value = serde_json::from_str(message.to_text()?)?;
        let Some(identifier) = request.get("id") else {
            continue;
        };
        let method = request["method"].as_str().unwrap();
        let result = match method {
            "initialize" | "thread/archive" => json!({}),
            "thread/list" => {
                assert_eq!(request["params"]["archived"], true);
                json!({"data":[]})
            }
            "thread/read" => json!({"thread":{"id":FIRST,"status":{"type":"idle"}}}),
            "thread/turns/list" => json!({"data":[]}),
            _ => bail!("unexpected method {method}"),
        };
        if method == "thread/archive" {
            assert_eq!(request["params"], json!({"threadId":FIRST}));
        }
        methods.push(method.to_string());
        connection.send(tungstenite::Message::Text(
            json!({"id":identifier,"result":result}).to_string().into(),
        ))?;
    }
    Ok(methods)
}
