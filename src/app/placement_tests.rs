use super::*;
use crate::{native_sections::Observation, store::remote::identity};
use std::path::Path;
use uuid::Uuid;

const HOST: &str = "remote:fixture";

fn board(directory: &Path, columns: usize, native: bool) -> Result<App> {
    let config = Config {
        data_dir: directory.join("root"),
        four_columns: columns == 4,
        legacy_mode: columns == 6,
        native_sections: native,
        sync_titles: false,
        ..Config::default()
    };
    let root = App::new(config.clone())?;
    if !config.legacy_mode {
        let remote = App::new(Config {
            host_id: HOST.into(),
            data_dir: directory.join("remote"),
            ..config
        })?;
        remote.update_health(json!({"runtimeVerified":true}))?;
        root.remotes.lock().unwrap().insert(HOST.into(), Arc::new(remote));
    }
    Ok(root)
}

fn threads<const COUNT: usize>(labels: [&str; COUNT]) -> [Value; COUNT] {
    labels.map(|name| json!({"id":Uuid::new_v4().to_string(), "name":name, "updatedAt":1}))
}

fn id(thread: &Value) -> &str {
    thread["id"].as_str().unwrap()
}

fn rows(root: &App) -> Result<Vec<Value>> {
    Ok(root.request("/api/sessions", None)?["sessions"].as_array().unwrap().clone())
}

fn assert_card(rows: &[Value], thread: &Value, remote: bool, expected: Option<Value>) {
    let thread = id(thread);
    let id = if remote { identity(HOST, thread) } else { thread.to_string() };
    let matches: Vec<_> = rows.iter().filter(|row| row["id"] == id).collect();
    assert_eq!(matches.len(), usize::from(expected.is_some()), "{id}");
    if let Some(state) = expected {
        assert_eq!(matches[0].get("state"), Some(&state), "{id}");
        assert!(matches[0].get("confirmedEmpty").is_none());
    }
}

fn seed_work(app: &App, threads: &[Value; 8]) -> Result<()> {
    app.discover(threads, true)?;
    let [history, manual, noted, completed, empty, running, note_only, stale] = threads;
    for thread in [history, manual] {
        app.store.observe(id(thread), &json!({"id":"turn","status":"completed","hasFinal":true}), None)?;
    }
    for (thread, state) in [(manual, "blocked"), (noted, "unknown"), (completed, "completed")] {
        app.store.set_state(id(thread), Some(state), "旧备注", 0, Some(0))?;
    }
    app.store.observe_runtime(id(running), &json!({"type":"active"}),
        &json!({"id":"turn","status":"inProgress"}), None)?;
    for thread in [empty, note_only, stale] {
        app.store.observe(id(thread), &json!({"confirmedEmpty":true}), None)?;
    }
    app.store.set_state(id(note_only), None, "仅保留备注", 0, Some(0))?;
    let mut newer = stale.clone();
    newer["updatedAt"] = json!(2);
    app.store.discover(&[newer])?;
    Ok(())
}

#[test]
fn snapshot_places_local_and_remote_cards_from_work_evidence() -> Result<()> {
    let threads = threads(["历史", "人工", "旧备注", "旧完成", "空会话", "运行", "只有备注", "空证据已过期"]);
    for columns in [4, 5, 6] {
        let directory = tempfile::tempdir()?;
        let root = board(directory.path(), columns, false)?;
        seed_work(&root, &threads)?;
        for remote in root.remote_instances()? { seed_work(&remote, &threads)?; }
        let rows = rows(&root)?;
        let unknown = if columns == 4 { Value::Null } else { json!("unknown") };
        let expected = [
            Some(if columns == 4 { Value::Null } else { json!("review") }),
            Some(json!("blocked")), Some(unknown.clone()), Some(json!(if columns == 6 { "completed" } else { "to_archive" })),
            (columns != 4).then(|| json!("unknown")), Some(json!("in_progress")),
            Some(unknown.clone()), Some(unknown),
        ];
        let hosts: &[_] = if columns == 6 { &[false] } else { &[false, true] };
        for &remote in hosts {
            for (thread, state) in threads.iter().zip(&expected) {
                assert_card(&rows, thread, remote, state.clone());
            }
        }
        for row in rows.iter().filter(|row| row["manual"] == true) {
            assert_eq!(row["note"], "旧备注");
            assert_eq!(row["version"], 1);
        }
        for row in rows.iter().filter(|row| row["title"] == "只有备注") {
            assert_eq!(row["note"], "仅保留备注");
            assert_eq!(row["manual"], false);
        }
        assert_eq!(root.store.state_revision(threads[3]["id"].as_str().unwrap())?.0, "completed");
    }
    Ok(())
}

fn seed_native(app: &App, threads: &[Value; 5]) -> Result<()> {
    app.discover(threads, true)?;
    let [mapped, missing, default, pinned, custom] = threads;
    for thread in [mapped, missing] {
        app.store.observe(id(thread), &json!({"status":"completed","hasFinal":true}), None)?;
        app.store.set_state(id(thread), Some("blocked"), "旧备注", 0, Some(0))?;
    }
    for thread in [default, pinned, custom] {
        app.store.observe(id(thread), &json!({"confirmedEmpty":true}), None)?;
    }
    for (thread, state, position) in [(mapped, "review", "review-section"), (default, "unknown", "chats"),
        (pinned, "unknown", "pinned"), (custom, "unknown", "custom")] {
        app.store.cache_native(id(thread),
            &Observation { state: state.into(), version: position.into() })?;
    }
    Ok(())
}

fn snapshot_with_busy_hosts(root: &App, remote: &App, thread: &Value) -> Result<()> {
    let cards = rows(root)?;
    let targets: Vec<_> = cards.iter().filter(|card| card["title"] == thread["name"])
        .map(|card| serde_json::from_value(card.clone())).collect::<serde_json::Result<_>>()?;
    assert_eq!(targets.len(), 2);
    let batch = Uuid::new_v4().to_string();
    root.store.create_archive_batch(&batch, &targets)?;
    for target in &targets {
        root.store.claim_archive_item(&batch, &target.id, &json!({"home":"offline"}))?;
        root.store.record_archive_result(&batch, &target.id, "pending", "verified", Some("等待目标库"))?;
    }
    std::thread::scope(|scope| -> Result<()> {
        let local_lock = root.lifecycle.write().unwrap();
        let remote_lock = remote.lifecycle.write().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        scope.spawn(move || { let _ = send.send(rows(root)); });
        let result = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop((local_lock, remote_lock));
        let visible = result.context("目标主机被占用时读取快照超时")??;
        assert!(visible.iter().all(|card| card["title"] != thread["name"]));
        assert!(!visible.is_empty());
        Ok(())
    })
}

#[test]
fn snapshot_respects_native_positions_and_remote_presence() -> Result<()> {
    let threads = threads(["已映射", "位置未确认", "默认区", "置顶", "未映射分区"]);
    for columns in [4, 5] {
        let directory = tempfile::tempdir()?;
        let root = board(directory.path(), columns, true)?;
        seed_native(&root, &threads)?;
        let remote = root.remote_instances()?.remove(0);
        seed_native(&remote, &threads)?;
        let active = rows(&root)?;
        let unknown = if columns == 4 { Value::Null } else { json!("unknown") };
        for remote_row in [false, true] {
            assert_card(&active, &threads[0], remote_row, Some(json!("review")));
            assert_card(&active, &threads[1], remote_row, Some(unknown.clone()));
            assert_card(&active, &threads[2], remote_row, (columns == 5).then(|| unknown.clone()));
            for thread in &threads[3..] {
                assert_card(&active, thread, remote_row, Some(unknown.clone()));
            }
        }
        remote.discover(&[], true)?;
        let cached = rows(&root)?;
        assert_card(&cached, &threads[2], true, Some(unknown));
        if columns == 4 {
            for (label, reason) in [("位置未确认", "尚未确认原生位置或工作状态"),
                ("默认区", "原生侧栏默认区"), ("置顶", "原生侧栏置顶"), ("未映射分区", "原生侧栏分区未映射")] {
                assert!(cached.iter().any(|row| row["remote"] == true && row["title"] == label
                    && row["placementReason"] == reason));
            }
            snapshot_with_busy_hosts(&root, &remote, &threads[0])?;
        }
    }
    Ok(())
}

#[test]
fn snapshot_preserves_legacy_remote_cache_in_each_mode() -> Result<()> {
    let [thread] = threads(["旧缓存"]);
    for columns in [4, 5, 6] {
        let directory = tempfile::tempdir()?;
        let root = board(directory.path(), columns, false)?;
        root.store.discover_remote(HOST, "fixture", &[json!({"id":thread["id"],
            "hostId":HOST,"title":"旧缓存","updatedAt":1})])?;
        root.store.connection()?.execute(
            "UPDATE remote_sessions SET state='completed',note='历史备注',version=2", [])?;
        let rows = rows(&root)?;
        assert_card(&rows, &thread, true, Some(json!(if columns == 6 { "completed" } else { "to_archive" })));
        assert_eq!(rows[0]["note"], "历史备注");
        assert_eq!(rows[0]["version"], 2);
        assert_eq!(rows[0]["readOnly"], columns != 6);
        assert_eq!(rows[0]["canArchive"], false);
        assert_eq!(root.store.remote_session(rows[0]["id"].as_str().unwrap())?["state"], "completed");
    }
    Ok(())
}
