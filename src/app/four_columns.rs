use serde_json::{Value, json};

pub(super) fn project(sessions: &mut Vec<Value>) {
    sessions.retain_mut(|row| {
        let empty = row.as_object_mut()
            .and_then(|row| row.remove("confirmedEmpty")) == Some(json!(true));
        if row["remote"] == true {
            row["canOpen"] = json!(can_open(row));
        }
        if row["suggestion"] == "unknown" {
            row["suggestion"] = Value::Null;
        }
        if row["state"] != "unknown" {
            return true;
        }
        if empty && row["presence"] != "cachedUnknown" && row["archived"] != true
            && row["note"] == ""
            && row["sectionVersion"].as_str().is_none_or(|position| {
                matches!(position, "unsectioned" | "chats" | "threads")
            })
        {
            return false;
        }
        row["state"] = Value::Null;
        row["placementReason"] = json!(match row["sectionVersion"].as_str() {
            Some("unsectioned" | "chats" | "threads") => "原生侧栏默认区",
            Some("pinned") => "原生侧栏置顶",
            Some(_) => "原生侧栏分区未映射",
            None => "尚未确认原生位置或工作状态",
        });
        true
    });
}

pub(super) fn can_open(row: &Value) -> bool {
    matches!(row["presence"].as_str(), Some("active" | "archived"))
        && row["catalogCurrent"] == true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::App, config::Config};
    use anyhow::Result;

    #[test]
    fn four_column_snapshot_preserves_legacy_cards_and_rejects_retired_writes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            four_columns: true,
            sync_titles: false,
            ..Config::default()
        })?;
        app.store.discover(&[
            json!({"id":"manual","updatedAt":1}),
            json!({"id":"unread","updatedAt":1}),
            json!({"id":"empty","updatedAt":1}),
        ])?;
        app.store.set_state("manual", Some("unknown"), "旧备注", 0, Some(0))?;
        app.store.observe("empty", &json!({"confirmedEmpty":true}), None)?;
        let report = app.snapshot()?;
        assert_eq!(report["fourColumns"], true);
        assert_eq!(report["states"], json!({
            "in_progress":"进行中","review":"待审阅","blocked":"已阻塞","to_archive":"待归档"
        }));
        assert!(report["columnSorts"].get("unknown").is_none());
        let rows = report["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let row = rows.iter().find(|row| row["id"] == "manual").unwrap();
        assert_eq!(row["state"], Value::Null);
        assert_eq!(row["note"], "旧备注");
        assert!(rows.iter().any(|row| row["id"] == "unread" && row["state"].is_null()));
        assert!(app.request("/api/state", Some(&json!({
            "id":"manual","state":"unknown","version":1,"inputVersion":0
        }))).is_err());
        app.request("/api/state", Some(&json!({
            "id":"manual","state":"review","version":1,"inputVersion":0
        })))?;
        assert_eq!(app.store.note("manual")?, "旧备注");
        Ok(())
    }

    #[test]
    fn four_column_remote_keeps_default_placement_without_replaying_old_backfill() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app = App::new(Config {
            data_dir: directory.path().into(),
            four_columns: true,
            native_sections: true,
            host_id: "remote:fixture".into(),
            ..Config::default()
        })?;
        app.store.discover(&[json!({"id":"old","updatedAt":1})])?;
        app.store.observe("old", &json!({"status":"completed","hasFinal":true}), None)?;
        assert!(!app.needs_native_backfill("old")?);
        assert_eq!(app.native_runtime_since("old", 42.0)?, 42.0);
        assert_eq!(app.store.sessions()?[0]["state"], "unknown");
        Ok(())
    }

    #[test]
    fn only_current_remote_rows_offer_navigation() {
        let mut rows = vec![
            json!({"state":"review","remote":true,"presence":"cachedUnknown","catalogCurrent":true}),
            json!({"state":"unknown","remote":true,"presence":"active","catalogCurrent":true}),
        ];
        project(&mut rows);
        assert_eq!(rows[0]["canOpen"], false);
        assert_eq!(rows[1]["canOpen"], true);
        assert_eq!(rows[1]["state"], Value::Null);
    }
}
