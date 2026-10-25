//! Outgoing correspondence (letters and telegrams). Drafts are editable; issuing assigns the
//! next number in the Hijri-year register and freezes the letter.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::hijri::HijriDate;

use crate::{InfraError, Result, Store, audit, parse_label, serializable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LetterKind {
    Official,
    Congratulation,
    Condolence,
    Thanks,
    Invitation,
    Circular,
}

impl LetterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Congratulation => "congratulation",
            Self::Condolence => "condolence",
            Self::Thanks => "thanks",
            Self::Invitation => "invitation",
            Self::Circular => "circular",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetterDraft {
    pub kind: LetterKind,
    pub recipient_name: String,
    pub recipient_title: Option<String>,
    pub recipient_org: Option<String>,
    pub subject: String,
    pub body: String,
    pub closing: Option<String>,
    pub signatory_name: String,
    pub signatory_title: Option<String>,
    pub attachments: Option<String>,
    pub cc: Option<String>,
    pub letter_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize)]
pub struct Letter {
    pub id: Uuid,
    pub ref_no: Option<String>,
    pub kind: LetterKind,
    pub recipient_name: String,
    pub recipient_title: Option<String>,
    pub recipient_org: Option<String>,
    pub subject: String,
    pub body: String,
    pub closing: Option<String>,
    pub signatory_name: String,
    pub signatory_title: Option<String>,
    pub attachments: Option<String>,
    pub cc: Option<String>,
    pub letter_date: NaiveDate,
    /// Umm al-Qura date of `letter_date`, `YYYY-MM-DD`.
    pub hijri_date: String,
    pub status: String,
    pub issued_at: Option<DateTime<Utc>>,
    pub issued_by: Option<String>,
    pub archive_id: Option<Uuid>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LetterRow {
    id: Uuid,
    ref_no: Option<String>,
    kind: String,
    recipient_name: String,
    recipient_title: Option<String>,
    recipient_org: Option<String>,
    subject: String,
    body: String,
    closing: Option<String>,
    signatory_name: String,
    signatory_title: Option<String>,
    attachments: Option<String>,
    cc: Option<String>,
    letter_date: NaiveDate,
    status: String,
    issued_at: Option<DateTime<Utc>>,
    issued_by: Option<String>,
    archive_id: Option<Uuid>,
    created_by: String,
    created_at: DateTime<Utc>,
}

const LETTER_COLUMNS: &str = "id, ref_no, kind::text AS kind, recipient_name, recipient_title, recipient_org, subject, body, closing,
     signatory_name, signatory_title, attachments, cc, letter_date, status, issued_at, issued_by, archive_id, created_by, created_at";

fn letter(r: LetterRow) -> Result<Letter> {
    Ok(Letter {
        hijri_date: HijriDate::from_gregorian(r.letter_date)?.to_string(),
        kind: parse_label(&r.kind)?,
        id: r.id,
        ref_no: r.ref_no,
        recipient_name: r.recipient_name,
        recipient_title: r.recipient_title,
        recipient_org: r.recipient_org,
        subject: r.subject,
        body: r.body,
        closing: r.closing,
        signatory_name: r.signatory_name,
        signatory_title: r.signatory_title,
        attachments: r.attachments,
        cc: r.cc,
        letter_date: r.letter_date,
        status: r.status,
        issued_at: r.issued_at,
        issued_by: r.issued_by,
        archive_id: r.archive_id,
        created_by: r.created_by,
        created_at: r.created_at,
    })
}

fn opt(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn validate(d: &LetterDraft) -> Result<()> {
    for (field, value) in [("recipient_name", &d.recipient_name), ("subject", &d.subject), ("body", &d.body), ("signatory_name", &d.signatory_name)] {
        if value.trim().is_empty() {
            return Err(DomainError::Invalid(format!("{field} is required")).into());
        }
    }
    // Fails early for dates the Umm al-Qura table cannot number.
    HijriDate::from_gregorian(d.letter_date)?;
    Ok(())
}

impl Store {
    pub async fn create_letter(&self, d: &LetterDraft, actor: &str) -> Result<Letter> {
        validate(d)?;
        let id = serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO letters (kind, recipient_name, recipient_title, recipient_org, subject, body, closing,
                                      signatory_name, signatory_title, attachments, cc, letter_date, created_by)
                 VALUES ($1::letter_kind, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
            )
            .bind(d.kind.as_str())
            .bind(d.recipient_name.trim())
            .bind(opt(&d.recipient_title))
            .bind(opt(&d.recipient_org))
            .bind(d.subject.trim())
            .bind(d.body.trim())
            .bind(opt(&d.closing))
            .bind(d.signatory_name.trim())
            .bind(opt(&d.signatory_title))
            .bind(opt(&d.attachments))
            .bind(opt(&d.cc))
            .bind(d.letter_date)
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "letter", &id.to_string(), serde_json::json!({ "kind": d.kind, "subject": d.subject.trim() })).await?;
            Ok(id)
        })?;
        self.letter(id).await
    }

    pub async fn update_letter(&self, id: Uuid, d: &LetterDraft, actor: &str) -> Result<Letter> {
        validate(d)?;
        serializable!(self, |conn| {
            let n = sqlx::query(
                "UPDATE letters SET kind = $2::letter_kind, recipient_name = $3, recipient_title = $4, recipient_org = $5, subject = $6,
                        body = $7, closing = $8, signatory_name = $9, signatory_title = $10, attachments = $11, cc = $12,
                        letter_date = $13, updated_at = now()
                  WHERE id = $1 AND status = 'draft'",
            )
            .bind(id)
            .bind(d.kind.as_str())
            .bind(d.recipient_name.trim())
            .bind(opt(&d.recipient_title))
            .bind(opt(&d.recipient_org))
            .bind(d.subject.trim())
            .bind(d.body.trim())
            .bind(opt(&d.closing))
            .bind(d.signatory_name.trim())
            .bind(opt(&d.signatory_title))
            .bind(opt(&d.attachments))
            .bind(opt(&d.cc))
            .bind(d.letter_date)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            if n == 0 {
                return Err(self_state_error(conn, id).await);
            }
            audit::append(conn, actor, "update", "letter", &id.to_string(), serde_json::json!({ "subject": d.subject.trim() })).await?;
            Ok(())
        })?;
        self.letter(id).await
    }

    /// Assigns `YYYY/NNNN` from the register of the letter's Hijri year (e.g. `1448/0007`).
    pub async fn issue_letter(&self, id: Uuid, actor: &str) -> Result<Letter> {
        serializable!(self, |conn| {
            let row: Option<(NaiveDate, String)> =
                sqlx::query_as("SELECT letter_date, status FROM letters WHERE id = $1 FOR UPDATE").bind(id).fetch_optional(&mut *conn).await?;
            let (date, status) = row.ok_or(InfraError::NotFound("letter"))?;
            if status != "draft" {
                return Err(InfraError::Conflict("letter_issued", "letter is already issued".into()));
            }
            let year = HijriDate::from_gregorian(date)?.year;
            let (seq,): (i32,) = sqlx::query_as(
                "INSERT INTO letter_counters (hijri_year, last_no) VALUES ($1, 1)
                 ON CONFLICT (hijri_year) DO UPDATE SET last_no = letter_counters.last_no + 1 RETURNING last_no",
            )
            .bind(year)
            .fetch_one(&mut *conn)
            .await?;
            let ref_no = format!("{year}/{seq:04}");
            sqlx::query("UPDATE letters SET status = 'issued', ref_no = $2, issued_at = now(), issued_by = $3 WHERE id = $1")
                .bind(id)
                .bind(&ref_no)
                .bind(actor)
                .execute(&mut *conn)
                .await?;
            audit::append(conn, actor, "issue", "letter", &id.to_string(), serde_json::json!({ "ref_no": ref_no })).await?;
            Ok(())
        })?;
        self.letter(id).await
    }

    /// Links the archived copy of an issued letter; allowed once (enforced by a trigger too).
    pub async fn link_letter_archive(&self, id: Uuid, document_id: Uuid, actor: &str) -> Result<Letter> {
        serializable!(self, |conn| {
            let n = sqlx::query("UPDATE letters SET archive_id = $2 WHERE id = $1 AND status = 'issued' AND archive_id IS NULL")
                .bind(id)
                .bind(document_id)
                .execute(&mut *conn)
                .await?
                .rows_affected();
            if n == 0 {
                return Err(InfraError::Conflict("letter_not_archivable", "only an issued, not yet archived letter can be linked".into()));
            }
            audit::append(conn, actor, "archive", "letter", &id.to_string(), serde_json::json!({ "document_id": document_id })).await?;
            Ok(())
        })?;
        self.letter(id).await
    }

    pub async fn letters(&self, status: Option<&str>, q: Option<&str>, limit: i64) -> Result<Vec<Letter>> {
        let pattern = q.map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let rows: Vec<LetterRow> = sqlx::query_as(&format!(
            "SELECT {LETTER_COLUMNS} FROM letters
              WHERE ($1::text IS NULL OR status = $1)
                AND ($2::text IS NULL OR subject ILIKE $2 OR recipient_name ILIKE $2 OR recipient_org ILIKE $2 OR ref_no ILIKE $2)
              ORDER BY created_at DESC LIMIT $3"
        ))
        .bind(status)
        .bind(pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(letter).collect()
    }

    pub async fn letter(&self, id: Uuid) -> Result<Letter> {
        let row: Option<LetterRow> =
            sqlx::query_as(&format!("SELECT {LETTER_COLUMNS} FROM letters WHERE id = $1")).bind(id).fetch_optional(&self.pool).await?;
        letter(row.ok_or(InfraError::NotFound("letter"))?)
    }
}

/// Why a draft-only update matched no row: the letter is missing or already issued.
async fn self_state_error(conn: &mut sqlx::PgConnection, id: Uuid) -> InfraError {
    match sqlx::query_as::<_, (String,)>("SELECT status FROM letters WHERE id = $1").bind(id).fetch_optional(conn).await {
        Ok(Some(_)) => InfraError::Conflict("letter_issued", "an issued letter cannot be edited".into()),
        Ok(None) => InfraError::NotFound("letter"),
        Err(e) => e.into(),
    }
}
