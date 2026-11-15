use super::*;
use codex_session_board::remote::hosts::Host;

fn legacy(server: &mut Server, remotes: &RemoteFixture) -> Result<()> {
    server.config.poll_seconds = 60;
    server.config.runtime_poll_seconds = 60;
    setup_remote(server);
    let app = server.app()?;
    for host in ["mac","other"] {
        app.store.discover_remote(host,host,&[json!({"id":FIRST,"hostId":host,"updatedAt":1}),
            json!({"id":SECOND,"hostId":host,"updatedAt":1})])?;
        app.store.save_remote(&json!({"id":identity(host,FIRST),"state":"blocked",
            "note":"保留的阻塞原因","version":0,"inputVersion":0}))?;
    }
    remotes.seed_legacy_cache(&server.config,&app.store)?;
    for host in ["mac","other"] {
        let target: Host = serde_json::from_value(json!({"hostId":host,"displayName":host,"alias":host}))?;
        let store = Store::open(&target.directory(&server.config),false)?;
        for id in [FIRST,SECOND] {
            store.observe(id,&json!({"id":"old","status":"completed","completedAt":1,
                "userItems":["input"]}),None)?;
            store.connection()?.execute("UPDATE sessions SET runtime_key='old-running-phase',
                needs_review=0 WHERE id=?", [id])?;
        }
        let surface = remotes.surface(host);
        let mut surface = surface.lock().unwrap();
        surface.slots.insert(FIRST.into(),"unsectioned".into());
        surface.slots.insert(SECOND.into(),"to_archive".into());
        surface.latest_turn = Some(json!({"id":"old","status":"completed","completedAt":1,
            "items":[{"id":"input","type":"userMessage","content":[{"type":"text","text":"old"}]}]}));
    }
    Ok(())
}

fn remove_disk_mapping(config: &Config) -> Result<()> {
    let mut state: Value = serde_json::from_slice(&std::fs::read(&config.sidebar_state_file)?)?;
    let sections = state["electron-persisted-atom-state"]["sidebar-custom-sections-v3"]
        .as_object_mut().unwrap().values_mut().next().unwrap()["sections"].as_array_mut().unwrap();
    for section in sections { section["hostSectionIds"].as_object_mut().unwrap().remove("mac"); }
    std::fs::write(&config.sidebar_state_file,serde_json::to_vec(&state)?)?;
    Ok(())
}

fn verify_initial(live: &Live, client: &Client, snapshot: &Value, remotes: &RemoteFixture) -> Result<()> {
    let blocked = row(snapshot,&identity("mac",FIRST));
    assert_eq!(blocked["state"],"blocked");
    assert_eq!(blocked["note"],"保留的阻塞原因");
    assert_eq!(blocked["readOnly"],false);
    let archive = row(snapshot,&identity("mac",SECOND));
    assert_eq!(archive["state"],"to_archive");
    assert_eq!(archive["followUp"],true);
    assert!(!live.post_path(client,"/api/archive",&update(archive,"to_archive",""))?.status().is_success());
    let surface = remotes.surface("mac");
    let surface = surface.lock().unwrap();
    assert_eq!(surface.calls.iter().filter(|method| *method == "thread/start").count(),1);
    assert_eq!(surface.calls.iter().filter(|method| *method == "move_thread_to_sidebar_section").count(),6);
    assert_eq!(surface.probe_threads.len(),1);
    let owned = remotes.archived("mac");
    assert!(surface.probe_threads.keys().all(|id| owned.contains(id)));
    assert!(snapshot["sessions"].as_array().unwrap().iter().all(|row|
        !surface.probe_threads.contains_key(row["threadId"].as_str().unwrap_or(""))));
    Ok(())
}

#[test]
fn remote_first_scan_backfills_legacy_rows_and_cold_maps_then_reconnects_without_old_phase_replay() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut server = Server::new(directory.path())?;
    let remotes = RemoteFixture::new(&mut server,directory.path())?;
    legacy(&mut server,&remotes)?;
    remove_disk_mapping(&server.config)?;
    remotes.surface("mac").lock().unwrap().lose_receipt = true;
    remotes.surface("mac").lock().unwrap().lose_archive_receipt = true;
    let client = Client::builder().no_proxy().timeout(Duration::from_secs(8)).build()?;
    let live = Live::new(&mut server.config,directory.path())?;
    live.ready(&client)?;
    let initial = wait_remote(&live,&client,true,None)?;
    verify_initial(&live,&client,&initial,&remotes)?;
    drop(live);
    let pause = remotes.pause()?;
    let offline = Live::new(&mut server.config,directory.path())?;
    offline.ready(&client)?;
    let cached = offline.snapshot(&client)?;
    assert_eq!(row(&cached,&identity("mac",FIRST))["state"],"blocked");
    assert_eq!(row(&cached,&identity("mac",FIRST))["note"],"保留的阻塞原因");
    assert_eq!(row(&cached,&identity("mac",FIRST))["readOnly"],true);
    assert_eq!(row(&cached,&identity("mac",SECOND))["followUp"],true);
    drop(offline);
    drop(pause);
    remotes.surface("mac").lock().unwrap().latest_turn = Some(json!({
        "id":"new","status":"completed","completedAt":codex_session_board::model::now(),
        "items":[{"type":"agentMessage","id":"answer","phase":"final_answer","text":"done"}]}));
    let restored = Live::new(&mut server.config,directory.path())?;
    restored.ready(&client)?;
    let after = wait_remote(&restored,&client,true,None)?;
    assert_eq!(row(&after,&identity("mac",FIRST))["state"],"review");
    assert_eq!(row(&after,&identity("mac",FIRST))["note"],"保留的阻塞原因");
    let surface = remotes.surface("mac");
    let surface = surface.lock().unwrap();
    assert_eq!(surface.calls.iter().filter(|method| *method == "thread/start").count(),1);
    assert_eq!(surface.calls.iter().filter(|method| *method == "move_thread_to_sidebar_section").count(),8);
    Ok(())
}
