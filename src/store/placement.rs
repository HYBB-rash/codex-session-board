use super::suggestion;
use serde_json::{Value, json};

pub(super) fn work_state<'a>(
    record: &'a Value,
    native: bool,
    history: impl FnOnce() -> &'a str,
) -> (&'a str, &'static str) {
    if native {
        (record["native_state"].as_str().unwrap_or("unknown"), "codex")
    } else if let Some(state) = record["selected_state"].as_str() {
        (state, "manual")
    } else if let Some(state) = record["automatic_state"].as_str() {
        (state, "runtime")
    } else {
        (history(), "history")
    }
}

pub(super) fn history(record: &Value, enabled: bool) -> (&'static str, &'static str) {
    if enabled { suggestion(record) } else { ("unknown", "尚无可确认的工作状态。") }
}

pub(super) fn confirmed_empty(record: &Value, turn: &Value) -> bool {
    turn["confirmedEmpty"] == true && record["observed_revision"] == record["updated_at"]
        && !record["selected_state"].is_string() && record["automatic_state"].is_null()
        && record["runtime_key"].is_null()
}

enum Placement<'a> {
    Column(&'a str),
    Unplaced(&'static str),
    Hidden,
}

fn decide(row: &Value, empty: bool, legacy: bool, four_columns: bool) -> Placement<'_> {
    let state = row["state"].as_str().unwrap_or("unknown");
    if !four_columns || state != "unknown" {
        return Placement::Column(if legacy { state } else { crate::model::board_state(state) });
    }
    let position = row["sectionVersion"].as_str();
    let default_section = matches!(position, Some("unsectioned" | "chats" | "threads"));
    if empty && row["presence"] != "cachedUnknown" && row["archived"] != true
        && row["note"] == "" && (position.is_none() || default_section) {
        return Placement::Hidden;
    }
    Placement::Unplaced(match position {
        _ if default_section => "原生侧栏默认区",
        Some("pinned") => "原生侧栏置顶",
        Some(_) => "原生侧栏分区未映射",
        None => "尚未确认原生位置或工作状态",
    })
}

pub(crate) fn project(rows: &mut Vec<Value>, legacy: bool, four_columns: bool) {
    rows.retain_mut(|row| {
        let empty = row.as_object_mut().and_then(|row| row.remove("confirmedEmpty")) == Some(json!(true));
        if four_columns && row["suggestion"] == "unknown" {
            row["suggestion"] = Value::Null;
        }
        match decide(row, empty, legacy, four_columns) {
            Placement::Column(state) if row["state"] != state => row["state"] = json!(state),
            Placement::Column(_) => (),
            Placement::Unplaced(reason) => {
                row["state"] = Value::Null;
                row["placementReason"] = json!(reason);
            }
            Placement::Hidden => return false,
        }
        true
    });
}
