use super::*;

pub(super) fn after_tombstone_failure(parent: &Path, lost_receipt: bool) -> Result<()> {
    let directory = tempfile::tempdir_in(parent)?;
    let mut fixture = BatchFixture::new(directory.path())?;
    let live = fixture.live.as_ref().unwrap();
    let snapshot = live.snapshot(&fixture.client)?;
    let original = row(&snapshot, &fixture.target).clone();
    fixture.store("mac")?.connection()?.execute_batch(
        "CREATE TRIGGER fail_archive BEFORE UPDATE OF archived ON sessions WHEN NEW.archived=1
         BEGIN SELECT RAISE(ABORT,'target tombstone write failed'); END;"
    )?;
    if lost_receipt {
        fixture.submit_losing_receipt()?;
    } else {
        assert_eq!(fixture.post("/api/archive")?["item"]["status"],"archived");
    }
    for _ in 0..20 {
        let id = uuid::Uuid::new_v4().to_string();
        live.post_path(&fixture.client, "/api/archive-batches", &json!({"id":id,
            "items":[{"id":SECOND,"title":"后续批次","version":0,"inputVersion":0}]}))?.error_for_status()?;
        live.post_path(&fixture.client, "/api/archive-batches/finish", &json!({"id":id}))?.error_for_status()?;
    }
    if lost_receipt {
        let pending = live.snapshot(&fixture.client)?;
        assert_eq!(row(&pending,&fixture.target)["archivePending"], true);
        assert_eq!(fixture.check()?, "verified");
    }
    let root = fixture.store("local")?;
    let database = root.connection()?;
    let older = uuid::Uuid::new_v4().to_string();
    database.execute("INSERT INTO archive_batches(id,created_at,finished_at) VALUES (?,0,1)", [&older])?;
    database.execute("INSERT INTO archive_batch_items(batch_id,position,id,target,status,reason,source)
        SELECT ?,position,id,target,status,reason,json_set(source,'$.account','zzzz-unused-account')
        FROM archive_batch_items WHERE batch_id=?", [&older, &fixture.batch])?;
    drop(database);
    fixture.reopen(directory.path())?;
    assert_history_and_offline_snapshot(&fixture)?;
    let fresh = restore_on_original_source(&mut fixture, directory.path(), &original)?;
    assert_only_restored_revision_can_archive(&fixture, &original, &fresh)
}

fn assert_history_and_offline_snapshot(fixture: &BatchFixture) -> Result<()> {
    let live = fixture.live.as_ref().unwrap();
    let history = App::new(fixture.server.config.clone())?.request("/api/archive-batches",None)?;
    let history = history["batches"].as_array().unwrap();
    assert_eq!(history.len(),20);
    assert!(!history.iter().any(|batch| batch["id"] == fixture.batch));
    let snapshot = live.snapshot(&fixture.client)?;
    assert!(snapshot["sessions"].as_array().unwrap().iter()
        .all(|card| card["id"] != fixture.target || card["archived"] == true),
        "二十个后续批次与重开不应重新提供已确认归档的活动修订");
    assert_ne!(row(&snapshot,&identity("other",FIRST))["archived"],true);
    assert!(!fixture.store("mac")?.archived_ids()?.contains(&FIRST.to_owned()));
    {
        let _offline = fixture.remotes.pause()?;
        let reader = Client::builder().no_proxy().timeout(Duration::from_secs(1)).build()?;
        let snapshot = live.snapshot(&reader)?;
        assert!(snapshot["sessions"].as_array().unwrap().iter().all(|card|
            card["id"] != fixture.target || card["archived"] == true));
        assert_ne!(row(&snapshot,&identity("other",FIRST))["archived"],true);
    }
    Ok(())
}

fn restore_on_original_source(fixture: &mut BatchFixture, directory: &Path, original: &Value) -> Result<Value> {
    fixture.remotes.surface("mac").lock().unwrap().archive_account = Some("different".into());
    fixture.remotes.restore_archived("mac", FIRST);
    fixture.server.config.runtime_poll_seconds = 1;
    fixture.reopen(directory)?;
    let live = fixture.live.as_ref().unwrap();
    let host_tick = |snapshot: &Value| snapshot["health"]["remoteConnections"].as_array().unwrap()
        .iter().find(|host| host["hostId"] == "mac").unwrap()["runtimeLastAt"].as_f64();
    let previous = host_tick(&live.snapshot(&fixture.client)?);
    let deadline = Instant::now() + Duration::from_secs(6);
    while host_tick(&live.snapshot(&fixture.client)?) <= previous {
        anyhow::ensure!(Instant::now() < deadline,"等待另一账号的主机观察");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!fixture.store("mac")?.archived_ids()?.contains(&FIRST.to_owned()));
    assert_eq!(fixture.store("mac")?.state_revision(FIRST)?.1, original["version"]);
    fixture.remotes.surface("mac").lock().unwrap().archive_account = Some("original".into());
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let snapshot = live.snapshot(&fixture.client)?;
        if let Some(card) = snapshot["sessions"].as_array().unwrap().iter().find(|card|
            card["id"] == fixture.target && card["archived"] != true && card["readOnly"] != true &&
            card["version"].as_i64() > original["version"].as_i64()) {
            return Ok(card.clone());
        }
        anyhow::ensure!(Instant::now()<deadline,"原生恢复至原分区后应自动重新提供新修订");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn assert_only_restored_revision_can_archive(fixture: &BatchFixture, original: &Value, fresh: &Value) -> Result<()> {
    let live = fixture.live.as_ref().unwrap();
    let target = json!({"id":original["id"],"version":original["version"],
        "inputVersion":original["inputVersion"],"sectionVersion":original["sectionVersion"]});
    let direct = live.post_path(&fixture.client,"/api/archive",&target)?;
    assert!(!direct.status().is_success());
    let failure = direct.text()?;
    assert!(failure.contains("已确认归档"),"{failure}");
    let replay = uuid::Uuid::new_v4().to_string();
    live.post_path(&fixture.client, "/api/archive-batches",
        &json!({"id":replay,"items":[original]}))?.error_for_status()?;
    let reply: Value = live.post_path(&fixture.client,"/api/archive",
        &json!({"id":fixture.target,"batchId":replay}))?.error_for_status()?.json()?;
    assert_eq!(reply["item"]["status"],"rejected");
    assert_eq!(fixture.writes("mac"),1);
    assert_eq!(fixture.writes("other"),0);
    assert_eq!(fresh["note"],"目标备注");
    fixture.store("mac")?.connection()?.execute_batch("DROP TRIGGER fail_archive")?;
    let fresh = json!({"id":fresh["id"],"version":fresh["version"],"inputVersion":fresh["inputVersion"],
        "sectionVersion":fresh["sectionVersion"]});
    let result: Value = live.post_path(&fixture.client,"/api/archive",&fresh)?.error_for_status()?.json()?;
    assert_eq!(result["archived"],true);
    assert_eq!(fixture.writes("mac"),2);
    Ok(())
}
