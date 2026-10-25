//! Donor CRM and donation intake: classification, fund routing, posting and receipts.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::charity::DonationKind;
use yusr_domain::ledger::{EntryDraft, LineDraft, Side};
use yusr_domain::money::Currency;
use yusr_domain::receipt::{ReceiptData, render_receipt, render_zakat_certificate};

use crate::ledger::{PostedEntry, account_id, codes, fund, post_in};
use crate::{InfraError, Result, Store, audit, parse_label, serializable};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Donor {
    pub id: Uuid,
    pub is_organization: bool,
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub country: Option<String>,
    pub preferred_language: String,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDonor {
    #[serde(default)]
    pub is_organization: bool,
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub country: Option<String>,
    #[serde(default = "arabic")]
    pub preferred_language: String,
    pub notes: Option<String>,
}

fn arabic() -> String {
    "ar".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDonation {
    pub donor_id: Uuid,
    pub donation_date: NaiveDate,
    /// When omitted, the type is inferred from `note` (e.g. "زكاة مال").
    pub kind: Option<DonationKind>,
    #[serde(default)]
    pub donor_designated: bool,
    /// When omitted, the default fund for the type is used.
    pub fund_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub amount: Decimal,
    pub currency: Currency,
    /// `cash`, `bank`, `cheque`, or a gateway name (`mada`, `knet`, `apple_pay`, …).
    pub payment_method: String,
    pub payment_ref: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordedDonation {
    pub donation_id: Uuid,
    pub receipt_no: String,
    pub kind: DonationKind,
    pub fund_id: Uuid,
    pub entry: PostedEntry,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DonationRow {
    pub id: Uuid,
    pub receipt_no: String,
    pub donor_id: Uuid,
    pub donor_name: String,
    pub donation_date: NaiveDate,
    pub kind: String,
    pub fund_code: String,
    pub amount: Decimal,
    pub currency: String,
    pub base_amount: Decimal,
    pub payment_method: String,
    pub payment_ref: Option<String>,
    pub note: Option<String>,
}

fn deposit_account(method: &str) -> &'static str {
    match method {
        "cash" => codes::CASH,
        "bank" | "transfer" | "cheque" | "sadad" => codes::BANK,
        _ => codes::GATEWAY_CLEARING,
    }
}

fn default_fund_code(kind: DonationKind, designated: bool) -> &'static str {
    match kind {
        DonationKind::Zakat => "ZAKAT",
        DonationKind::Kaffarah => "KAFF",
        DonationKind::Waqf => "WAQF",
        DonationKind::Sadaqah if designated => "SADQ",
        DonationKind::Sadaqah | DonationKind::General => "GEN",
    }
}

/// Records a donation inside the caller's SERIALIZABLE transaction.
pub async fn record_in(conn: &mut PgConnection, base: Currency, d: &NewDonation, actor: &str) -> Result<RecordedDonation> {
    let kind = match d.kind {
        Some(k) => k,
        None => d.note.as_deref().and_then(DonationKind::classify_note).ok_or_else(|| {
            DomainError::Invalid("donation type missing and the note does not name one (zakat, sadaqah, kaffarah, waqf)".into())
        })?,
    };
    if d.payment_method.trim().is_empty() {
        return Err(DomainError::Invalid("payment_method is required".into()).into());
    }
    let fund_id = match d.fund_id {
        Some(id) => id,
        None => {
            let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM funds WHERE code = $1")
                .bind(default_fund_code(kind, d.donor_designated))
                .fetch_one(&mut *conn)
                .await?;
            id
        }
    };
    let f = fund(conn, fund_id).await?;
    kind.check_fund(d.donor_designated, f.restriction)?;
    if let Some(fund_kind) = f.donation_kind {
        if fund_kind != kind {
            return Err(DomainError::RestrictionMismatch { kind: kind.as_str(), restriction: fund_kind.as_str() }.into());
        }
    }
    let donor: Option<(String,)> = sqlx::query_as("SELECT full_name FROM donors WHERE id = $1").bind(d.donor_id).fetch_optional(&mut *conn).await?;
    let (donor_name,) = donor.ok_or(InfraError::NotFound("donor"))?;

    let (seq,): (i32,) = sqlx::query_as(
        "INSERT INTO receipt_counters (year, last_no) VALUES ($1, 1)
         ON CONFLICT (year) DO UPDATE SET last_no = receipt_counters.last_no + 1 RETURNING last_no",
    )
    .bind(d.donation_date.year())
    .fetch_one(&mut *conn)
    .await?;
    let receipt_no = format!("R-{}-{seq:06}", d.donation_date.year());

    let debit = account_id(conn, deposit_account(&d.payment_method)).await?;
    let credit = account_id(conn, codes::revenue_for(kind)).await?;
    let line = |account_id, side| LineDraft {
        account_id,
        fund_id,
        project_id: d.project_id,
        side,
        amount: d.amount,
        currency: d.currency,
        memo: Some(receipt_no.clone()),
    };
    let draft = EntryDraft {
        entry_date: d.donation_date,
        description: format!("{} donation from {donor_name} ({receipt_no})", kind.as_str()),
        reference: Some(receipt_no.clone()),
        lines: vec![line(debit, Side::Debit), line(credit, Side::Credit)],
    };
    let entry = post_in(conn, base, &draft, actor, "donation", None).await?;

    let donation_id = Uuid::new_v4();
    let audit_hash = hex::decode(&entry.audit_hash).expect("hex we produced");
    sqlx::query(
        "INSERT INTO donations (id, receipt_no, donor_id, donation_date, kind, donor_designated, fund_id, project_id, amount,
                                currency, base_amount, payment_method, payment_ref, note, journal_entry_id, audit_hash, created_by)
         VALUES ($1, $2, $3, $4, $5::donation_kind, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(donation_id)
    .bind(&receipt_no)
    .bind(d.donor_id)
    .bind(d.donation_date)
    .bind(kind.as_str())
    .bind(d.donor_designated)
    .bind(fund_id)
    .bind(d.project_id)
    .bind(d.amount)
    .bind(d.currency.as_str())
    .bind(entry.lines[0].base_amount)
    .bind(d.payment_method.trim())
    .bind(&d.payment_ref)
    .bind(&d.note)
    .bind(entry.id)
    .bind(&audit_hash)
    .bind(actor)
    .execute(&mut *conn)
    .await?;
    Ok(RecordedDonation { donation_id, receipt_no, kind, fund_id, entry })
}

impl Store {
    pub async fn create_donor(&self, d: &NewDonor, actor: &str) -> Result<Uuid> {
        if d.full_name.trim().is_empty() {
            return Err(DomainError::Invalid("full_name is required".into()).into());
        }
        if !matches!(d.preferred_language.as_str(), "ar" | "en") {
            return Err(DomainError::Invalid("preferred_language must be ar or en".into()).into());
        }
        serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO donors (is_organization, full_name, national_id, phone, email, country, preferred_language, notes)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
            )
            .bind(d.is_organization)
            .bind(d.full_name.trim())
            .bind(&d.national_id)
            .bind(&d.phone)
            .bind(&d.email)
            .bind(&d.country)
            .bind(&d.preferred_language)
            .bind(&d.notes)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "donor", &id.to_string(), serde_json::to_value(d).expect("serializes")).await?;
            Ok(id)
        })
    }

    pub async fn search_donors(&self, query: &str, limit: i64) -> Result<Vec<Donor>> {
        let pattern = format!("%{}%", query.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        Ok(sqlx::query_as(
            "SELECT * FROM donors WHERE full_name ILIKE $1 OR national_id ILIKE $1 OR phone ILIKE $1 OR email ILIKE $1
              ORDER BY full_name LIMIT $2",
        )
        .bind(pattern)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn donor(&self, id: Uuid) -> Result<Donor> {
        sqlx::query_as("SELECT * FROM donors WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(InfraError::NotFound("donor"))
    }

    pub async fn record_donation(&self, d: &NewDonation, actor: &str) -> Result<RecordedDonation> {
        if d.amount <= Decimal::ZERO {
            return Err(DomainError::NonPositiveAmount { line: 0, amount: d.amount }.into());
        }
        let base = self.base_currency;
        serializable!(self, |conn| record_in(conn, base, d, actor).await)
    }

    pub async fn donations(&self, donor_id: Option<Uuid>, from: NaiveDate, to: NaiveDate, limit: i64) -> Result<Vec<DonationRow>> {
        Ok(sqlx::query_as(
            "SELECT d.id, d.receipt_no, d.donor_id, r.full_name AS donor_name, d.donation_date, d.kind::text AS kind,
                    f.code AS fund_code, d.amount, d.currency::text AS currency, d.base_amount, d.payment_method, d.payment_ref, d.note
               FROM donations d JOIN donors r ON r.id = d.donor_id JOIN funds f ON f.id = d.fund_id
              WHERE ($1::uuid IS NULL OR d.donor_id = $1) AND d.donation_date BETWEEN $2 AND $3
              ORDER BY d.donation_date DESC, d.receipt_no DESC LIMIT $4",
        )
        .bind(donor_id)
        .bind(from)
        .bind(to)
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?)
    }

    /// Printable HTML receipt, or the Zakat certificate when `zakat_certificate` is set.
    pub async fn receipt_html(&self, donation_id: Uuid, zakat_certificate: bool) -> Result<String> {
        type Row = (String, NaiveDate, String, Option<String>, Decimal, String, String, String, String, String, Vec<u8>);
        let row: Option<Row> = sqlx::query_as(
            "SELECT d.receipt_no, d.donation_date, r.full_name, r.national_id, d.amount, d.currency::text, d.kind::text,
                    f.name_ar, f.name_en, d.payment_method, d.audit_hash
               FROM donations d JOIN donors r ON r.id = d.donor_id JOIN funds f ON f.id = d.fund_id WHERE d.id = $1",
        )
        .bind(donation_id)
        .fetch_optional(&self.pool)
        .await?;
        let (receipt_no, date, donor_name, donor_national_id, amount, currency, kind, fund_name_ar, fund_name_en, payment_method, hash) =
            row.ok_or(InfraError::NotFound("donation"))?;
        let kind: DonationKind = parse_label(&kind)?;
        if zakat_certificate && kind != DonationKind::Zakat {
            return Err(DomainError::Invalid("a Zakat certificate is only issued for Zakat donations".into()).into());
        }
        let data = ReceiptData {
            receipt_no,
            date,
            donor_name,
            donor_national_id,
            amount,
            currency: currency.parse()?,
            kind,
            fund_name_ar,
            fund_name_en,
            payment_method,
            audit_hash: hex::encode(hash),
        };
        let org = self.organization().await?;
        Ok(if zakat_certificate { render_zakat_certificate(&org, &data) } else { render_receipt(&org, &data) })
    }
}
