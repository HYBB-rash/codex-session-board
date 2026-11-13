use super::*;
use crate::{app::App, config::Config, remote::hosts::Host};

const SSH: &str = r#"import base64,hashlib,json,os,pathlib,re,shlex,socket,struct,sys,time
path=pathlib.Path(sys.argv[1])
state=json.loads(path.read_text())
def event(value):
    with open(str(path)+".events","a") as out: out.write(value+"\n")
args=sys.argv[2:]
if "-G" in args:
    event("config")
    if state.get("gDelay"): time.sleep(state["gDelay"])
    sys.stderr.write(state.get("gStderr",""))
    sys.stderr.flush()
    target=state.get("targets",{}).get(args[-1],state)
    sys.stdout.write(state.get("gOutput","hostname "+target["hostname"]+"\nuser "+target["user"]+"\nport "+str(target["port"])+"\nidentityfile NOT-PERSISTED\n"))
    sys.stdout.flush()
    sys.exit(state.get("gStatus",0))
if "-M" in args:
    event("master")
    if state.get("connectDelay"): time.sleep(state["connectDelay"])
    if state.get("masterFailure"): sys.exit(1)
    endpoint=next(value.split("=",1)[1] for value in args if value.startswith("ControlPath="))
    listener=socket.socket(socket.AF_UNIX); listener.bind(endpoint); listener.listen()
    while True: time.sleep(1)
if state.get("connectDelay") and not any(value.startswith("ControlPath=") for value in args):
    time.sleep(state["connectDelay"])
payload=shlex.split(args[-1])[-1]
if "def compact(payload):" in payload: event("compact")
marker=re.findall(r"[0-9a-f]{32}",payload)[-1]
mode="proxy" if "exec codex app-server proxy" in payload else "clock" if "import time;" in payload else "probe"
event(mode)
if state.get("delay"): time.sleep(state["delay"])
peer=state.get("peerByTarget",{}).get(args[-2],state["peer"])
if mode=="clock": peer=state.get("clockPeer",peer)
if state.get("wrongMarker"): marker="z"*32
out=sys.stdout.buffer
out.write(state.get("banner","profile banner\n").encode()+marker.encode()+b"\n")
out.write(json.dumps(peer).encode()+b"\n")
out.flush()
if mode=="clock":
    out.write((str(time.time())+"\n").encode()); out.flush(); sys.exit()
if mode=="probe": sys.exit()
inp=sys.stdin.buffer
header=b""
while not header.endswith(b"\r\n\r\n"):
    piece=inp.read(1)
    if not piece: sys.exit()
    header+=piece
key=re.search(br"Sec-WebSocket-Key:\s*([^\r]+)",header,re.I).group(1)
accept=base64.b64encode(hashlib.sha1(key+b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest())
out.write(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: "+accept+b"\r\n\r\n"); out.flush()
while True:
    first=inp.read(2)
    if len(first)!=2 or first[0]&15==8: break
    size=first[1]&127
    if size==126: size=struct.unpack("!H",inp.read(2))[0]
    if size==127: size=struct.unpack("!Q",inp.read(8))[0]
    mask=inp.read(4) if first[1]&128 else None
    data=inp.read(size)
    if mask: data=bytes(value^mask[index%4] for index,value in enumerate(data))
    message=json.loads(data)
    method=message["method"]; event(method)
    if "id" not in message: continue
    result={"codexHome":state.get("initializeHome",state["peer"]["declaredCodexHome"])} if method=="initialize" else state.get("result",{"ok":True})
    data=json.dumps({"id":message["id"],"result":result}).encode()
    prefix=bytes([129,len(data)]) if len(data)<126 else bytes([129,126])+struct.pack("!H",len(data))
    out.write(prefix+data); out.flush()
"#;

pub(crate) struct Fixture {
    _directory: tempfile::TempDir,
    pub config: Config,
    pub host: Host,
    path: PathBuf,
}

impl Fixture {
    pub(crate) fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let script = directory.path().join("ssh.py");
        std::fs::write(&script, SSH)?;
        let path = directory.path().join("peer.json");
        crate::config::write_json(
            &path,
            &json!({
                "hostname":"mac.example","user":"remote","port":22,
                "peer":{"uid":501,"home":"/Users/remote","codexHome":"/Users/remote/.codex",
                    "declaredCodexHome":"/Users/remote/.codex"}
            }),
            false,
        )?;
        let config = Config {
            data_dir: directory.path().join("data"),
            sync_titles: false,
            rpc_timeout_seconds: 3,
            ssh_command: vec![
                "python3".into(),
                "-B".into(),
                script.to_string_lossy().into(),
                path.to_string_lossy().into(),
            ],
            ..Config::default()
        };
        let host = serde_json::from_value(
            json!({"hostId":"remote:test","displayName":"test","alias":"example"}),
        )?;
        Ok(Self {
            _directory: directory,
            config,
            host,
            path,
        })
    }

    pub(crate) fn update(&self, key: &str, value: Value) -> Result<()> {
        let mut state: Value = serde_json::from_slice(&std::fs::read(&self.path)?)?;
        state[key] = value;
        crate::config::write_json(&self.path, &state, true)
    }

    pub(crate) fn peer(&self) -> Result<Peer> {
        let state: Value = serde_json::from_slice(&std::fs::read(&self.path)?)?;
        Ok(serde_json::from_value(state["peer"].clone())?)
    }

    pub(crate) fn binding(&self) -> Result<Binding> {
        Binding::prepare(&self.host, &self.config)
    }

    pub(crate) fn events(&self) -> String {
        std::fs::read_to_string(self.path.with_extension("json.events")).unwrap_or_default()
    }

    pub(crate) fn app(&self) -> Result<App> {
        let mut config = self.config.clone();
        config.host_id = self.host.host_id.clone();
        config.remote_binding = Some(self.binding()?);
        config.data_dir = self.host.directory(&self.config);
        config.proxy_command = self.host.proxy(&self.config)?;
        config.clock_command = self.host.command(&self.config, framing::CLOCK)?;
        App::new(config)
    }
}
