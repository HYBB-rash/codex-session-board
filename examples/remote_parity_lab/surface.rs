use super::engine::{HostRpc, Partition, seed};
use anyhow::{Result, bail, ensure};
use codex_session_board::{model, rpc::Rpc};
use serde_json::{Value, json};

pub const THREAD: &str = "11111111-1111-4111-8111-111111111111";

pub struct Surface {
    pub host: String,
    pub title: String,
    pub section: String,
    pub status: Value,
    pub turn: Value,
    pub archived: bool,
    pub fail: Option<&'static str>,
    pub lost: Option<&'static str>,
    pub race_title: Option<String>,
    pub writes: Vec<String>,
}

impl Surface {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.into(),
            title: "完整标题".into(),
            section: "unknown-id".into(),
            status: json!({"type":"idle"}),
            turn: json!({"id":"turn","status":"completed","startedAt":1,"completedAt":2,"items":[]}),
            archived: false,
            fail: None,
            lost: None,
            race_title: None,
            writes: Vec::new(),
        }
    }

    fn sections(&self) -> Value {
        let sections: Vec<_> = model::board_states()
            .chain([("pinned", "Pinned")])
            .map(|(state, label)| {
                let section = format!("{state}-id");
                let items = if self.section == section {
                    vec![format!("codex:thread:{}:{THREAD}", self.host)]
                } else {
                    vec![]
                };
                json!({"sectionId":section,"name":label,"itemKeys":items})
            })
            .collect();
        json!({"sections":sections,"unavailableHosts":[]})
    }

    fn mutate(&mut self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "move_thread_to_sidebar_section" => {
                ensure!(params["hostId"] == self.host, "Wrong target host");
                self.section = params["sectionId"].as_str().unwrap().into();
            }
            "thread/name/set" => {
                if let Some(title) = self.race_title.take() {
                    self.title = title;
                }
                self.title = params["name"].as_str().unwrap().into();
            }
            "thread/archive" => self.archived = true,
            _ => bail!("Unexpected mutation {method}"),
        }
        self.writes.push(method.into());
        if self.lost.take() == Some(method) {
            bail!("Lost acknowledgement");
        }
        Ok(json!({}))
    }
}

impl Rpc for Surface {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        if self.fail == Some(method) {
            self.fail = None;
            bail!("Disconnected");
        }
        if !["list_threads", "thread/list"].contains(&method) {
            ensure!(params["threadId"] == THREAD, "Wrong thread");
        }
        match method {
            "list_threads" => Ok(self.sections()),
            "thread/read" => {
                Ok(json!({"thread":{"id":THREAD,"name":self.title,"status":self.status}}))
            }
            "thread/turns/list" => Ok(json!({"data":[self.turn],"nextCursor":null})),
            "thread/list" => Ok(
                json!({"data":if self.archived {vec![json!({"id":THREAD})]} else {vec![]},"nextCursor":null}),
            ),
            _ => self.mutate(method, &params),
        }
    }
}

impl HostRpc for Surface {
    fn host(&self) -> &str {
        &self.host
    }
}

pub fn fixture(host: &str) -> Result<(tempfile::TempDir, Partition, Surface)> {
    let directory = tempfile::tempdir()?;
    let partition = Partition::open(directory.path(), host)?;
    partition.discover(host, &[seed(host, THREAD)])?;
    let mut surface = Surface::new(host);
    partition.refresh(&mut surface, THREAD)?;
    Ok((directory, partition, surface))
}
