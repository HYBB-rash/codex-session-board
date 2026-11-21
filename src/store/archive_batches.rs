use super::{Store, now};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) enum ArchiveReceipt {
    Pending,
    Confirmed,
}

fn revision(id: &str, card: &Value) -> Value {
    json!({"id":id,"version":card["version"],"inputVersion":card["inputVersion"],
        "sectionVersion":card["sectionVersion"]})
}

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

    pub(crate) fn archive_receipt(&self, id: &str, card: &Value) -> Result<Option<ArchiveReceipt>> {
        Ok(self.archive_receipts(&[revision(id, card)])?.pop().flatten())
    }

    fn archive_receipts(&self, revisions: &[Value]) -> Result<Vec<Option<ArchiveReceipt>>> {
        if revisions.is_empty() { return Ok(Vec::new()) }
        let database = self.connection()?;
        let mut query = database.prepare_cached(
            "WITH cards AS MATERIALIZED (
               SELECT key, CAST(value ->> 'id' AS TEXT) AS id, value ->> 'version' AS version,
                 value ->> 'inputVersion' AS input, value ->> 'sectionVersion' AS section FROM json_each(?)
             )
             SELECT cards.key, MAX(receipt.status != 'pending')
             FROM cards CROSS JOIN archive_batch_items AS receipt ON receipt.id=cards.id
             WHERE receipt.status IN ('pending','archived','verified')
               AND (receipt.status='pending' OR (cards.version=receipt.target ->> 'version'
                 AND cards.input=receipt.target ->> 'inputVersion'
                 AND (NOT ?2 OR cards.section IS receipt.target ->> 'sectionVersion')))
             GROUP BY cards.key"
        )?;
        let mut rows = query.query(params![serde_json::to_string(revisions)?, self.native_sections])?;
        let mut receipts = vec![None; revisions.len()];
        while let Some(row) = rows.next()? {
            let position = usize::try_from(row.get::<_, i64>(0)?)?;
            receipts[position] = Some(if row.get(1)? {
                ArchiveReceipt::Confirmed
            } else {
                ArchiveReceipt::Pending
            });
        }
        Ok(receipts)
    }

    pub(crate) fn apply_archive_receipts(&self, sessions: &mut Vec<Value>) -> Result<()> {
        let revisions: Vec<_> = sessions.iter().map(|card|
            revision(card["id"].as_str().unwrap_or_default(),card)).collect();
        let mut receipts = self.archive_receipts(&revisions)?.into_iter();
        sessions.retain_mut(|card| match receipts.next().flatten() {
            Some(ArchiveReceipt::Confirmed) => card["archived"] == true,
            Some(ArchiveReceipt::Pending) => { card["archivePending"] = Value::Bool(true); true }
            None => true,
        });
        Ok(())
    }

    pub(crate) fn failed_archive_marks(&self) -> Result<Vec<(Target, String)>> {
        let database = self.connection()?;
        let mut query = database.prepare_cached(
            "SELECT DISTINCT target,source FROM archive_batch_items
             WHERE status IN ('archived','verified') AND reason IS NOT NULL AND source IS NOT NULL"
        )?;
        let mut rows = query.query([])?;
        let mut targets = Vec::new();
        while let Some(row) = rows.next()? {
            targets.push((serde_json::from_str(&row.get::<_, String>(0)?)?, row.get(1)?));
        }
        Ok(targets)
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
