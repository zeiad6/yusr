//! Persistence for the hash-chained audit log.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use yusr_domain::audit::{AuditRecord, verify_chain};

use crate::Result;

/// Appends a sealed record in the caller's transaction and returns its hash.
///
/// Two transactions racing for the same sequence number collide on the primary key;
/// the loser is retried by [`crate::serializable!`] and re-seals on the new head.
pub async fn append(
    conn: &mut PgConnection,
    actor: &str,
    action: &str,
    entity: &str,
    entity_id: &str,
    payload: serde_json::Value,
) -> Result<[u8; 32]> {
    let head: Option<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1").fetch_optional(&mut *conn).await?;
    let head = head.map(|(seq, hash)| (to_hash(&hash), seq));
    let record = AuditRecord::seal(head.as_ref().map(|(h, s)| (h, *s)), Utc::now(), actor, action, entity, entity_id, payload);
    sqlx::query(
        "INSERT INTO audit_log (seq, occurred_at, actor, action, entity, entity_id, payload, prev_hash, hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(record.seq)
    .bind(record.occurred_at)
    .bind(&record.actor)
    .bind(&record.action)
    .bind(&record.entity)
    .bind(&record.entity_id)
    .bind(&record.payload)
    .bind(&record.prev_hash[..])
    .bind(&record.hash[..])
    .execute(&mut *conn)
    .await?;
    Ok(record.hash)
}

fn to_hash(v: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&v[..32]); // the CHECK constraint guarantees 32 bytes
    out
}

type Row = (i64, DateTime<Utc>, String, String, String, String, serde_json::Value, Vec<u8>, Vec<u8>);

fn record((seq, occurred_at, actor, action, entity, entity_id, payload, prev, hash): Row) -> AuditRecord {
    AuditRecord { seq, occurred_at, actor, action, entity, entity_id, payload, prev_hash: to_hash(&prev), hash: to_hash(&hash) }
}

const SELECT: &str = "SELECT seq, occurred_at, actor, action, entity, entity_id, payload, prev_hash, hash FROM audit_log";

#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyReport {
    pub records_checked: i64,
    pub head_hash: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

/// Re-hashes the whole chain in pages of 1000 records.
pub async fn verify(pool: &PgPool) -> Result<VerifyReport> {
    verify_on(&mut *pool.acquire().await?).await
}

/// [`verify`] on a given connection, so a restore can check the chain before it commits.
pub async fn verify_on(conn: &mut sqlx::PgConnection) -> Result<VerifyReport> {
    const PAGE: i64 = 1000;
    let mut anchor: Option<([u8; 32], i64)> = None;
    let mut checked = 0i64;
    loop {
        let after = anchor.map_or(0, |(_, s)| s);
        let rows: Vec<Row> = sqlx::query_as(&format!("{SELECT} WHERE seq > $1 ORDER BY seq LIMIT $2"))
            .bind(after)
            .bind(PAGE)
            .fetch_all(&mut *conn)
            .await?;
        if rows.is_empty() {
            break;
        }
        let records: Vec<AuditRecord> = rows.into_iter().map(record).collect();
        if let Err(e) = verify_chain(anchor, &records) {
            return Ok(VerifyReport { records_checked: checked, head_hash: None, ok: false, error: Some(e.to_string()) });
        }
        checked += records.len() as i64;
        let last = records.last().expect("non-empty page");
        anchor = Some((last.hash, last.seq));
    }
    Ok(VerifyReport { records_checked: checked, head_hash: anchor.map(|(h, _)| hex::encode(h)), ok: true, error: None })
}

pub async fn list(pool: &PgPool, entity: Option<&str>, limit: i64) -> Result<Vec<AuditRecord>> {
    let rows: Vec<Row> = sqlx::query_as(&format!("{SELECT} WHERE ($1::text IS NULL OR entity = $1) ORDER BY seq DESC LIMIT $2"))
        .bind(entity)
        .bind(limit.clamp(1, 500))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(record).collect())
}
