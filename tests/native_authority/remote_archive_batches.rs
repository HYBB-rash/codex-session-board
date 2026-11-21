use super::*;
use codex_session_board::remote::hosts::Host;
use std::path::Path;

#[path = "ten_card_archive.rs"]
mod ten_cards;
#[path = "remote_archive_window.rs"]
mod window;

struct BatchFixture {
    live: Option<Live>,
    remotes: RemoteFixture,
    server: Server,
    client: Client,
    batch: String,
    target: String,
}

impl BatchFixture {
    fn new(directory: &Path) -> Result<Self> {
        Self::with_surface(directory, |_, _| {})
    }

    fn with_surface(directory: &Path, seed: impl FnOnce(&Server, &RemoteFixture)) -> Result<Self> {
        let mut server = Server::new(directory)?;
        let remotes = RemoteFixture::new(&mut server, directory)?;
        for host in ["mac", "other"] {
            remotes.surface(host).lock().unwrap().archive_account = Some("original".into());
        }
        seed(&server, &remotes);
        let live = launch(&mut server, directory, &remotes, 60)?;
        let client = Client::builder().no_proxy().timeout(Duration::from_secs(8)).build()?;
        live.ready(&client)?;
        let snapshot = wait_remote(&live, &client, true, None)?;
        let target = identity("mac", FIRST);
        live.post_path(&client, "/api/state", &update(row(&snapshot, &target),
            "to_archive", "目标备注"))?.error_for_status()?;
        let snapshot = live.snapshot(&client)?;
        let mut item = update(row(&snapshot, &target), "to_archive", "");
        item["title"] = json!("远程归档");
        let batch = uuid::Uuid::new_v4().to_string();
        live.post_path(&client, "/api/archive-batches",
            &json!({"id":batch,"items":[item]}))?.error_for_status()?;
        Ok(Self { live: Some(live), remotes, server, client, batch, target })
    }

    fn post(&self, path: &str) -> Result<Value> {
        Ok(self.live.as_ref().unwrap().post_path(&self.client, path,
            &json!({"id":self.target,"batchId":self.batch}))?.error_for_status()?.json()?)
    }

    fn check(&self) -> Result<String> {
        let reply: Value = self.live.as_ref().unwrap().post_path(&self.client,
            "/api/archive-batches/check", &json!({"id":self.batch}))?
            .error_for_status()?.json()?;
        Ok(reply["batch"]["items"][0]["status"].as_str().unwrap().into())
    }

    fn store(&self, host: &str) -> Result<Store> {
        let directory = if host == "local" {
            self.server.config.data_dir.clone()
        } else {
            let host: Host = serde_json::from_value(
                json!({"hostId":host,"displayName":host,"alias":host}))?;
            host.directory(&self.server.config)
        };
        Store::open(&directory, false)
    }

    fn status(&self) -> Result<String> {
        Ok(self.store("local")?.connection()?.query_row(
            "SELECT status FROM archive_batch_items WHERE batch_id=? AND id=?",
            [&self.batch, &self.target], |row| row.get(0))?)
    }

    fn writes(&self, host: &str) -> usize {
        self.remotes.surface(host).lock().unwrap().calls.iter()
            .filter(|method| *method == "thread/archive").count()
    }

    fn submit_losing_receipt(&self) -> Result<()> {
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let gate = Gate(Arc::clone(&release));
        let (started, received) = mpsc::channel();
        {
            let surface = self.remotes.surface("mac");
            let mut surface = surface.lock().unwrap();
            surface.lose_archive_receipt = true;
            surface.read_blocked = Some(BlockingMove {
                identifier: FIRST.into(), started, release,
            });
        }
        std::thread::scope(|scope| -> Result<()> {
            let request = scope.spawn(|| self.post("/api/archive"));
            received.recv_timeout(Duration::from_secs(5))?;
            assert_eq!(self.status()?, "pending");
            self.assert_owners(false)?;
            assert_eq!(self.writes("mac"), 0);
            eprintln!("C1_TRACE before-native-write root=pending mac.session=active other.session=active mac.native_writes=0");
            drop(gate);
            assert_eq!(request.join().unwrap()?["item"]["status"], "pending");
            Ok(())
        })?;
        self.remotes.surface("mac").lock().unwrap().read_blocked = None;
        Ok(())
    }

    fn assert_owners(&self, archived: bool) -> Result<()> {
        for host in ["local", "mac", "other"] {
            let store = self.store(host)?;
            let batches: i64 = store.connection()?.query_row(
                "SELECT COUNT(*) FROM archive_batch_items", [], |row| row.get(0))?;
            assert_eq!(batches, i64::from(host == "local"), "{host}: batch owner");
            assert_eq!(store.archived_ids()?.contains(&FIRST.to_owned()),
                host == "mac" && archived, "{host}: session owner");
            assert_eq!(self.writes("other"), 0);
        }
        assert_eq!(self.store("mac")?.note(FIRST)?, "目标备注");
        assert_eq!(self.store("other")?.note(FIRST)?, "");
        assert_eq!(self.store("local")?.note(FIRST)?, "");
        assert!(!self.server.surface.lock().unwrap().calls.iter()
            .any(|method| method == "thread/archive"));
        Ok(())
    }

    fn reopen(&mut self, directory: &Path) -> Result<()> {
        drop(self.live.take());
        self.live = Some(Live::new(&mut self.server.config, directory)?);
        self.live.as_ref().unwrap().ready(&self.client)?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let snapshot = self.live.as_ref().unwrap().snapshot(&self.client)?;
            if snapshot["health"]["remoteConnections"].as_array().is_some_and(|hosts|
                hosts.len() == 2 && hosts.iter().all(|host| host["runtimeVerified"] == true)) {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "重开后远程主机未就绪");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn isolated(directory: &Path, test: &str) -> Result<bool> {
    if std::env::var_os("CSB_REMOTE_ARCHIVE_FIXTURE").is_some() {
        return Ok(false);
    }
    let result = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", &format!("remote_isolation::archive_batches::{test}"), "--nocapture"])
        .env("CSB_REMOTE_ARCHIVE_FIXTURE", "1").env("CODEX_HOME", directory).output()?;
    eprintln!("{}", String::from_utf8_lossy(&result.stderr));
    anyhow::ensure!(result.status.success(), "远程归档 fixture 失败: {test}\n{}",
        String::from_utf8_lossy(&result.stdout));
    Ok(true)
}

#[test]
fn offline_panel_confirms_new_input_and_archives_only_the_ready_host() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated(directory.path(), "offline_panel_confirms_new_input_and_archives_only_the_ready_host")? {
        return Ok(());
    }
    let fixture = BatchFixture::new(directory.path())?;
    let local = fixture.store("local")?;
    local.discover_remote("offline", "离线主机", &[json!({"id":FIRST,"hostId":"offline","title":"缓存","updatedAt":1})])?;
    local.observe(SECOND, &json!({"id":"turn","userItems":[]}), None)?;
    let live = fixture.live.as_ref().unwrap();
    let snapshot = live.snapshot(&fixture.client)?;
    live.post_path(&fixture.client, "/api/state", &update(row(&snapshot, SECOND), "to_archive", "复核"))?
        .error_for_status()?;
    local.observe(SECOND, &json!({"id":"turn","userItems":["input"]}), None)?;
    {
        let mut native = fixture.server.surface.lock().unwrap();
        native.preview_revision = Some(json!({"id":"turn","status":"completed"}));
        native.latest_turn = Some(json!({"id":"turn","status":"completed","items":[
            {"id":"input","type":"userMessage"},{"type":"agentMessage","phase":"final_answer","text":"归档前核对"}]}));
    }
    live.check_panel(&[&fixture.target, SECOND])?;
    assert_eq!(fixture.writes("mac"), 1);
    assert_eq!(fixture.writes("other"), 0);
    assert!(!fixture.server.surface.lock().unwrap().calls.iter().any(|method| method == "thread/archive"));
    let after = live.snapshot(&fixture.client)?;
    assert_eq!(row(&after, SECOND)["followUp"], false);
    assert!(fixture.remotes.archived("mac").contains(FIRST));
    Ok(())
}

#[test]
fn remote_batch_owner_persists_a_success_receipt_in_root_and_confirms_only_the_target() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated(directory.path(),
        "remote_batch_owner_persists_a_success_receipt_in_root_and_confirms_only_the_target")? {
        return Ok(());
    }
    let mut fixture = BatchFixture::new(directory.path())?;
    fixture.assert_owners(false)?;
    assert_eq!(fixture.post("/api/archive")?["item"]["status"], "archived");
    assert_eq!(fixture.status()?, "archived");
    fixture.assert_owners(true)?;
    fixture.reopen(directory.path())?;
    assert_eq!(fixture.post("/api/archive")?["item"]["status"], "archived");
    assert_eq!(fixture.check()?, "archived");
    fixture.assert_owners(true)?;
    assert_eq!(fixture.writes("mac"), 1);
    eprintln!("C1_TRACE receipt root=archived mac.session=archived other.session=active mac.native_writes=1");
    drop(fixture);
    window::after_tombstone_failure(directory.path(),false)
}

#[test]
fn remote_batch_owner_recovers_a_lost_receipt_once_and_preserves_same_uuid_on_other_hosts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated(directory.path(),
        "remote_batch_owner_recovers_a_lost_receipt_once_and_preserves_same_uuid_on_other_hosts")? {
        return Ok(());
    }
    let mut fixture = BatchFixture::new(directory.path())?;
    fixture.assert_owners(false)?;
    assert_eq!(fixture.status()?, "awaiting");
    fixture.submit_losing_receipt()?;
    assert_eq!(fixture.status()?, "pending");
    assert!(fixture.remotes.archived("mac").contains(FIRST));
    assert_eq!(fixture.writes("mac"), 1);
    fixture.assert_owners(false)?;
    eprintln!("C1_TRACE submit root=pending mac.session=active other.session=active mac.native_writes=1");
    fixture.reopen(directory.path())?;
    let reopened = App::new(fixture.server.config.clone())?;
    let history = reopened.request("/api/archive-batches", None)?;
    assert_eq!(history["batches"][0]["items"][0]["status"], "pending");
    assert_eq!(history["batches"][0]["items"][0]["id"], fixture.target);
    assert_eq!(fixture.post("/api/archive")?["item"]["status"], "pending");
    assert_eq!(fixture.writes("mac"), 1);
    eprintln!("C1_TRACE reopen root=pending mac.native_writes=1");
    assert_eq!(fixture.check()?, "verified");
    assert_eq!(fixture.check()?, "verified");
    fixture.assert_owners(true)?;
    assert_eq!(fixture.writes("mac"), 1);
    eprintln!("C1_TRACE check root=verified mac.session=archived other.session=active mac.native_writes=1");
    drop(fixture);
    window::after_tombstone_failure(directory.path(),true)
}

#[test]
fn remote_batch_owner_persists_new_input_refusal_without_a_native_write() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated(directory.path(),
        "remote_batch_owner_persists_new_input_refusal_without_a_native_write")? {
        return Ok(());
    }
    let mut fixture = BatchFixture::new(directory.path())?;
    fixture.remotes.surface("mac").lock().unwrap().latest_turn = Some(json!({
        "id":"new","status":"completed","items":[{"id":"new-user","type":"userMessage"}]
    }));
    assert_eq!(fixture.post("/api/archive")?["item"]["status"], "newInput");
    assert_eq!(fixture.status()?, "newInput");
    assert_eq!(fixture.check()?, "newInput");
    fixture.reopen(directory.path())?;
    assert_eq!(fixture.post("/api/archive")?["item"]["status"], "newInput");
    fixture.assert_owners(false)?;
    assert_eq!(fixture.writes("mac"), 0);
    eprintln!("C1_TRACE new-input root=newInput mac.native_writes=0");
    Ok(())
}

#[test]
fn remote_batch_owner_keeps_pending_when_the_bound_source_changes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated(directory.path(),
        "remote_batch_owner_keeps_pending_when_the_bound_source_changes")? {
        return Ok(());
    }
    let fixture = BatchFixture::new(directory.path())?;
    fixture.remotes.surface("mac").lock().unwrap().lose_archive_receipt = true;
    assert_eq!(fixture.post("/api/archive")?["item"]["status"], "pending");
    let source: String = fixture.store("local")?.connection()?.query_row(
        "SELECT source FROM archive_batch_items WHERE batch_id=?", [&fixture.batch], |row| row.get(0))?;
    let source: Value = serde_json::from_str(&source)?;
    assert!(source["home"].as_str().unwrap().ends_with("/mac/codex"));
    assert!(source["peer"].as_str().is_some_and(|peer| !peer.is_empty()));
    fixture.remotes.surface("mac").lock().unwrap().archive_account = Some("different".into());
    assert_eq!(fixture.check()?, "pending");
    fixture.assert_owners(false)?;
    assert_eq!(fixture.writes("mac"), 1);
    fixture.remotes.surface("mac").lock().unwrap().archive_account = Some("original".into());
    assert_eq!(fixture.check()?, "verified");
    fixture.assert_owners(true)?;
    assert_eq!(fixture.writes("mac"), 1);
    Ok(())
}
