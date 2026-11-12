use super::*;
use serde_json::Value;

#[test]
fn remote_empty_notes_clear_but_omitted_notes_survive_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = Store::open(directory.path(), false)?;
    discover(&store, "mac", 1)?;
    let identifier = identity("mac", THREAD);
    let mut version = 0;
    for (note, expected) in [
        (Some(json!("旧备注")), "旧备注"),
        (None, "旧备注"),
        (Some(json!("")), ""),
        (Some(json!("\t\r\n　")), ""),
    ] {
        let mut body = json!({"id":identifier,"state":"blocked","version":version,"inputVersion":0});
        if let Some(note) = note { body["note"] = note; }
        store.save_remote(&body)?;
        version += 1;
        let saved = store.remote_session(&identifier)?;
        assert_eq!(saved["state"], "blocked");
        assert_eq!(saved["note"], expected);
        assert_eq!(saved["version"], version);
        assert!(store.save_remote(&body).is_err());
    }
    for note in [Value::Null, json!(false), json!("字".repeat(2001))] {
        assert!(store.save_remote(&json!({"id":identifier,"state":"blocked","note":note,
            "version":version,"inputVersion":0})).is_err());
    }
    drop(store);
    let store = Store::open(directory.path(), false)?;
    let saved = store.remote_session(&identifier)?;
    assert_eq!(saved["note"], "");
    assert_eq!(saved["state"], "blocked");
    assert_eq!(saved["version"], version);
    Ok(())
}
