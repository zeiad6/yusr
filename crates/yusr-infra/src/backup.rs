//! Exact full backups and verified restores.
//!
//! A backup is every table of the schema, exported with `COPY` from one REPEATABLE READ
//! snapshot, plus every sequence position, the applied migrations and a SHA-256 per table.
//! A restore checks all of that before touching anything, loads the tables in one
//! transaction with the application's triggers suspended (the rows were already validated
//! when first written, and the audit chain must keep its original hashes), then exports the
//! tables again and commits only if every hash matches the backup byte for byte.
//!
//! File layout: `YUSRBAK1` then a gzip stream of the table data in manifest order, the
//! manifest JSON, and the manifest length as a little-endian u64.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};

use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use yusr_domain::DomainError;

use crate::{InfraError, Result, Store, audit};

pub const MAGIC: &[u8; 8] = b"YUSRBAK1";
const FORMAT: u32 = 1;
/// Largest restorable backup once decompressed; a bigger stream is refused as a likely
/// decompression bomb. The whole backup is held in memory while it is checked.
pub const MAX_RESTORE_BYTES: u64 = 2 << 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableDump {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: u64,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SequenceState {
    pub name: String,
    pub last_value: i64,
    pub is_called: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Migration {
    pub version: i64,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub base_currency: String,
    pub migrations: Vec<Migration>,
    pub tables: Vec<TableDump>,
    pub sequences: Vec<SequenceState>,
    pub audit_records: i64,
    pub audit_head: Option<String>,
}

impl Manifest {
    pub fn total_rows(&self) -> u64 {
        self.tables.iter().map(|t| t.rows).sum()
    }
}

/// A finished backup file and what it contains.
pub struct Backup {
    pub bytes: Vec<u8>,
    pub manifest: Manifest,
    /// SHA-256 of the whole file, for the operator's records.
    pub file_sha256: String,
}

/// The outcome of a restore.
#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub tables: usize,
    pub rows: u64,
    pub audit_records: i64,
    pub backup_created_at: DateTime<Utc>,
    pub verified: bool,
}

fn invalid(msg: impl Into<String>) -> InfraError {
    DomainError::Invalid(msg.into()).into()
}

fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Output of `COPY ... TO` in text format depends on these settings; both ends pin them.
const PIN_SETTINGS: &[&str] = &[
    "SET LOCAL TimeZone = 'UTC'",
    "SET LOCAL DateStyle = 'ISO, YMD'",
    "SET LOCAL IntervalStyle = 'postgres'",
    "SET LOCAL extra_float_digits = 3",
    "SET LOCAL bytea_output = 'hex'",
];

async fn pin(conn: &mut PgConnection) -> Result<()> {
    for s in PIN_SETTINGS {
        sqlx::query(s).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Application tables, parents before children so foreign keys hold during a load.
async fn tables(conn: &mut PgConnection) -> Result<Vec<String>> {
    let names: Vec<(String,)> = sqlx::query_as(
        "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = current_schema() AND c.relkind IN ('r', 'p') AND c.relname <> '_sqlx_migrations'",
    )
    .fetch_all(&mut *conn)
    .await?;
    let edges: Vec<(String, String)> = sqlx::query_as(
        "SELECT ch.relname::text, pa.relname::text FROM pg_constraint k
           JOIN pg_class ch ON ch.oid = k.conrelid JOIN pg_class pa ON pa.oid = k.confrelid
           JOIN pg_namespace n ON n.oid = ch.relnamespace
          WHERE k.contype = 'f' AND n.nspname = current_schema() AND ch.oid <> pa.oid",
    )
    .fetch_all(&mut *conn)
    .await?;
    topo_order(names.into_iter().map(|(n,)| n).collect(), &edges)
}

/// Kahn's algorithm with alphabetical tie-breaks, so the order is stable between runs.
fn topo_order(names: Vec<String>, child_parent: &[(String, String)]) -> Result<Vec<String>> {
    let mut parents: BTreeMap<&str, BTreeSet<&str>> = names.iter().map(|n| (n.as_str(), BTreeSet::new())).collect();
    // Self-references (a parent account) load fine within one COPY: foreign keys are checked per statement.
    for (child, parent) in child_parent.iter().filter(|(c, p)| c != p) {
        if let (Some(ps), true) = (parents.get_mut(child.as_str()), names.contains(parent)) {
            ps.insert(parent.as_str());
        }
    }
    let mut order = Vec::with_capacity(names.len());
    while !parents.is_empty() {
        let ready: Vec<&str> = parents.iter().filter(|(_, ps)| ps.is_empty()).map(|(n, _)| *n).collect();
        if ready.is_empty() {
            return Err(InfraError::Config("foreign keys form a cycle between tables".into()));
        }
        for n in ready {
            parents.remove(n);
            parents.values_mut().for_each(|ps| {
                ps.remove(n);
            });
            order.push(n.to_owned());
        }
    }
    Ok(order)
}

/// Stored columns in table order; generated columns are recomputed by the database.
async fn columns(conn: &mut PgConnection, table: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT a.attname::text FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = current_schema() AND c.relname = $1 AND a.attnum > 0 AND NOT a.attisdropped AND a.attgenerated = ''
          ORDER BY a.attnum",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(c,)| c).collect())
}

async fn order_by(conn: &mut PgConnection, table: &str, columns: &[String]) -> Result<String> {
    let pk: Vec<(String,)> = sqlx::query_as(
        "SELECT a.attname::text FROM pg_index i
           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY (i.indkey)
          WHERE i.indrelid = (quote_ident(current_schema()) || '.' || quote_ident($1))::regclass AND i.indisprimary
          ORDER BY array_position(i.indkey::int2[], a.attnum)",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;
    Ok(if pk.is_empty() {
        (1..=columns.len()).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
    } else {
        pk.iter().map(|(c,)| ident(c)).collect::<Vec<_>>().join(", ")
    })
}

fn column_list(columns: &[String]) -> String {
    columns.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ")
}

/// Streams one table in key order into `sink`, returning (rows, bytes, sha256).
async fn export_table(conn: &mut PgConnection, table: &str, columns: &[String], sink: &mut impl Write) -> Result<(u64, u64, String)> {
    let order = order_by(conn, table, columns).await?;
    let sql = format!("COPY (SELECT {} FROM {} ORDER BY {order}) TO STDOUT", column_list(columns), ident(table));
    let mut stream = conn.copy_out_raw(&sql).await?;
    let (mut hash, mut rows, mut bytes) = (Sha256::new(), 0u64, 0u64);
    while let Some(chunk) = stream.try_next().await? {
        hash.update(&chunk);
        // Text-format COPY escapes embedded newlines, so each row ends with exactly one.
        rows += chunk.iter().filter(|b| **b == b'\n').count() as u64;
        bytes += chunk.len() as u64;
        sink.write_all(&chunk).map_err(|e| InfraError::Config(format!("backup write: {e}")))?;
    }
    Ok((rows, bytes, hex::encode(hash.finalize())))
}

async fn sequences(conn: &mut PgConnection) -> Result<Vec<SequenceState>> {
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT sequencename::text FROM pg_sequences WHERE schemaname = current_schema() ORDER BY sequencename")
            .fetch_all(&mut *conn)
            .await?;
    let mut out = Vec::with_capacity(names.len());
    for (name,) in names {
        let (last_value, is_called): (i64, bool) =
            sqlx::query_as(&format!("SELECT last_value, is_called FROM {}", ident(&name))).fetch_one(&mut *conn).await?;
        out.push(SequenceState { name, last_value, is_called });
    }
    Ok(out)
}

async fn migrations(conn: &mut PgConnection) -> Result<Vec<Migration>> {
    let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations WHERE success ORDER BY version")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(|(version, c)| Migration { version, checksum: hex::encode(c) }).collect())
}

/// A parsed, integrity-checked backup file.
pub struct Parsed {
    pub manifest: Manifest,
    data: Vec<u8>,
}

impl Parsed {
    fn table_data(&self) -> impl Iterator<Item = (&TableDump, &[u8])> {
        let mut at = 0usize;
        self.manifest.tables.iter().map(move |t| {
            let slice = &self.data[at..at + t.bytes as usize];
            at += t.bytes as usize;
            (t, slice)
        })
    }
}

/// Checks the container, the manifest and every table hash. Touches no database.
pub fn parse(file: &[u8]) -> Result<Parsed> {
    let body = file.strip_prefix(MAGIC.as_slice()).ok_or_else(|| invalid("not a Yusr backup file"))?;
    let mut raw = Vec::new();
    GzDecoder::new(body)
        .take(MAX_RESTORE_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|_| invalid("the backup file is damaged (compression check failed)"))?;
    if raw.len() as u64 > MAX_RESTORE_BYTES {
        return Err(invalid("the backup is larger than the restore limit"));
    }
    let len_at = raw.len().checked_sub(8).ok_or_else(|| invalid("the backup file is truncated"))?;
    let manifest_len = u64::from_le_bytes(raw[len_at..].try_into().expect("8 bytes")) as usize;
    let manifest_at = len_at.checked_sub(manifest_len).ok_or_else(|| invalid("the backup file is truncated"))?;
    let manifest: Manifest = serde_json::from_slice(&raw[manifest_at..len_at]).map_err(|_| invalid("the backup manifest is unreadable"))?;
    if manifest.format != FORMAT {
        return Err(invalid(format!("unsupported backup format {}", manifest.format)));
    }
    let data_len: u64 = manifest.tables.iter().map(|t| t.bytes).sum();
    if data_len != manifest_at as u64 {
        return Err(invalid("the backup data does not match its manifest"));
    }
    raw.truncate(manifest_at);
    let parsed = Parsed { manifest, data: raw };
    for (t, data) in parsed.table_data() {
        if hex::encode(Sha256::digest(data)) != t.sha256 {
            return Err(invalid(format!("table {} in the backup is corrupted (checksum mismatch)", t.name)));
        }
    }
    Ok(parsed)
}

/// Serialises restores: two at once would interleave their truncates and loads.
static RESTORE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

impl Store {
    /// Exports the whole database from one consistent snapshot.
    pub async fn backup(&self, actor: &str) -> Result<Backup> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY").execute(&mut *tx).await?;
        pin(&mut tx).await?;
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        let mut dumps = Vec::new();
        for table in tables(&mut tx).await? {
            let cols = columns(&mut tx, &table).await?;
            let (rows, bytes, sha256) = export_table(&mut tx, &table, &cols, &mut gz).await?;
            dumps.push(TableDump { name: table, columns: cols, rows, bytes, sha256 });
        }
        let report = audit::verify_on(&mut tx).await?;
        let manifest = Manifest {
            format: FORMAT,
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at: Utc::now(),
            created_by: actor.into(),
            base_currency: self.base_currency.to_string(),
            migrations: migrations(&mut tx).await?,
            sequences: sequences(&mut tx).await?,
            tables: dumps,
            audit_records: report.records_checked,
            audit_head: report.head_hash,
        };
        tx.commit().await?;
        let json = serde_json::to_vec(&manifest).expect("manifest serializes");
        let io = |e: std::io::Error| InfraError::Config(format!("backup write: {e}"));
        gz.write_all(&json).map_err(io)?;
        gz.write_all(&(json.len() as u64).to_le_bytes()).map_err(io)?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend(gz.finish().map_err(io)?);
        let file_sha256 = hex::encode(Sha256::digest(&bytes));
        crate::serializable!(self, |conn| {
            let summary = serde_json::json!({ "tables": manifest.tables.len(), "rows": manifest.total_rows(), "sha256": file_sha256 });
            audit::append(conn, actor, "backup", "database", "full", summary).await?;
            Ok(())
        })?;
        Ok(Backup { bytes, manifest, file_sha256 })
    }

    /// Whether `parsed` can be restored into this database, and why not.
    pub async fn check_compatible(&self, parsed: &Parsed) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        if migrations(&mut conn).await? != parsed.manifest.migrations {
            return Err(invalid("the backup was made by a different version of Yusr (schema migrations differ)"));
        }
        if parsed.manifest.base_currency != self.base_currency.to_string() {
            return Err(invalid(format!("the backup uses base currency {}", parsed.manifest.base_currency)));
        }
        let ours: BTreeSet<String> = tables(&mut conn).await?.into_iter().collect();
        let theirs: BTreeSet<String> = parsed.manifest.tables.iter().map(|t| t.name.clone()).collect();
        if ours != theirs {
            return Err(invalid("the backup's tables do not match this database"));
        }
        for t in &parsed.manifest.tables {
            if columns(&mut conn, &t.name).await? != t.columns {
                return Err(invalid(format!("the columns of {} differ from the backup", t.name)));
            }
        }
        Ok(())
    }

    /// Replaces every table with the backup's contents, verified before commit. Every
    /// session is signed out afterwards: restored token versions could otherwise revive
    /// tokens that had been revoked since the backup was taken.
    pub async fn restore(&self, parsed: &Parsed, actor: &str) -> Result<RestoreReport> {
        let _one = RESTORE.lock().await;
        self.check_compatible(parsed).await?;
        let m = &parsed.manifest;
        let mut tx = self.pool.begin().await?;
        pin(&mut tx).await?;
        let order: Vec<String> = tables(&mut tx).await?;
        let list = order.iter().map(|t| ident(t)).collect::<Vec<_>>().join(", ");
        for t in &order {
            sqlx::query(&format!("ALTER TABLE {} DISABLE TRIGGER USER", ident(t))).execute(&mut *tx).await?;
        }
        sqlx::query(&format!("TRUNCATE {list}")).execute(&mut *tx).await?;
        let by_name: BTreeMap<&str, (&TableDump, &[u8])> = parsed.table_data().map(|(t, d)| (t.name.as_str(), (t, d))).collect();
        for t in &order {
            let (dump, data) = by_name[t.as_str()];
            let mut copy = tx.copy_in_raw(&format!("COPY {} ({}) FROM STDIN", ident(t), column_list(&dump.columns))).await?;
            if !data.is_empty() {
                copy.send(data).await?;
            }
            let rows = copy.finish().await?;
            if rows != dump.rows {
                return Err(invalid(format!("table {t}: loaded {rows} rows, the backup has {}", dump.rows)));
            }
        }
        for s in &m.sequences {
            sqlx::query("SELECT setval($1::regclass, $2, $3)").bind(ident(&s.name)).bind(s.last_value).bind(s.is_called).execute(&mut *tx).await?;
        }
        for t in &order {
            sqlx::query(&format!("ALTER TABLE {} ENABLE TRIGGER USER", ident(t))).execute(&mut *tx).await?;
        }
        // Proof of an exact restore: the reloaded tables export to the same bytes.
        for t in &order {
            let (dump, _) = by_name[t.as_str()];
            let (rows, _, sha256) = export_table(&mut tx, t, &dump.columns, &mut std::io::sink()).await?;
            if sha256 != dump.sha256 || rows != dump.rows {
                return Err(invalid(format!("table {t} did not restore exactly; nothing was changed")));
            }
        }
        let chain = audit::verify_on(&mut tx).await?;
        if !chain.ok || chain.head_hash != m.audit_head {
            return Err(invalid("the restored audit chain does not verify; nothing was changed"));
        }
        sqlx::query("UPDATE users SET token_version = token_version + 1").execute(&mut *tx).await?;
        let summary = serde_json::json!({ "tables": order.len(), "rows": m.total_rows(), "backup_created_at": m.created_at, "backup_created_by": m.created_by });
        audit::append(&mut *tx, actor, "restore", "database", "full", summary).await?;
        let (secret,): (Vec<u8>,) = sqlx::query_as("SELECT voucher_secret FROM settings").fetch_one(&mut *tx).await?;
        tx.commit().await?;
        *self.voucher_secret.write().expect("voucher secret lock") = secret;
        Ok(RestoreReport { tables: order.len(), rows: m.total_rows(), audit_records: chain.records_checked + 1, backup_created_at: m.created_at, verified: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_come_first() {
        let names = ["lines", "entries", "accounts", "funds"].map(String::from).to_vec();
        let edges = [("lines", "entries"), ("lines", "accounts"), ("lines", "funds"), ("entries", "entries")]
            .map(|(a, b)| (a.to_owned(), b.to_owned()));
        let order = topo_order(names, &edges).unwrap();
        let at = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(at("entries") < at("lines") && at("accounts") < at("lines") && at("funds") < at("lines"));
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn cycles_are_reported() {
        let names = ["a", "b"].map(String::from).to_vec();
        let edges = [("a", "b"), ("b", "a")].map(|(a, b)| (a.to_owned(), b.to_owned()));
        assert!(topo_order(names, &edges).is_err());
    }

    #[test]
    fn damaged_files_are_rejected() {
        assert!(parse(b"not a backup").is_err());
        let mut gz = GzEncoder::new(MAGIC.to_vec(), Compression::default());
        gz.write_all(b"garbage").unwrap();
        assert!(parse(&gz.finish().unwrap()).is_err());
        let mut truncated = MAGIC.to_vec();
        truncated.extend_from_slice(&[0x1f, 0x8b, 0x08]);
        assert!(parse(&truncated).is_err());
    }

    #[test]
    fn identifiers_are_quoted() {
        assert_eq!(ident("users"), "\"users\"");
        assert_eq!(ident("a\"b"), "\"a\"\"b\"");
    }
}
