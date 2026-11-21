use super::*;
use serde_json::Value;

fn card(app: &App, title: &str, state: &str) -> Result<Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let record = json!({"id":id,"name":title,"updatedAt":1,"ordinary":true});
    app.store.discover(std::slice::from_ref(&record))?;
    if title == "Follow" {
        app.store.observe(&id, &json!({"id":"turn","userItems":[]}), None)?;
    }
    app.store.set_state(&id, Some(state), "", 0, Some(0))?;
    Ok(record)
}

fn populate(root: &App, remote: &App) -> Result<String> {
    card(root, "Local", "completed")?;
    let follow = card(root, "Follow", "to_archive")?;
    root.store.observe(follow["id"].as_str().unwrap(),
        &json!({"id":"turn","userItems":["input"]}), None)?;
    let ready = card(remote, "Online", "to_archive")?;
    let mut archived = card(remote, "Archived", "to_archive")?;
    archived["archived"] = json!(true);
    card(remote, "Cache", "to_archive")?;
    let pending = card(root, "Pending", "to_archive")?;
    let batch = uuid::Uuid::new_v4().to_string();
    root.request("/api/archive-batches", Some(&json!({"id":batch,"items":[{
        "id":pending["id"],"title":"Pending","version":1,"inputVersion":0
    }]})))?;
    assert!(root.store.claim_archive_item(&batch, pending["id"].as_str().unwrap(), &json!({}))?);
    let id = ready["id"].as_str().unwrap().to_owned();
    let metadata = json!({"id":uuid::Uuid::new_v4().to_string(),"name":"Catalog","ordinary":true});
    let mut catalog = remote.catalog.lock().unwrap();
    catalog.snapshot = Some(crate::catalog::Snapshot { source: json!({}), captured_at:1.0,
        active:vec![ready,metadata], archived:vec![archived], projects:vec![], coverage:json!({}), issues:vec![] });
    catalog.live = true;
    Ok(id)
}

#[test]
fn card_buttons_and_batch_submit_only_eligible_revisions() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.config.four_columns = true;
    fixture.config.desktop_socket = fixture.config.data_dir.join("offline.sock");
    let (root, remote) = managed(&fixture)?;
    let root = Arc::new(root);
    let thread = populate(&root, &remote)?;
    fixture.update("results", json!({"account/read":{"account":null,"requiresOpenaiAuth":false},
        "thread/list":{"data":[]},"thread/read":{"thread":{"id":thread,"status":{"type":"idle"}}},
        "thread/turns/list":{"data":[]},"thread/archive":null}))?;
    drop(remote.connection(Duration::from_secs(3), Role::RemoteObserver)?);
    let (server, port) = crate::http::bind(0)?;
    let app = Arc::clone(&root);
    let service = std::thread::spawn(move || crate::http::run(server, port, app));
    let browser = std::process::Command::new("node").arg("scripts/check-card-actions.mjs")
        .arg(format!("http://127.0.0.1:{port}")).output();
    root.shutdown()?;
    service.join().unwrap()?;
    let browser = browser?;
    assert!(browser.status.success(), "{}", String::from_utf8_lossy(&browser.stderr));
    let batches = root.store.archive_batches()?;
    assert_eq!(batches.len(), 3);
    let revisions = |batch: &crate::store::archive_batches::Batch| {
        let mut items: Vec<_> = batch.items.iter().map(|item|
            (item.target.title.clone(), item.target.version, item.target.input_version, item.status.clone())
        ).collect();
        items.sort();
        items
    };
    assert_eq!(revisions(&batches[1]), [
        ("Local".into(), 1, 0, "rejected".into()), ("Online".into(), 1, 0, "pending".into())]);
    assert_eq!(revisions(&batches[0]), [("Follow".into(), 2, 1, "rejected".into())]);
    assert_eq!(fixture.events().lines().filter(|line| *line == "thread/archive").count(), 1);
    assert_receipt_versions(&root, &thread, &batches[1])
}

fn assert_receipt_versions(root: &App, thread: &str, batch: &crate::store::archive_batches::Batch) -> Result<()> {
    let accepted = &batch.items.iter().find(|item| item.status == "pending").unwrap().target;
    root.store.record_archive_result(&batch.id,&accepted.id,"pending","verified",None)?;
    let original = serde_json::to_value(accepted)?;
    let mut distinct = vec![];
    for (key,value) in [("version",json!(accepted.version+1)),("inputVersion",json!(accepted.input_version+1)),
        ("sectionVersion",json!("another-section")),("archived",json!(true)),
        ("id",json!(crate::store::remote::identity("another-host", thread)))] {
        let mut card = original.clone();
        card[key] = value;
        distinct.push(card);
    }
    for native in [false, true] {
        let mut store = crate::store::Store::open(&root.config.data_dir, false)?;
        store.native_sections = native;
        let mut visible = distinct.clone();
        visible.push(original.clone());
        store.apply_archive_receipts(&mut visible)?;
        let expected: Vec<_> = distinct.iter()
            .filter(|card| native || card["sectionVersion"] == original["sectionVersion"])
            .cloned().collect();
        assert_eq!(visible, expected, "原生分区启用时才把分区版本作为不同修订");
    }
    Ok(())
}
