use super::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct Surface {
    pub threads: HashMap<String, Value>,
    pub sections: HashMap<String, Value>,
    pub logical: Vec<(String, String)>,
    pub backend: HashMap<String, String>,
    pub placements: HashMap<String, String>,
    pub calls: Vec<String>,
    pub lose_start: bool,
    pub lose_name: bool,
    pub lose_move: bool,
    pub reject_archive: bool,
    pub cross_read: bool,
    pub unmaterialized: bool,
    pub changing_read_times: bool,
    pending_read: Option<(String, Value)>,
}

pub(super) struct Fixture {
    pub _directory: tempfile::TempDir,
    pub config: Config,
    pub store: Store,
    pub surface: Arc<Mutex<Surface>>,
}

pub(super) struct Mock(pub Arc<Mutex<Surface>>, pub bool);

impl Fixture {
    pub fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let config = Config {host_id:"remote:a".into(),rpc_timeout_seconds:0,
            data_dir:directory.path().join("board"),
            sidebar_state_file:directory.path().join("state.json"),
            status_sections:crate::model::board_states().map(|(state,_)|
                (state.into(),uuid::Uuid::new_v4().to_string())).collect(),..Config::default()};
        let entries: Vec<_> = config.section_bindings().map(|(_,name,id)|
            json!({"id":id,"name":name,"hostSectionIds":{}})).collect();
        std::fs::write(&config.sidebar_state_file,json!({"electron-renderer-content-owner":"owner",
            "electron-owner-atoms:owner":{"sidebar-custom-sections-v3":{
                "account":{"sections":entries}}}}).to_string())?;
        let store = Store::open(&config.data_dir,false)?;
        let logical = config.section_bindings().map(|(_,name,id)| (id.into(),name.into())).collect();
        Ok(Self {_directory:directory,config,store,surface:Arc::new(Mutex::new(
            Surface {logical,..Surface::default()}))})
    }

    pub fn run(&self) -> Result<mapping::Record> {
        let mut seed = mapping::seed(&self.config)?;
        seed.scope["source"] = json!({"peer":{"home":"/remote"}});
        let mut record = self.store.native_mapping(&self.config.host_id)?
            .unwrap_or(mapping::Record {scope:seed.scope,logical:seed.logical,probe:None});
        let sections = self.surface.lock().unwrap().sections.clone();
        record.logical = mapping::usable(&self.config,&sections,record.logical);
        let mut backend = Mock(Arc::clone(&self.surface),false);
        let mut sidebar = Mock(Arc::clone(&self.surface),true);
        Calibration {config:&self.config,store:&self.store,record:&mut record,
            backend:&mut backend,sidebar:&mut sidebar}.complete(sections)?;
        Ok(record)
    }

    pub fn record(&self) -> Result<mapping::Record> {
        self.store.native_mapping(&self.config.host_id)?.context("missing record")
    }

    pub fn calls(&self, method: &str) -> usize {
        self.surface.lock().unwrap().calls.iter().filter(|call| *call == method).count()
    }
}

impl Surface {
    fn start(&mut self, params: Value) -> Result<Value> {
        assert_eq!(params,json!({"cwd":"/remote","ephemeral":false,"historyMode":"paginated"}));
        let id = uuid::Uuid::new_v4().to_string();
        let thread = json!({"id":id,"name":null,"cwd":"/remote","historyMode":params["historyMode"],
            "status":{"type":"notLoaded"},"turns":[],"section":null,"archived":false});
        self.threads.insert(id,thread.clone());
        anyhow::ensure!(!std::mem::take(&mut self.lose_start),"lost start receipt");
        Ok(json!({"thread":thread}))
    }

    fn backend_call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "thread/start" => self.start(params),
            "thread/read" => self.read_thread(params),
            "thread/turns/list" => self.list_turns(params),
            "thread/name/set" => {
                self.threads.get_mut(params["threadId"].as_str().unwrap()).unwrap()["name"]
                    = params["name"].clone();
                anyhow::ensure!(!std::mem::take(&mut self.lose_name),"lost name receipt");
                Ok(json!({}))
            }
            "thread/archive" => {
                anyhow::ensure!(!self.reject_archive,"archive disconnected");
                self.threads.get_mut(params["threadId"].as_str().unwrap()).unwrap()["archived"]
                    = json!(true);
                Ok(json!({}))
            }
            "threadSection/list" => Ok(json!({"data":self.sections.values().collect::<Vec<_>>()})),
            "thread/list" => {
                Ok(json!({"data":self.threads.values().filter(|thread|
                    (thread["archived"] == true) == (params["archived"] == true))
                    .collect::<Vec<_>>()}))
            }
            _ => anyhow::bail!("unexpected backend write: {method}"),
        }
    }

    fn list_turns(&self, params: Value) -> Result<Value> {
        assert_eq!(params["itemsView"],"summary");
        assert_eq!(params["limit"],1);
        let id = params["threadId"].as_str().unwrap();
        let turns = &self.threads[id]["turns"];
        if self.unmaterialized && turns == &json!([]) {
            return Err(crate::rpc::RequestError {method:"thread/turns/list".into(),details:json!({
                "code":-32600,"message":format!("thread {id} is not materialized yet; thread/turns/list is unavailable before first user message")})}.into());
        }
        Ok(json!({"data":turns}))
    }

    fn read_thread(&self, params: Value) -> Result<Value> {
        let id = params["threadId"].as_str().unwrap();
        let thread = self.threads.get(id).ok_or_else(|| crate::rpc::RequestError {
            method:"thread/read".into(),details:json!({"code":-32600,"message":format!("thread not loaded: {id}")})})?;
        if params["includeTurns"] == true && thread["historyMode"] == "paginated" {
            return Err(crate::rpc::RequestError {method:"thread/read".into(),
                details:json!({"code":-32603,"message":"list_turns is not supported yet"})}.into());
        }
        if params["includeTurns"] == true && thread["historyMode"] == "legacy" && thread["turns"] == json!([]) {
            return Err(crate::rpc::RequestError {method:"thread/read".into(),details:json!({
                "code":-32600,"message":format!("thread {id} is not materialized yet; includeTurns is unavailable before first user message")})}.into());
        }
        let mut thread = thread.clone();
        if params["includeTurns"] == false { thread["turns"] = json!([]); }
        if self.changing_read_times {
            for key in ["createdAt","updatedAt","recencyAt"] { thread[key] = json!(self.calls.len()); }
        }
        Ok(json!({"thread":thread}))
    }

    fn move_thread(&mut self, params: Value) -> Result<Value> {
        assert_eq!(params["hostId"],"remote:a");
        assert_eq!(params["source"],"codex");
        let target = params["sectionId"].as_str().unwrap();
        let name = self.logical.iter().find(|(id,_)| id == target).unwrap().1.clone();
        let backend = self.backend.entry(target.into()).or_insert_with(||
            uuid::Uuid::new_v4().to_string()).clone();
        let actual = json!({"id":backend,"name":name,"appearance":null});
        self.sections.insert(backend,actual.clone());
        let id = params["threadId"].as_str().unwrap();
        self.placements.insert(id.into(),target.into());
        self.threads.get_mut(id).unwrap()["section"] = actual.clone();
        if std::mem::take(&mut self.cross_read) {
            let foreign = json!({"id":"foreign-same-name","name":name,"appearance":null});
            self.sections.insert("foreign-same-name".into(),foreign.clone());
            self.threads.get_mut(id).unwrap()["section"] = foreign;
            self.pending_read = Some((id.into(),actual));
        }
        anyhow::ensure!(!std::mem::take(&mut self.lose_move),"lost move receipt");
        Ok(json!({}))
    }

    fn sidebar_call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "move_thread_to_sidebar_section" => self.move_thread(params),
            "list_threads" => {
                let sections: Vec<_> = self.logical.iter().map(|(target,name)| {
                    let keys: Vec<_> = self.placements.iter().filter(|(_,id)| *id == target)
                        .map(|(id,_)| format!("codex:thread:local:{id}")).collect();
                    json!({"sectionId":target,"name":name,"itemKeys":keys})
                }).collect();
                if let Some((id,actual)) = self.pending_read.take() {
                    self.threads.get_mut(&id).unwrap()["section"] = actual;
                }
                Ok(json!({"sections":sections,"unavailableHosts":[]}))
            }
            _ => anyhow::bail!("unexpected sidebar write: {method}"),
        }
    }
}

impl Rpc for Mock {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let mut surface = self.0.lock().unwrap();
        surface.calls.push(method.into());
        if self.1 {surface.sidebar_call(method,params)} else {surface.backend_call(method,params)}
    }
}
