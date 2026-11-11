use rusqlite::{Connection, Result, types::Value};
use std::collections::BTreeMap;

pub type Rows = Vec<Vec<Value>>;

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub schema: Rows,
    pub tables: BTreeMap<String, Rows>,
}

fn rows(database: &Connection, sql: &str) -> Result<Rows> {
    let mut query = database.prepare(sql)?;
    let count = query.column_count();
    query.query_map([], |row| (0..count).map(|index| row.get(index)).collect())?
        .collect()
}

pub fn schema(database: &Connection) -> Result<Rows> {
    rows(database, "SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
}

pub fn snapshot(database: &Connection) -> Result<Snapshot> {
    let mut query = database.prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?;
    let names = query.query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    let tables = names.into_iter().map(|name| {
        let escaped = name.replace('"', "\"\"");
        Ok((name, rows(database, &format!("SELECT * FROM \"{escaped}\" ORDER BY rowid"))?))
    }).collect::<Result<_>>()?;
    Ok(Snapshot { schema: schema(database)?, tables })
}
