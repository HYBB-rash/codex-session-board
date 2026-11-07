use super::super::{
    archive::Engine,
    transport::{Server, Surface, section_id},
};
use super::{FIRST, SECOND};
use anyhow::Result;
use codex_session_board::{
    config::Config,
    remote::hosts::Host,
    rpc::{DesktopConnection, Role, Rpc},
    store::Store,
};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HOSTS: [&str; 2] = ["mac", "other"];
const SSH: &str = r#"import json,os,pathlib,re,socket,struct,sys,threading,time
root=pathlib.Path(sys.argv[1])
args=sys.argv[2:]
target=args[-1] if "-G" in args else args[-2]
assert target in ("mac","other")
def event(phase):
    with open(root/"events","a") as out: out.write(target+" "+phase+"\n")
if "-G" in args:
    event("config")
    print("hostname "+target+".fixture.invalid\nuser remote\nport 22",flush=True)
    sys.exit()
operation=args[-1]
mode="proxy" if "app-server proxy" in operation else "clock" if "import time" in operation else "probe"
if (root/"pause").exists():
    event(mode+" paused")
    sys.exit(75)
markers=re.findall(r"(?<![0-9a-f])[0-9a-f]{32}(?![0-9a-f])",operation)
home=str(root/target/"home")
codex_home=str(root/target/"codex")
if markers:
    peer={"uid":501,"home":home,"codexHome":codex_home,"declaredCodexHome":codex_home}
    print(markers[-1]+"\n"+json.dumps(peer),flush=True)
event(mode+(" framed" if markers else " legacy"))
if mode=="clock":
    print(time.time(),flush=True)
    sys.exit()
if mode=="probe": sys.exit()
stream=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
stream.connect(str(root/(target+".sock")))
def upstream():
    try:
        while True:
            data=os.read(0,65536)
            if not data: break
            stream.sendall(data)
        stream.shutdown(socket.SHUT_WR)
    except OSError: pass
threading.Thread(target=upstream,daemon=True).start()
def receive(length):
    data=b""
    while len(data)<length:
        part=stream.recv(length-len(data))
        if not part: raise EOFError("engine closed")
        data+=part
    return data
out=sys.stdout.buffer
header=b""
while not header.endswith(b"\r\n\r\n"): header+=receive(1)
out.write(header); out.flush()
prefix=receive(2)
assert prefix[0]==129 and not prefix[1]&128
size=prefix[1]&127
if size==126: size=struct.unpack("!H",receive(2))[0]
if size==127: size=struct.unpack("!Q",receive(8))[0]
response=json.loads(receive(size))
response["result"]["codexHome"]=codex_home
data=json.dumps(response).encode()
prefix=bytes([129,len(data)]) if len(data)<126 else bytes([129,126])+struct.pack("!H",len(data))
out.write(prefix+data); out.flush()
while True:
    data=stream.recv(65536)
    if not data: break
    out.write(data); out.flush()
"#;

pub(super) struct RemoteFixture {
    directory: PathBuf,
    surfaces: Vec<Arc<Mutex<Surface>>>,
    engines: Vec<Engine>,
}

pub(super) struct Pause(PathBuf);

impl Drop for Pause {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl RemoteFixture {
    pub(super) fn new(server: &mut Server, directory: &Path) -> Result<Self> {
        let directory = directory.join("remote-proxies");
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        fs::write(directory.join("ssh.py"), SSH)?;
        let mut surfaces = Vec::new();
        let mut engines = Vec::new();
        for host in HOSTS {
            let surface = Arc::new(Mutex::new(Surface::default()));
            surface.lock().unwrap().slots.extend([
                (FIRST.into(), "blocked".into()),
                (SECOND.into(), "unknown".into()),
            ]);
            engines.push(Engine::new(
                &directory.join(format!("{host}.sock")),
                Arc::clone(&surface),
            )?);
            surfaces.push(surface);
        }
        let fixture = Self {
            directory,
            surfaces,
            engines,
        };
        server.config.ssh_command = fixture.command();
        fixture.register(&server.config)?;
        fixture.check_legacy_proxy()?;
        Ok(fixture)
    }

    fn command(&self) -> Vec<String> {
        vec![
            "python3".into(),
            "-B".into(),
            self.directory.join("ssh.py").to_string_lossy().into(),
            self.directory.to_string_lossy().into(),
        ]
    }

    fn register(&self, config: &Config) -> Result<()> {
        let mut state: Value = serde_json::from_slice(&fs::read(&config.sidebar_state_file)?)?;
        state["codex-managed-remote-connections"] =
            json!(HOSTS.map(|host| json!({"hostId":host,"displayName":host,"alias":host})));
        let accounts = state["electron-persisted-atom-state"]["sidebar-custom-sections-v3"]
            .as_object_mut()
            .unwrap();
        for account in accounts.values_mut() {
            for section in account["sections"].as_array_mut().unwrap() {
                for host in HOSTS {
                    section["hostSectionIds"][host] =
                        json!(format!("backend:{}", section["id"].as_str().unwrap()));
                }
            }
        }
        fs::write(&config.sidebar_state_file, serde_json::to_vec(&state)?)?;
        Ok(())
    }

    fn check_legacy_proxy(&self) -> Result<()> {
        for host in HOSTS {
            let mut command = self.command();
            command.extend([
                "--".into(),
                host.into(),
                "exec codex app-server proxy".into(),
            ]);
            let mut connection =
                DesktopConnection::proxy(&command, Duration::from_secs(6), Role::RemoteObserver)?;
            let result = connection.call("thread/list", json!({"archived":false}))?;
            assert_eq!(result["data"].as_array().unwrap().len(), 2);
            assert!(
                result["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|thread| thread["id"] == FIRST
                        && thread["section"]["id"] == format!("backend:{}", section_id("blocked")))
            );
        }
        for surface in &self.surfaces {
            surface.lock().unwrap().calls.clear();
        }
        Ok(())
    }

    pub(super) fn seed_native_cache(&self, config: &Config, source: &Store) -> Result<()> {
        for identifier in HOSTS {
            let host: Host = serde_json::from_value(
                json!({"hostId":identifier,"displayName":identifier,"alias":identifier}),
            )?;
            let directory = host.directory(config);
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&directory)?;
            fs::write(
                directory.join("source.json"),
                serde_json::to_vec(&json!({
                    "schemaVersion":2,"hostId":identifier,
                    "ssh":{"hostname":format!("{identifier}.fixture.invalid"),"user":"remote","port":22},
                    "peer":null
                }))?,
            )?;
            let store = Store::open(&directory, false)?;
            store.import_remote(source, identifier)?;
            for (thread, state) in [(FIRST, "blocked"), (SECOND, "unknown")] {
                store.connection()?.execute(
                    "UPDATE sessions SET native_state=?,native_revision=? WHERE id=?",
                    rusqlite::params![state, section_id(state), thread],
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn pause(&self) -> Result<Pause> {
        let path = self.directory.join("pause");
        drop(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        );
        Ok(Pause(path))
    }

    pub(super) fn assert_managed(&self, config: &Config) -> Result<()> {
        let events = fs::read_to_string(self.directory.join("events"))?;
        for (host, surface) in HOSTS.iter().zip(&self.surfaces) {
            for phase in ["clock framed", "proxy framed"] {
                assert!(
                    events
                        .lines()
                        .any(|event| event == format!("{host} {phase}"))
                );
            }
            let calls = surface.lock().unwrap();
            for method in [
                "initialize",
                "thread/list",
                "threadSection/list",
                "thread/turns/list",
            ] {
                assert!(
                    calls.calls.iter().any(|call| call == method),
                    "{host}: {method}"
                );
            }
            let host: Host =
                serde_json::from_value(json!({"hostId":host,"displayName":host,"alias":host}))?;
            let record: Value =
                serde_json::from_slice(&fs::read(host.directory(config).join("source.json"))?)?;
            assert_eq!(
                record["peer"]["codexHome"],
                self.directory
                    .join(&host.host_id)
                    .join("codex")
                    .to_str()
                    .unwrap()
            );
        }
        Ok(())
    }

    pub(super) fn assert_isolated(&self) -> Result<()> {
        assert!(!self.directory.join("pause").exists());
        for engine in &self.engines {
            assert!(engine.archived.lock().unwrap().is_empty());
        }
        let events = fs::read_to_string(self.directory.join("events"))?;
        assert!(events.lines().any(|event| event == "mac probe paused"));
        Ok(())
    }
}
