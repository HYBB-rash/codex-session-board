use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ColumnSort {
    pub field: SortField,
    pub direction: SortDirection,
    pub version: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    UpdatedAt,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SortUpdate {
    state: String,
    #[serde(flatten)]
    preference: ColumnSort,
}

fn read(database: &Connection, state: &str) -> Result<ColumnSort> {
    let value: Option<String> = database
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("column_sort:{state}")],
            |row| row.get(0),
        )
        .optional()?;
    value.map_or_else(
        || {
            Ok(ColumnSort {
                field: SortField::UpdatedAt,
                direction: if state == "to_archive" {
                    SortDirection::Asc
                } else {
                    SortDirection::Desc
                },
                version: 0,
            })
        },
        |value| serde_json::from_str(&value).context("栏目排序设置损坏"),
    )
}

impl Store {
    pub fn column_sorts(&self) -> Result<Value> {
        let database = self.connection()?;
        let preferences: BTreeMap<_, _> = crate::model::states_for(self.legacy_mode)
            .map(|(state, _)| Ok((state, read(&database, state)?)))
            .collect::<Result<_>>()?;
        Ok(serde_json::to_value(preferences)?)
    }

    pub fn save_column_sort(&self, body: &Value) -> Result<Value> {
        let update: SortUpdate =
            serde_json::from_value(body.clone()).context("栏目排序请求格式无效")?;
        ensure!(
            crate::model::states_for(self.legacy_mode).any(|(state, _)| state == update.state),
            "未知状态栏目"
        );
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let current = read(&transaction, &update.state)?;
        ensure!(
            current.version == update.preference.version,
            "栏目排序已更新，请刷新后重试"
        );
        let mut preference = update.preference;
        if current != preference {
            preference.version = preference.version.checked_add(1).context("排序版本溢出")?;
            transaction.execute(
                "INSERT INTO settings(key,value) VALUES (?,?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![
                    format!("column_sort:{}", update.state),
                    serde_json::to_string(&preference)?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(json!({"ok":true,"state":update.state,"preference":preference}))
    }
}
