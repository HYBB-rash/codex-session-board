use super::{Store, now};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Target {
    pub id: String,
    pub title: String,
    pub version: i64,
    pub input_version: i64,
    pub section_version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    #[serde(flatten)]
    pub target: Target,
    pub status: String,
    pub message: Option<String>,
    pub checked_at: Option<f64>,
    #[serde(skip)]
    pub source: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Batch {
    pub id: String,
    pub created_at: f64,
    pub finished_at: Option<f64>,
    pub items: Vec<Item>,
}

impl Store {
    pub(crate) fn create_archive_batch(&self, id: &str, targets: &[Target]) -> Result<Batch> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        if let Some(batch) = batch(&transaction, id)? {
            ensure!(
                batch.items.iter().map(|item| &item.target).eq(targets),
                "归档批次名单已固定"
            );
            return Ok(batch);
        }
        transaction.execute(
            "INSERT INTO archive_batches(id,created_at) VALUES (?,?)",
            params![id, now()],
        )?;
        for (position, target) in targets.iter().enumerate() {
            transaction.execute(
                "INSERT INTO archive_batch_items(batch_id,position,id,target) VALUES (?,?,?,?)",
                params![id, i64::try_from(position)?, target.id, serde_json::to_string(target)?],
            )?;
        }
        let result = batch(&transaction, id)?.context("归档批次不存在")?;
        transaction.commit()?;
        Ok(result)
    }

    pub(crate) fn archive_batch(&self, id: &str) -> Result<Batch> {
        batch(&*self.connection()?, id)?.context("归档批次不存在")
    }

    pub(crate) fn archive_item(&self, batch: &str, id: &str) -> Result<Item> {
        let database = self.connection()?;
        let mut query = database.prepare(
            "SELECT target,status,reason,checked_at,source FROM archive_batch_items
             WHERE batch_id=? AND id=?",
        )?;
        item(query.query([batch, id])?.next()?.context("会话不在此归档批次")?)
    }

    pub(crate) fn archive_batches(&self) -> Result<Vec<Batch>> {
        let database = self.connection()?;
        let mut query = database.prepare(
            "SELECT id FROM archive_batches WHERE id IN (
               SELECT id FROM (SELECT id FROM archive_batches ORDER BY created_at DESC,id DESC LIMIT 20)
               UNION SELECT batch_id FROM archive_batch_items WHERE status='pending'
             ) ORDER BY created_at DESC,id DESC",
        )?;
        let identifiers = query.query_map([], |row| row.get::<_, String>(0))?;
        identifiers.map(|id| batch(&database, &id?)?.context("归档批次不存在")).collect()
    }

    pub(crate) fn archive_is_pending(&self, id: &str) -> Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM archive_batch_items WHERE id=? AND status='pending')",
            [id], |row| row.get(0),
        )?)
    }

    pub(crate) fn claim_archive_item(&self, batch: &str, id: &str, source: &Value) -> Result<bool> {
        Ok(self.connection()?.execute(
            "UPDATE OR IGNORE archive_batch_items SET status='pending', source=?
             WHERE batch_id=? AND id=? AND status='awaiting'
               AND EXISTS (SELECT 1 FROM archive_batches WHERE id=? AND finished_at IS NULL)",
            params![source.to_string(), batch, id, batch],
        )? == 1)
    }

    pub(crate) fn record_archive_result(
        &self, batch: &str, id: &str, from: &str, status: &str, reason: Option<&str>,
    ) -> Result<()> {
        self.connection()?.execute(
            "UPDATE archive_batch_items SET status=?, reason=?, checked_at=?
             WHERE batch_id=? AND id=? AND status=?
             AND (? != 'awaiting' OR EXISTS
               (SELECT 1 FROM archive_batches WHERE id=? AND finished_at IS NULL))",
            params![status, reason, now(), batch, id, from, from, batch],
        )?;
        Ok(())
    }

    pub(crate) fn finish_archive_batch(&self, id: &str) -> Result<Batch> {
        self.connection()?.execute(
            "UPDATE archive_batches SET finished_at=? WHERE id=? AND finished_at IS NULL",
            params![now(), id],
        )?;
        self.archive_batch(id)
    }
}

fn batch(database: &Connection, id: &str) -> Result<Option<Batch>> {
    let Some((created_at, finished_at)) = database.query_row(
        "SELECT created_at,finished_at FROM archive_batches WHERE id=?",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()? else {
        return Ok(None);
    };
    let mut query = database.prepare(
        "SELECT target,status,reason,checked_at,source FROM archive_batch_items
         WHERE batch_id=? ORDER BY position",
    )?;
    let mut rows = query.query([id])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(item(row)?);
    }
    Ok(Some(Batch { id: id.to_owned(), created_at, finished_at, items }))
}

fn item(row: &rusqlite::Row<'_>) -> Result<Item> {
    let target: String = row.get(0)?;
    Ok(Item {
        target: serde_json::from_str(&target)?,
        status: row.get(1)?,
        message: row.get(2)?,
        checked_at: row.get(3)?,
        source: row.get(4)?,
    })
}
