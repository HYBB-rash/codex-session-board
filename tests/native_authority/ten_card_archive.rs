use super::*;
use rusqlite::OptionalExtension;
use serde_json::Value;

struct TenCards {
    fixture: BatchFixture,
    identifiers: Vec<String>,
    targets: Vec<Value>,
}

impl TenCards {
    fn new(directory: &Path) -> Result<Self> {
        let identifiers: Vec<_> = [FIRST.to_owned(), SECOND.to_owned()].into_iter()
            .chain((3..=10).map(|number| format!("00000000-0000-4000-8000-{number:012}")))
            .collect();
        let fixture = BatchFixture::with_surface(directory, |server, remotes| {
            let surface = remotes.surface("mac");
            let mut native = surface.lock().unwrap();
            for identifier in &identifiers {
                native.slots.insert(identifier.clone(), "to_archive".into());
            }
            let mut sidebar = server.surface.lock().unwrap();
            sidebar.remote_threads.extend(identifiers[2..].iter().map(|identifier|
                json!({"id":identifier,"hostId":"mac","title":"十卡归档","updatedAt":1})));
        })?;
        let mut result = Self { fixture, identifiers, targets: Vec::new() };
        result.request("/api/archive-batches/finish", json!({"id":result.fixture.batch}))?;
        result.request("/api/refresh", json!({}))?;
        let snapshot = result.wait_for_cards()?;
        for (position, identifier) in result.identifiers.iter().enumerate() {
            result.request("/api/state", update(row(&snapshot, &identity("mac", identifier)),
                "to_archive", &format!("保留备注 {position}")))?;
        }
        let snapshot = result.fixture.live.as_ref().unwrap().snapshot(&result.fixture.client)?;
        result.targets = result.identifiers.iter().enumerate().map(|(position, identifier)| {
            let mut target = update(row(&snapshot, &identity("mac", identifier)), "to_archive", "");
            target["title"] = json!(format!("十卡 {position}"));
            target
        }).collect();
        result.fixture.batch = uuid::Uuid::new_v4().to_string();
        result.request("/api/archive-batches",
            json!({"id":result.fixture.batch,"items":result.targets}))?;
        result.assert_frozen_targets()?;
        Ok(result)
    }

    fn assert_frozen_targets(&self) -> Result<()> {
        for (record, target) in self.records()?["items"].as_array().unwrap().iter().zip(&self.targets) {
            let frozen: Value = serde_json::from_str(record["target"].as_str().unwrap())?;
            let expected: serde_json::Map<_,_> = ["id","title","version","inputVersion","sectionVersion"]
                .into_iter().map(|field| (field.into(), target[field].clone())).collect();
            assert_eq!(frozen, Value::Object(expected));
        }
        Ok(())
    }

    fn request(&self, path: &str, body: Value) -> Result<Value> {
        Ok(self.fixture.live.as_ref().unwrap().post_path(&self.fixture.client, path, &body)?
            .error_for_status()?.json()?)
    }

    fn wait_for_cards(&self) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let snapshot = self.fixture.live.as_ref().unwrap().snapshot(&self.fixture.client)?;
            if self.identifiers.iter().all(|identifier| snapshot["sessions"].as_array().unwrap()
                .iter().any(|row| row["id"] == identity("mac", identifier)
                    && row["state"] == "to_archive" && row["readOnly"] == false)) {
                return Ok(snapshot);
            }
            anyhow::ensure!(Instant::now() < deadline, "十卡未就绪: {snapshot}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn submit(&self, position: usize) -> Result<String> {
        let response = self.request("/api/archive",
            json!({"batchId":self.fixture.batch,"id":self.targets[position]["id"],
                "version":-1,"inputVersion":-1,"sectionVersion":"重放请求不覆盖冻结目标"}))?;
        Ok(response["item"]["status"].as_str().unwrap().into())
    }

    fn records(&self) -> Result<Value> {
        let store = self.fixture.store("local")?;
        let database = store.connection()?;
        let mut query = database.prepare(
            "SELECT target,status,reason,checked_at,source FROM archive_batch_items
             WHERE batch_id=? ORDER BY position")?;
        let records = query.query_map([&self.fixture.batch], |row| {
            Ok(json!({"target":row.get::<_,String>(0)?,"status":row.get::<_,String>(1)?,
                "message":row.get::<_,Option<String>>(2)?,"checkedAt":row.get::<_,Option<f64>>(3)?,
                "source":row.get::<_,Option<String>>(4)?}))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        let (created, finished): (f64, Option<f64>) = database.query_row(
            "SELECT created_at,finished_at FROM archive_batches WHERE id=?", [&self.fixture.batch],
            |row| Ok((row.get(0)?,row.get(1)?)))?;
        Ok(json!({"batchId":self.fixture.batch,"createdAt":created,"finishedAt":finished,"items":records}))
    }

    fn owners(&self) -> Result<Value> {
        let mut hosts = serde_json::Map::new();
        for host in ["local", "mac", "other"] {
            let store = self.fixture.store(host)?;
            let database = store.connection()?;
            let rows: Vec<_> = self.identifiers.iter().map(|identifier| -> Result<Value> {
                Ok(database.query_row("SELECT archived,note FROM sessions WHERE id=?", [identifier],
                    |row| Ok(json!({"id":identifier,"archived":row.get::<_,bool>(0)?,
                        "note":row.get::<_,String>(1)?}))).optional()?.unwrap_or(Value::Null))
            }).collect::<Result<_>>()?;
            hosts.insert(host.into(), json!(rows));
        }
        Ok(Value::Object(hosts))
    }

    fn assert_readback(&self, records: &Value) -> Result<()> {
        let history: Value = self.fixture.client.get(format!(
            "http://127.0.0.1:{}/api/archive-batches", self.fixture.server.config.port))
            .send()?.error_for_status()?.json()?;
        let batch = history["batches"].as_array().unwrap().iter()
            .find(|batch| batch["id"] == self.fixture.batch).unwrap();
        let expected: Vec<_> = records["items"].as_array().unwrap().iter().map(|record| {
            let mut item: Value = serde_json::from_str(record["target"].as_str().unwrap()).unwrap();
            for field in ["status", "message", "checkedAt"] {
                item[field] = record[field].clone();
            }
            item
        }).collect();
        assert_eq!(batch["items"], json!(expected));
        assert_eq!(batch["createdAt"], records["createdAt"]);
        assert_eq!(batch["finishedAt"], records["finishedAt"]);
        Ok(())
    }

    fn assert_writes(&self, expected: &[usize]) {
        let surface = self.fixture.remotes.surface("mac");
        let surface = surface.lock().unwrap();
        for (identifier, expected) in self.identifiers.iter().zip(expected) {
            assert_eq!(surface.archive_writes.iter().filter(|written| *written == identifier).count(),
                *expected, "{identifier}");
        }
        assert!(self.fixture.server.surface.lock().unwrap().archive_writes.is_empty());
        assert!(self.fixture.remotes.surface("other").lock().unwrap().archive_writes.is_empty());
    }

    fn assert_statuses(&self, expected: &[&str]) -> Result<()> {
        let records = self.records()?;
        let actual: Vec<_> = records["items"].as_array().unwrap().iter()
            .map(|item| item["status"].as_str().unwrap()).collect();
        assert_eq!(actual, expected);
        Ok(())
    }

    fn prepare_results(&self) -> Result<()> {
        for position in 0..2 {
            assert_eq!(self.submit(position)?, "archived");
        }
        for position in 2..4 {
            self.fixture.remotes.surface("mac").lock().unwrap().lose_archive_receipt = true;
            assert_eq!(self.submit(position)?, "pending");
        }
        {
            let surface = self.fixture.remotes.surface("mac");
            let mut native = surface.lock().unwrap();
            native.probe_threads.insert(self.identifiers[4].clone(), json!({
                "id":self.identifiers[4],"status":{"type":"idle"},"turns":[{
                    "id":"new-turn","status":"completed","items":[{"id":"new-user","type":"userMessage"}]
                }]
            }));
            native.probe_threads.insert(self.identifiers[5].clone(), json!({
                "id":self.identifiers[5],"status":{"type":"active"},"turns":[]
            }));
        }
        assert_eq!(self.submit(4)?, "newInput");
        assert_eq!(self.submit(5)?, "rejected");
        Ok(())
    }

    fn observe_submission(&self) -> Result<()> {
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let gate = Gate(Arc::clone(&release));
        let (started, received) = mpsc::channel();
        self.fixture.remotes.surface("mac").lock().unwrap().read_blocked = Some(BlockingMove {
            identifier: self.identifiers[6].clone(), started, release,
        });
        std::thread::scope(|scope| -> Result<()> {
            let sending = scope.spawn(|| self.submit(6));
            received.recv_timeout(Duration::from_secs(5))?;
            self.assert_statuses(&["archived","archived","pending","pending","newInput","rejected",
                "pending","awaiting","awaiting","awaiting"])?;
            self.assert_writes(&[1,1,1,1,0,0,0,0,0,0]);
            eprintln!("E3_TRACE submitting=1 confirmed=2 pending=2 refused=2 awaiting=3 records={}",
                self.records()?);
            drop(gate);
            assert_eq!(sending.join().unwrap()?, "archived");
            Ok(())
        })?;
        self.fixture.remotes.surface("mac").lock().unwrap().read_blocked = None;
        Ok(())
    }

    fn assert_replay(&self, records: &Value) -> Result<()> {
        let owners = self.owners()?;
        for position in 0..10 {
            assert_eq!(self.submit(position)?, records["items"][position]["status"]);
        }
        self.assert_writes(&[1,1,1,1,0,0,1,0,0,0]);
        assert_eq!(self.records()?, *records);
        assert_eq!(self.owners()?, owners);
        Ok(())
    }

    fn assert_submission_owners(&self, before: &Value) -> Result<()> {
        let owners = self.owners()?;
        assert_eq!(owners["local"], before["local"]);
        assert_eq!(owners["other"], before["other"]);
        for position in 0..10 {
            assert_eq!(owners["mac"][position]["archived"], json!([0,1,6].contains(&position)));
            assert_eq!(owners["mac"][position]["note"], json!(format!("保留备注 {position}")));
        }
        Ok(())
    }

    fn assert_checked(&self, records: &Value, owners: &Value) -> Result<()> {
        self.assert_statuses(&["archived","archived","verified","verified","newInput","rejected",
            "archived","awaiting","awaiting","awaiting"])?;
        let checked = self.records()?;
        self.assert_readback(&checked)?;
        for position in 0..10 {
            if [2,3].contains(&position) {
                let mut expected = records["items"][position].clone();
                expected["status"] = json!("verified");
                expected["checkedAt"] = checked["items"][position]["checkedAt"].clone();
                assert!(expected["checkedAt"].is_f64());
                assert!(records["items"][position]["checkedAt"].is_null());
                assert_eq!(checked["items"][position], expected);
            } else {
                assert_eq!(checked["items"][position], records["items"][position]);
            }
        }
        let checked_owners = self.owners()?;
        assert_eq!(checked_owners["local"], owners["local"]);
        assert_eq!(checked_owners["other"], owners["other"]);
        for position in 0..10 {
            assert_eq!(checked_owners["mac"][position]["archived"], json!([0,1,2,3,6].contains(&position)));
            assert_eq!(checked_owners["mac"][position]["note"], json!(format!("保留备注 {position}")));
        }
        Ok(())
    }
}

#[test]
fn ten_card_batch_reopens_from_storage_and_checks_without_repeating_native_writes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    if isolated(directory.path(),
        "ten_cards::ten_card_batch_reopens_from_storage_and_checks_without_repeating_native_writes")? {
        return Ok(());
    }
    let mut cards = TenCards::new(directory.path())?;
    let before = cards.owners()?;
    cards.prepare_results()?;
    cards.observe_submission()?;
    cards.assert_submission_owners(&before)?;
    cards.request("/api/archive-batches/finish", json!({"id":cards.fixture.batch}))?;
    let records = cards.records()?;
    let owners = cards.owners()?;
    cards.assert_readback(&records)?;
    cards.fixture.reopen(directory.path())?;
    assert_eq!(cards.records()?, records);
    assert_eq!(cards.owners()?, owners);
    cards.assert_readback(&records)?;
    cards.assert_replay(&records)?;
    for _ in 0..2 {
        cards.request("/api/archive-batches/check", json!({"id":cards.fixture.batch}))?;
    }
    cards.assert_checked(&records, &owners)?;
    let checked = cards.records()?;
    let checked_owners = cards.owners()?;
    cards.fixture.reopen(directory.path())?;
    assert_eq!(cards.records()?, checked);
    assert_eq!(cards.owners()?, checked_owners);
    cards.assert_readback(&checked)?;
    cards.assert_replay(&checked)?;
    eprintln!("E3_TRACE reopened=2 confirmed=5 refused=2 awaiting=3 native_writes=[1,1,1,1,0,0,1,0,0,0]");
    Ok(())
}
