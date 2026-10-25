//! Recurring sponsorships (كفالات): a donor commits a monthly amount, usually to a named
//! beneficiary (orphan, family, student…). Each payment is an ordinary donation — receipt,
//! journal entry and audit hash — linked to the months it covers, so arrears are always
//! derived from the books rather than kept as a separate balance.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::charity::DonationKind;
use yusr_domain::money::Currency;

use crate::donations::{NewDonation, RecordedDonation, record_in};
use crate::{InfraError, Result, Store, audit, parse_label, serializable};

/// Longest stretch one payment may prepay; matches the CHECK on `sponsorship_payments.months`.
pub const MAX_MONTHS_PER_PAYMENT: i16 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SponsorshipProgram {
    Orphan,
    Family,
    Student,
    Patient,
    Elderly,
    Other,
}

impl SponsorshipProgram {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orphan => "orphan",
            Self::Family => "family",
            Self::Student => "student",
            Self::Patient => "patient",
            Self::Elderly => "elderly",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SponsorshipStatus {
    Active,
    Paused,
    Ended,
}

impl SponsorshipStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Ended => "ended",
        }
    }
}

fn sadaqah() -> DonationKind {
    DonationKind::Sadaqah
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSponsorship {
    pub donor_id: Uuid,
    pub beneficiary_id: Option<Uuid>,
    pub program: SponsorshipProgram,
    /// Sadaqah (the default) goes to the designated-sadaqah fund; zakat-funded sponsorships to the zakat fund.
    #[serde(default = "sadaqah")]
    pub kind: DonationKind,
    pub fund_id: Option<Uuid>,
    pub monthly_amount: Decimal,
    pub currency: Currency,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Sponsorship {
    pub id: Uuid,
    pub sponsorship_no: i64,
    pub donor_id: Uuid,
    pub donor_name: String,
    pub beneficiary_id: Option<Uuid>,
    pub beneficiary_name: Option<String>,
    pub program: SponsorshipProgram,
    pub kind: DonationKind,
    pub fund_id: Option<Uuid>,
    pub monthly_amount: Decimal,
    pub currency: Currency,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub status: SponsorshipStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub months_due: i64,
    pub months_paid: i64,
    pub amount_paid: Decimal,
    /// Unpaid months to date for an active sponsorship; 0 otherwise.
    pub arrears_months: i64,
    pub last_payment: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SponsorshipPaymentRow {
    pub donation_id: Uuid,
    pub receipt_no: String,
    pub donation_date: NaiveDate,
    pub months: i16,
    pub amount: Decimal,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SponsorshipDetail {
    #[serde(flatten)]
    pub sponsorship: Sponsorship,
    pub payments: Vec<SponsorshipPaymentRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsorshipPayment {
    pub months: i16,
    pub donation_date: NaiveDate,
    pub payment_method: String,
    pub payment_ref: Option<String>,
    pub note: Option<String>,
}

/// Calendar months owed from the start month through the month of `min(today, end)`, inclusive.
pub fn months_due(start: NaiveDate, end: Option<NaiveDate>, today: NaiveDate) -> i64 {
    let until = end.map_or(today, |e| e.min(today));
    if until < start {
        return 0;
    }
    let index = |d: NaiveDate| i64::from(d.year()) * 12 + i64::from(d.month0());
    index(until) - index(start) + 1
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    sponsorship_no: i64,
    donor_id: Uuid,
    donor_name: String,
    beneficiary_id: Option<Uuid>,
    beneficiary_name: Option<String>,
    program: String,
    kind: String,
    fund_id: Option<Uuid>,
    monthly_amount: Decimal,
    currency: String,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    status: String,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    months_paid: i64,
    amount_paid: Decimal,
    last_payment: Option<NaiveDate>,
}

const SELECT: &str = "SELECT s.id, s.sponsorship_no, s.donor_id, o.full_name AS donor_name, s.beneficiary_id, b.full_name AS beneficiary_name,
        s.program::text AS program, s.kind::text AS kind, s.fund_id, s.monthly_amount, s.currency::text AS currency, s.start_date, s.end_date,
        s.status::text AS status, s.notes, s.created_at,
        COALESCE(SUM(p.months), 0)::bigint AS months_paid, COALESCE(SUM(d.amount), 0) AS amount_paid, MAX(d.donation_date) AS last_payment
   FROM sponsorships s
   JOIN donors o ON o.id = s.donor_id
   LEFT JOIN beneficiaries b ON b.id = s.beneficiary_id
   LEFT JOIN sponsorship_payments p ON p.sponsorship_id = s.id
   LEFT JOIN donations d ON d.id = p.donation_id";
const GROUP: &str = "GROUP BY s.id, o.full_name, b.full_name";

fn sponsorship(r: Row, today: NaiveDate) -> Result<Sponsorship> {
    let status: SponsorshipStatus = parse_label(&r.status)?;
    let due = months_due(r.start_date, r.end_date, today);
    Ok(Sponsorship {
        program: parse_label(&r.program)?,
        kind: parse_label(&r.kind)?,
        currency: r.currency.parse()?,
        arrears_months: if status == SponsorshipStatus::Active { (due - r.months_paid).max(0) } else { 0 },
        months_due: due,
        status,
        id: r.id,
        sponsorship_no: r.sponsorship_no,
        donor_id: r.donor_id,
        donor_name: r.donor_name,
        beneficiary_id: r.beneficiary_id,
        beneficiary_name: r.beneficiary_name,
        fund_id: r.fund_id,
        monthly_amount: r.monthly_amount,
        start_date: r.start_date,
        end_date: r.end_date,
        notes: r.notes,
        created_at: r.created_at,
        months_paid: r.months_paid,
        amount_paid: r.amount_paid,
        last_payment: r.last_payment,
    })
}

impl Store {
    pub async fn create_sponsorship(&self, n: &NewSponsorship, actor: &str) -> Result<Sponsorship> {
        if n.monthly_amount <= Decimal::ZERO || n.monthly_amount != n.currency.round(n.monthly_amount) {
            return Err(DomainError::NonPositiveAmount { line: 0, amount: n.monthly_amount }.into());
        }
        if n.end_date.is_some_and(|e| e < n.start_date) {
            return Err(DomainError::Invalid("end_date is before start_date".into()).into());
        }
        let id = serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO sponsorships (donor_id, beneficiary_id, program, kind, fund_id, monthly_amount, currency, start_date, end_date, notes, created_by)
                 VALUES ($1, $2, $3::sponsorship_program, $4::donation_kind, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
            )
            .bind(n.donor_id)
            .bind(n.beneficiary_id)
            .bind(n.program.as_str())
            .bind(n.kind.as_str())
            .bind(n.fund_id)
            .bind(n.monthly_amount)
            .bind(n.currency.as_str())
            .bind(n.start_date)
            .bind(n.end_date)
            .bind(n.notes.as_deref().map(str::trim).filter(|s| !s.is_empty()))
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "sponsorship", &id.to_string(), serde_json::to_value(n).expect("serializes")).await?;
            Ok(id)
        })?;
        Ok(self.sponsorship(id).await?.sponsorship)
    }

    pub async fn sponsorships(&self, status: Option<SponsorshipStatus>, beneficiary_id: Option<Uuid>, limit: i64) -> Result<Vec<Sponsorship>> {
        let today = Utc::now().date_naive();
        let rows: Vec<Row> = sqlx::query_as(&format!(
            "{SELECT} WHERE ($1::sponsorship_status IS NULL OR s.status = $1::sponsorship_status)
                        AND ($2::uuid IS NULL OR s.beneficiary_id = $2)
              {GROUP} ORDER BY s.sponsorship_no DESC LIMIT $3"
        ))
        .bind(status.map(SponsorshipStatus::as_str))
        .bind(beneficiary_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|r| sponsorship(r, today)).collect()
    }

    pub async fn sponsorship(&self, id: Uuid) -> Result<SponsorshipDetail> {
        let row: Option<Row> = sqlx::query_as(&format!("{SELECT} WHERE s.id = $1 {GROUP}")).bind(id).fetch_optional(&self.pool).await?;
        let sponsorship = sponsorship(row.ok_or(InfraError::NotFound("sponsorship"))?, Utc::now().date_naive())?;
        let payments = sqlx::query_as(
            "SELECT d.id AS donation_id, d.receipt_no, d.donation_date, p.months, d.amount, d.currency::text AS currency
               FROM sponsorship_payments p JOIN donations d ON d.id = p.donation_id
              WHERE p.sponsorship_id = $1 ORDER BY d.donation_date DESC, d.receipt_no DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(SponsorshipDetail { sponsorship, payments })
    }

    /// Records `months × monthly_amount` as a donation from the sponsor, in one transaction
    /// with its link, so a payment can never exist without its receipt or vice versa.
    pub async fn pay_sponsorship(&self, id: Uuid, p: &SponsorshipPayment, actor: &str) -> Result<RecordedDonation> {
        if !(1..=MAX_MONTHS_PER_PAYMENT).contains(&p.months) {
            return Err(DomainError::Invalid(format!("months must be between 1 and {MAX_MONTHS_PER_PAYMENT}")).into());
        }
        let base = self.base_currency;
        serializable!(self, |conn| {
            type S = (Uuid, i64, String, Option<Uuid>, Decimal, String, String);
            let row: Option<S> = sqlx::query_as(
                "SELECT donor_id, sponsorship_no, kind::text, fund_id, monthly_amount, currency::text, status::text FROM sponsorships WHERE id = $1 FOR UPDATE",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            let (donor_id, no, kind, fund_id, monthly, currency, status) = row.ok_or(InfraError::NotFound("sponsorship"))?;
            if status == "ended" {
                return Err(InfraError::Conflict("sponsorship_ended", format!("sponsorship #{no} has ended")));
            }
            let kind: DonationKind = parse_label(&kind)?;
            let donation = NewDonation {
                donor_id,
                donation_date: p.donation_date,
                kind: Some(kind),
                // Sponsorship money is earmarked by the sponsor, which routes sadaqah to the designated fund.
                donor_designated: kind == DonationKind::Sadaqah,
                fund_id,
                project_id: None,
                amount: monthly * Decimal::from(p.months),
                currency: currency.parse()?,
                payment_method: p.payment_method.clone(),
                payment_ref: p.payment_ref.clone(),
                note: Some(p.note.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("كفالة رقم {no} — {} شهر", p.months))),
            };
            let recorded = record_in(conn, base, &donation, actor).await?;
            sqlx::query("INSERT INTO sponsorship_payments (sponsorship_id, donation_id, months) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(recorded.donation_id)
                .bind(p.months)
                .execute(&mut *conn)
                .await?;
            audit::append(
                conn,
                actor,
                "pay",
                "sponsorship",
                &id.to_string(),
                serde_json::json!({ "donation_id": recorded.donation_id, "receipt_no": recorded.receipt_no, "months": p.months }),
            )
            .await?;
            Ok(recorded)
        })
    }

    pub async fn set_sponsorship_status(&self, id: Uuid, status: SponsorshipStatus, actor: &str) -> Result<Sponsorship> {
        serializable!(self, |conn| {
            let n = sqlx::query(
                "UPDATE sponsorships SET status = $2::sponsorship_status,
                        end_date = CASE WHEN $2 = 'ended' THEN COALESCE(end_date, CURRENT_DATE) ELSE end_date END
                  WHERE id = $1",
            )
            .bind(id)
            .bind(status.as_str())
            .execute(&mut *conn)
            .await?
            .rows_affected();
            if n == 0 {
                return Err(InfraError::NotFound("sponsorship"));
            }
            audit::append(conn, actor, "update", "sponsorship", &id.to_string(), serde_json::json!({ "status": status })).await?;
            Ok(())
        })?;
        Ok(self.sponsorship(id).await?.sponsorship)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn months_due_counts_calendar_months_inclusively() {
        assert_eq!(months_due(d(2026, 1, 15), None, d(2026, 1, 20)), 1);
        assert_eq!(months_due(d(2026, 1, 31), None, d(2026, 3, 1)), 3);
        assert_eq!(months_due(d(2025, 11, 1), None, d(2026, 2, 1)), 4);
        assert_eq!(months_due(d(2026, 5, 1), None, d(2026, 4, 30)), 0);
        // The end date caps the obligation even when today is later.
        assert_eq!(months_due(d(2026, 1, 1), Some(d(2026, 6, 30)), d(2026, 12, 1)), 6);
    }
}
