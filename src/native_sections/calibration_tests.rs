#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
#[path = "calibration_fixture.rs"]
mod fixture;
use fixture::{Fixture, Mock};
use std::sync::Arc;

#[test]
fn archived_unreadable_probe_finishes_after_restart_without_repeating_writes() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().fail_archived_list = true;
    assert!(fixture.run().is_err());
    let pending = fixture.record()?;
    let identifier = pending.probe.as_ref().unwrap().thread.as_ref().unwrap().clone();
    assert_eq!(pending.logical.len(),5);
    {
        let mut surface = fixture.surface.lock().unwrap();
        assert_eq!(surface.threads[&identifier]["archived"],true);
        surface.archived_reads_missing = true;
        surface.fail_archived_list = true;
    }
    assert!(fixture.run().is_err());
    assert_eq!(serde_json::to_value(fixture.record()?.probe)?,serde_json::to_value(&pending.probe)?);
    fixture.store = Store::open(&fixture.config.data_dir,false)?;
    let recovered = fixture.run()?;
    assert!(recovered.probe.is_none());
    assert_eq!(recovered.logical,pending.logical);
    assert!(fixture.store.is_native_probe(&fixture.config.host_id,&identifier)?);
    assert_eq!(fixture.calls("thread/start"),1);
    assert_eq!(fixture.calls("thread/archive"),1);
    assert_eq!(fixture.calls("move_thread_to_sidebar_section"),5);
    Ok(())
}

#[test]
fn readable_archived_probe_still_preserves_user_adoption() -> Result<()> {
    for (field,value) in [("name",json!("用户接手")),
                         ("turns",json!([{"id":"user-turn","status":"completed"}]))] {
        let fixture = Fixture::new()?;
        fixture.surface.lock().unwrap().fail_archived_list = true;
        assert!(fixture.run().is_err());
        let identifier = fixture.record()?.probe.unwrap().thread.unwrap();
        fixture.surface.lock().unwrap().threads.get_mut(&identifier).unwrap()[field] = value.clone();
        assert!(fixture.run().is_err());
        assert!(fixture.record()?.probe.is_none());
        assert!(!fixture.store.is_native_probe(&fixture.config.host_id,&identifier)?);
        assert_eq!(fixture.surface.lock().unwrap().threads[&identifier][field],value);
        assert_eq!(fixture.calls("thread/start"),1);
        assert_eq!(fixture.calls("thread/archive"),1);
    }
    Ok(())
}

#[test]
fn archived_missing_probe_before_mapping_completion_keeps_its_ownership_marker() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().lose_name = true;
    assert!(fixture.run().is_err());
    let identifier = fixture.record()?.probe.unwrap().thread.unwrap();
    {
        let mut surface = fixture.surface.lock().unwrap();
        surface.threads.get_mut(&identifier).unwrap()["archived"] = json!(true);
        surface.archived_reads_missing = true;
    }
    assert!(fixture.run()?.probe.is_none());
    assert!(fixture.store.is_native_probe(&fixture.config.host_id,&identifier)?);
    assert_eq!(fixture.calls("thread/start"),2);
    assert_eq!(fixture.calls("thread/archive"),1);
    Ok(())
}

#[test]
fn empty_history_fallback_requires_the_exact_native_unmaterialized_proof_for_this_thread() {
    let id = uuid::Uuid::new_v4().to_string();
    let message = format!("thread {id} is not materialized yet; includeTurns is unavailable before first user message");
    let error = |method: &str,code: i64,message: &str| anyhow::Error::new(
        crate::rpc::RequestError {method:method.into(),details:json!({"code":code,"message":message})});
    assert!(unmaterialized(&error("thread/read",-32600,&message),&id));
    assert!(!unmaterialized(&error("thread/read",-32603,&message),&id));
    assert!(!unmaterialized(&error("thread/list",-32600,&message),&id));
    assert!(!unmaterialized(&error("thread/read",-32600,"history unavailable"),&id));
    assert!(!unmaterialized(&error("thread/read",-32600,&message),&uuid::Uuid::new_v4().to_string()));
    assert!(!unmaterialized(&anyhow::anyhow!(message),&id));
    let paginated = format!("thread {id} is not materialized yet; thread/turns/list is unavailable before first user message");
    assert!(paginated_unmaterialized(&error("thread/turns/list",-32600,&paginated),&id));
    assert!(!paginated_unmaterialized(&error("thread/turns/list",-32603,&paginated),&id));
    assert!(!paginated_unmaterialized(&error("thread/turns/list",-32600,&paginated),&uuid::Uuid::new_v4().to_string()));
    assert!(!paginated_unmaterialized(&error("thread/turns/list",-32600,"list_turns is not supported yet"),&id));
}

#[test]
fn a_known_unmaterialized_legacy_probe_can_finish_after_restart_without_recreation() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().lose_name = true;
    assert!(fixture.run().is_err());
    let store = &fixture.store;
    let mut record = store.native_mapping("remote:a")?.unwrap();
    let probe = record.probe.as_mut().unwrap();
    let identifier = probe.thread.clone().unwrap();
    probe.history_mode = Value::Null;
    fixture.surface.lock().unwrap().threads.get_mut(&identifier).unwrap()["historyMode"] = json!("legacy");
    store.save_native_mapping("remote:a",&record)?;
    assert!(fixture.run()?.probe.is_none());
    assert_eq!(fixture.calls("thread/start"),1);
    assert_eq!(fixture.calls("thread/archive"),1);
    Ok(())
}

#[test]
fn a_known_disappeared_probe_is_independently_retired_before_a_new_empty_probe_is_created() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().lose_name = true;
    assert!(fixture.run().is_err());
    let record = fixture.store.native_mapping("remote:a")?.unwrap();
    let identifier = record.probe.unwrap().thread.unwrap();
    fixture.surface.lock().unwrap().threads.remove(&identifier);
    assert!(fixture.run()?.probe.is_none());
    assert!(!fixture.store.is_native_probe("remote:a",&identifier)?);
    assert_eq!(fixture.calls("thread/start"),2);
    assert_eq!(fixture.calls("thread/archive"),1);
    Ok(())
}

#[test]
fn empty_probe_learns_every_remote_mapping_without_disk_flush_and_reuses_after_restart() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = std::fs::read(&fixture.config.sidebar_state_file)?;
    fixture.surface.lock().unwrap().lose_move = true;
    fixture.surface.lock().unwrap().unmaterialized = true;
    fixture.surface.lock().unwrap().changing_read_times = true;
    let first = fixture.run()?;
    assert_eq!(first.logical.len(),5);
    assert!(first.probe.is_none());
    assert_eq!(fixture.calls("move_thread_to_sidebar_section"),5);
    assert!(fixture.calls("thread/turns/list") > 0);
    assert_eq!(fixture.calls("thread/start"),1);
    assert_eq!(fixture.calls("thread/archive"),1);
    let reopened = Store::open(&fixture.config.data_dir,false)?;
    assert_eq!(reopened.native_mapping(&fixture.config.host_id)?.unwrap().logical,first.logical);
    assert_eq!(fixture.run()?.logical,first.logical);
    assert_eq!(fixture.calls("thread/start"),1);
    assert_eq!(fixture.calls("move_thread_to_sidebar_section"),5);
    assert_eq!(std::fs::read(&fixture.config.sidebar_state_file)?,before);
    let surface = fixture.surface.lock().unwrap();
    assert!(surface.threads.values().all(|thread|
        thread["archived"] == true && thread["turns"] == json!([])));
    assert!(!surface.calls.iter().any(|call| call.starts_with("turn/")));
    Ok(())
}

#[test]
fn unknown_creation_never_duplicates_until_explicit_matching_request_reset() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().lose_start = true;
    assert!(fixture.run().is_err());
    assert!(fixture.run().is_err());
    assert_eq!(fixture.calls("thread/start"),1);
    let record = fixture.record()?;
    let probe = record.probe.as_ref().unwrap();
    assert!(probe.thread.is_none());
    assert!(fixture.store.abandon_unknown_native_probe(&fixture.config.host_id,
        &uuid::Uuid::new_v4().to_string(),&record.scope["source"]).is_err());
    assert!(fixture.store.abandon_unknown_native_probe(&fixture.config.host_id,
        &probe.request,&json!({"peer":{"home":"/another"}})).is_err());
    assert_eq!(fixture.calls("thread/start"),1);
    fixture.store.abandon_unknown_native_probe(&fixture.config.host_id,
        &probe.request,&record.scope["source"])?;
    assert_eq!(fixture.run()?.logical.len(),5);
    assert_eq!(fixture.calls("thread/start"),2);
    Ok(())
}

#[test]
fn renamed_probe_is_preserved_after_lost_name_receipt_and_a_new_owned_probe_can_finish() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().lose_name = true;
    assert!(fixture.run().is_err());
    let id = fixture.record()?.probe.unwrap().thread.unwrap();
    fixture.surface.lock().unwrap().threads.get_mut(&id).unwrap()["name"] = json!("用户接手");
    assert!(fixture.run().is_err());
    assert!(!fixture.store.is_native_probe(&fixture.config.host_id,&id)?);
    assert!(fixture.record()?.probe.is_none());
    assert_eq!(fixture.run()?.logical.len(),5);
    let surface = fixture.surface.lock().unwrap();
    assert_eq!(surface.threads[&id]["name"],"用户接手");
    assert_eq!(surface.threads[&id]["archived"],false);
    assert_eq!(surface.calls.iter().filter(|call| *call == "thread/start").count(),2);
    Ok(())
}

#[test]
fn reconnect_cleans_known_probe_but_never_archives_a_probe_adopted_during_disconnect() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().reject_archive = true;
    assert!(fixture.run().is_err());
    assert_eq!(fixture.record()?.logical.len(),5);
    let id = fixture.record()?.probe.unwrap().thread.unwrap();
    fixture.surface.lock().unwrap().threads.get_mut(&id).unwrap()["turns"] =
        json!([{"id":"user-turn","status":"completed"}]);
    fixture.surface.lock().unwrap().reject_archive = false;
    assert!(fixture.run().is_err());
    assert!(fixture.run()?.probe.is_none());
    assert_eq!(fixture.calls("thread/start"),1);
    assert_eq!(fixture.calls("thread/archive"),1);
    assert!(!fixture.store.is_native_probe(&fixture.config.host_id,&id)?);
    assert_eq!(fixture.surface.lock().unwrap().threads[&id]["archived"],false);
    Ok(())
}

#[test]
fn cross_time_same_name_position_cannot_publish_a_mapping_and_backend_rebuild_recalibrates() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().cross_read = true;
    assert!(fixture.run().is_err());
    assert!(fixture.record()?.logical.is_empty());
    let first = fixture.run()?;
    assert_eq!(fixture.calls("thread/start"),1);
    assert!(!first.logical.contains_key("foreign-same-name"));
    {
        let mut surface = fixture.surface.lock().unwrap();
        surface.sections.clear();
        surface.backend.clear();
    }
    let rebuilt = fixture.run()?;
    assert_eq!(rebuilt.logical.len(),5);
    assert!(rebuilt.logical.keys().all(|id| !first.logical.contains_key(id)));
    assert_eq!(fixture.calls("thread/start"),2);
    assert_eq!(fixture.calls("thread/archive"),2);
    Ok(())
}

#[test]
fn deleted_logical_identity_cleans_owned_probe_and_requires_explicit_new_configuration() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.surface.lock().unwrap().reject_archive = true;
    assert!(fixture.run().is_err());
    let mut record = fixture.record()?;
    fixture.config.status_sections.insert("review".into(),uuid::Uuid::new_v4().to_string());
    fixture.surface.lock().unwrap().reject_archive = false;
    let mut backend = Mock(Arc::clone(&fixture.surface),false);
    let mut sidebar = Mock(Arc::clone(&fixture.surface),true);
    Calibration {config:&fixture.config,store:&fixture.store,record:&mut record,
        backend:&mut backend,sidebar:&mut sidebar}.cleanup()?;
    assert!(record.probe.is_none());
    assert!(fixture.run().is_err());
    assert_eq!(fixture.calls("thread/start"),1);
    assert_eq!(fixture.calls("thread/archive"),2);
    Ok(())
}
