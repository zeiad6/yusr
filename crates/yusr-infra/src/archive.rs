//! Electronic archive: numbered documents with write-once attached files. A document filed
//! in error is voided with a reason, never deleted, so the register keeps no gaps.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use yusr_domain::DomainError;

use crate::{InfraError, Result, Store, audit, parse_label, serializable};

/// Largest single file accepted: scanned contracts and photos, not video.
pub const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCategory {
    Incoming,
    Outgoing,
    Contract,
    Financial,
    Report,
    Beneficiary,
    Donor,
    Hr,
    Legal,
    Other,
}

impl DocumentCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
            Self::Contract => "contract",
            Self::Financial => "financial",
            Self::Report => "report",
            Self::Beneficiary => "beneficiary",
            Self::Donor => "donor",
            Self::Hr => "hr",
            Self::Legal => "legal",
            Self::Other => "other",
        }
    }

    /// Classes only roles with `archive.confidential` may see (staff files, money, legal).
    pub fn is_confidential(self) -> bool {
        matches!(self, Self::Hr | Self::Financial | Self::Legal | Self::Contract)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDocument {
    pub title: String,
    pub category: DocumentCategory,
    pub reference_no: Option<String>,
    pub document_date: NaiveDate,
    pub party: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub id: Uuid,
    pub doc_no: String,
    pub title: String,
    pub category: DocumentCategory,
    pub reference_no: Option<String>,
    pub document_date: NaiveDate,
    pub party: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub voided_at: Option<DateTime<Utc>>,
    pub void_reason: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub file_count: i64,
    pub total_bytes: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DocumentFile {
    pub id: Uuid,
    pub document_id: Uuid,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub uploaded_by: String,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct FileWithData {
    #[sqlx(flatten)]
    meta: DocumentFile,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentDetail {
    #[serde(flatten)]
    pub document: Document,
    pub files: Vec<DocumentFile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DocumentQuery {
    pub q: Option<String>,
    pub category: Option<DocumentCategory>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    #[serde(default)]
    pub include_voided: bool,
    /// Set by the API for roles without access to confidential categories; never from the client.
    #[serde(skip)]
    pub hide_confidential: bool,
}

type DocumentRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    NaiveDate,
    Option<String>,
    Option<String>,
    Vec<String>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    DateTime<Utc>,
    i64,
    i64,
);

const DOCUMENT_COLUMNS: &str = "d.id, d.doc_no, d.title, d.category::text, d.reference_no, d.document_date, d.party, d.description, d.tags,
     d.voided_at, d.void_reason, d.created_by, d.created_at,
     (SELECT COUNT(*) FROM document_files f WHERE f.document_id = d.id),
     (SELECT COALESCE(SUM(f.size_bytes), 0)::bigint FROM document_files f WHERE f.document_id = d.id)";

fn document(r: DocumentRow) -> Result<Document> {
    let (id, doc_no, title, category, reference_no, document_date, party, description, tags, voided_at, void_reason, created_by, created_at, file_count, total_bytes) = r;
    Ok(Document {
        id,
        doc_no,
        title,
        category: parse_label(&category)?,
        reference_no,
        document_date,
        party,
        description,
        tags,
        voided_at,
        void_reason,
        created_by,
        created_at,
        file_count,
        total_bytes,
    })
}

fn trimmed(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Keeps the base name only and drops control characters, so a crafted name cannot break
/// the Content-Disposition header or point outside a download folder.
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().filter(|c| !c.is_control() && *c != '"').collect();
    let clean = clean.trim();
    if clean.is_empty() || clean == "." || clean == ".." { "file".to_owned() } else { clean.chars().take(200).collect() }
}

impl Store {
    pub async fn create_document(&self, d: &NewDocument, actor: &str) -> Result<Document> {
        let title = d.title.trim();
        if title.is_empty() {
            return Err(DomainError::Invalid("document title is required".into()).into());
        }
        let tags: Vec<String> = d.tags.iter().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect();
        let id = serializable!(self, |conn| {
            let (seq,): (i32,) = sqlx::query_as(
                "INSERT INTO archive_counters (year, last_no) VALUES ($1, 1)
                 ON CONFLICT (year) DO UPDATE SET last_no = archive_counters.last_no + 1 RETURNING last_no",
            )
            .bind(d.document_date.year())
            .fetch_one(&mut *conn)
            .await?;
            let doc_no = format!("ARC-{}-{seq:06}", d.document_date.year());
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO documents (doc_no, title, category, reference_no, document_date, party, description, tags, created_by)
                 VALUES ($1, $2, $3::document_category, $4, $5, $6, $7, $8, $9) RETURNING id",
            )
            .bind(&doc_no)
            .bind(title)
            .bind(d.category.as_str())
            .bind(trimmed(&d.reference_no))
            .bind(d.document_date)
            .bind(trimmed(&d.party))
            .bind(trimmed(&d.description))
            .bind(&tags)
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "document", &id.to_string(), serde_json::json!({ "doc_no": doc_no, "title": title, "category": d.category })).await?;
            Ok(id)
        })?;
        Ok(self.document(id).await?.document)
    }

    pub async fn documents(&self, q: &DocumentQuery, limit: i64) -> Result<Vec<Document>> {
        let pattern = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let rows: Vec<DocumentRow> = sqlx::query_as(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents d
              WHERE ($1::text IS NULL OR d.title ILIKE $1 OR d.doc_no ILIKE $1 OR d.reference_no ILIKE $1
                     OR d.party ILIKE $1 OR d.description ILIKE $1 OR array_to_string(d.tags, ' ') ILIKE $1
                     OR EXISTS (SELECT 1 FROM document_files f WHERE f.document_id = d.id AND f.file_name ILIKE $1))
                AND ($2::document_category IS NULL OR d.category = $2::document_category)
                AND ($3::date IS NULL OR d.document_date >= $3)
                AND ($4::date IS NULL OR d.document_date <= $4)
                AND ($5 OR d.voided_at IS NULL)
                AND (NOT $7 OR d.category NOT IN ('hr', 'financial', 'legal', 'contract'))
              ORDER BY d.document_date DESC, d.doc_no DESC
              LIMIT $6"
        ))
        .bind(pattern)
        .bind(q.category.map(DocumentCategory::as_str))
        .bind(q.from)
        .bind(q.to)
        .bind(q.include_voided)
        .bind(limit)
        .bind(q.hide_confidential)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(document).collect()
    }

    pub async fn document_category(&self, id: Uuid) -> Result<DocumentCategory> {
        let row: Option<(String,)> = sqlx::query_as("SELECT category::text FROM documents WHERE id = $1").bind(id).fetch_optional(&self.pool).await?;
        parse_label(&row.ok_or(InfraError::NotFound("document"))?.0)
    }

    pub async fn document(&self, id: Uuid) -> Result<DocumentDetail> {
        let row: Option<DocumentRow> = sqlx::query_as(&format!("SELECT {DOCUMENT_COLUMNS} FROM documents d WHERE d.id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let document = document(row.ok_or(InfraError::NotFound("document"))?)?;
        let files = sqlx::query_as(
            "SELECT id, document_id, file_name, content_type, size_bytes, sha256, uploaded_by, uploaded_at
               FROM document_files WHERE document_id = $1 ORDER BY uploaded_at, file_name",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(DocumentDetail { document, files })
    }

    /// Attaches a file; the SHA-256 recorded in the audit chain proves it was never swapped.
    pub async fn add_document_file(&self, document_id: Uuid, file_name: &str, content_type: &str, data: &[u8], actor: &str) -> Result<DocumentFile> {
        if data.is_empty() {
            return Err(DomainError::Invalid("the file is empty".into()).into());
        }
        if data.len() > MAX_FILE_BYTES {
            return Err(DomainError::Invalid(format!("file exceeds {} MB", MAX_FILE_BYTES / 1024 / 1024)).into());
        }
        let name = safe_file_name(file_name);
        let content_type = if content_type.trim().is_empty() { "application/octet-stream" } else { content_type.trim() };
        let sha256 = hex::encode(Sha256::digest(data));
        serializable!(self, |conn| {
            let state: Option<(Option<DateTime<Utc>>,)> =
                sqlx::query_as("SELECT voided_at FROM documents WHERE id = $1 FOR UPDATE").bind(document_id).fetch_optional(&mut *conn).await?;
            match state {
                None => return Err(InfraError::NotFound("document")),
                Some((Some(_),)) => return Err(InfraError::Conflict("document_voided", "document is voided".into())),
                Some((None,)) => {}
            }
            let file: DocumentFile = sqlx::query_as(
                "INSERT INTO document_files (document_id, file_name, content_type, size_bytes, sha256, data, uploaded_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, document_id, file_name, content_type, size_bytes, sha256, uploaded_by, uploaded_at",
            )
            .bind(document_id)
            .bind(&name)
            .bind(content_type)
            .bind(data.len() as i64)
            .bind(&sha256)
            .bind(data)
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(
                conn,
                actor,
                "attach",
                "document",
                &document_id.to_string(),
                serde_json::json!({ "file_id": file.id, "file_name": name, "size": data.len(), "sha256": sha256 }),
            )
            .await?;
            Ok(file)
        })
    }

    pub async fn document_file(&self, file_id: Uuid) -> Result<(DocumentFile, Vec<u8>)> {
        let row: Option<FileWithData> = sqlx::query_as(
            "SELECT id, document_id, file_name, content_type, size_bytes, sha256, uploaded_by, uploaded_at, data FROM document_files WHERE id = $1",
        )
        .bind(file_id)
        .fetch_optional(&self.pool)
        .await?;
        let row = row.ok_or(InfraError::NotFound("file"))?;
        Ok((row.meta, row.data))
    }

    pub async fn void_document(&self, id: Uuid, reason: &str, actor: &str) -> Result<()> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(DomainError::Invalid("a void needs a reason".into()).into());
        }
        serializable!(self, |conn| {
            let n = sqlx::query("UPDATE documents SET voided_at = now(), voided_by = $2, void_reason = $3 WHERE id = $1 AND voided_at IS NULL")
                .bind(id)
                .bind(actor)
                .bind(reason)
                .execute(&mut *conn)
                .await?
                .rows_affected();
            if n == 0 {
                let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM documents WHERE id = $1").bind(id).fetch_optional(&mut *conn).await?;
                return Err(match exists {
                    None => InfraError::NotFound("document"),
                    Some(_) => InfraError::Conflict("document_voided", "document is already voided".into()),
                });
            }
            audit::append(conn, actor, "void", "document", &id.to_string(), serde_json::json!({ "reason": reason })).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_reduced_to_a_safe_base_name() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("C:\\scans\\عقد.pdf"), "عقد.pdf");
        assert_eq!(safe_file_name("a\"b\r\n.pdf"), "ab.pdf");
        assert_eq!(safe_file_name(".."), "file");
        assert_eq!(safe_file_name("   "), "file");
        assert_eq!(safe_file_name(&"x".repeat(300)).chars().count(), 200);
    }
}
