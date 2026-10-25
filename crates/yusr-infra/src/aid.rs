//! Beneficiary case management and aid distribution (cash, in-kind, digital vouchers).

use chrono::{DateTime, NaiveDate, Utc};
use rand::RngCore;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::charity::{Asnaf, DonationKind};
use yusr_domain::ledger::{EntryDraft, LineDraft, Side};
use yusr_domain::money::Currency;
use yusr_domain::scoring::{CaseProfile, Weights, score};
use yusr_domain::voucher;

use crate::ledger::{PostedEntry, account_id, codes, fund, post_in};
use crate::{InfraError, Result, Store, audit, parse_label, serializable};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewBeneficiary {
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub city: Option<String>,
    pub category: Asnaf,
    pub profile: CaseProfile,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Beneficiary {
    pub id: Uuid,
    pub file_no: i64,
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub city: Option<String>,
    pub category: String,
    pub profile: serde_json::Value,
    pub score: i16,
    pub priority: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AidKind {
    Cash,
    InKind,
    Voucher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDistribution {
    pub beneficiary_id: Uuid,
    pub kind: AidKind,
    pub fund_id: Uuid,
    pub project_id: Option<Uuid>,
    pub amount: Decimal,
    pub currency: Currency,
    pub description: String,
    /// In-kind item list, e.g. `[{"item": "food basket", "qty": 2}]`.
    pub items: Option<serde_json::Value>,
    pub distributed_on: NaiveDate,
    /// Cash aid paid from the bank instead of the cash box.
    #[serde(default)]
    pub paid_from_bank: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Distribution {
    pub id: Uuid,
    pub voucher_code: Option<String>,
    pub entry: PostedEntry,
}

const SELECT_BENEFICIARY: &str = "SELECT id, file_no, full_name, national_id, phone, city, category::text AS category, profile,
        score, priority, status::text AS status, created_at FROM beneficiaries";

impl Store {
    /// Opens a case and scores it with the default weights.
    pub async fn create_beneficiary(&self, b: &NewBeneficiary, actor: &str) -> Result<Beneficiary> {
        if b.full_name.trim().is_empty() {
            return Err(DomainError::Invalid("full_name is required".into()).into());
        }
        let s = score(&b.profile, &Weights::default())?;
        let profile = serde_json::to_value(&b.profile).expect("profile serializes");
        let priority = serde_json::to_value(s.priority).expect("serializes").as_str().unwrap_or_default().to_owned();
        let id = serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO beneficiaries (full_name, national_id, phone, city, category, profile, score, priority)
                 VALUES ($1, $2, $3, $4, $5::asnaf, $6, $7, $8) RETURNING id",
            )
            .bind(b.full_name.trim())
            .bind(&b.national_id)
            .bind(&b.phone)
            .bind(&b.city)
            .bind(b.category.as_str())
            .bind(&profile)
            .bind(i16::from(s.score))
            .bind(&priority)
            .fetch_one(&mut *conn)
            .await?;
            sqlx::query("INSERT INTO case_events (beneficiary_id, event_type, note, actor) VALUES ($1, 'assessment', $2, $3)")
                .bind(id)
                .bind(format!("Initial assessment: score {} ({priority})", s.score))
                .bind(actor)
                .execute(&mut *conn)
                .await?;
            audit::append(conn, actor, "create", "beneficiary", &id.to_string(), serde_json::json!({ "category": b.category, "score": s.score })).await?;
            Ok(id)
        })?;
        self.beneficiary(id).await
    }

    /// Re-scores a case after a home visit or a change in circumstances.
    pub async fn reassess(&self, id: Uuid, profile: &CaseProfile, note: &str, actor: &str) -> Result<Beneficiary> {
        let s = score(profile, &Weights::default())?;
        let value = serde_json::to_value(profile).expect("profile serializes");
        let priority = serde_json::to_value(s.priority).expect("serializes").as_str().unwrap_or_default().to_owned();
        serializable!(self, |conn| {
            let n = sqlx::query("UPDATE beneficiaries SET profile = $2, score = $3, priority = $4, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(&value)
                .bind(i16::from(s.score))
                .bind(&priority)
                .execute(&mut *conn)
                .await?
                .rows_affected();
            if n == 0 {
                return Err(InfraError::NotFound("beneficiary"));
            }
            sqlx::query("INSERT INTO case_events (beneficiary_id, event_type, note, actor) VALUES ($1, 'reassessment', $2, $3)")
                .bind(id)
                .bind(format!("Score {} ({priority}). {note}", s.score))
                .bind(actor)
                .execute(&mut *conn)
                .await?;
            audit::append(conn, actor, "reassess", "beneficiary", &id.to_string(), serde_json::json!({ "score": s.score, "note": note })).await?;
            Ok(())
        })?;
        self.beneficiary(id).await
    }

    pub async fn beneficiary(&self, id: Uuid) -> Result<Beneficiary> {
        sqlx::query_as(&format!("{SELECT_BENEFICIARY} WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(InfraError::NotFound("beneficiary"))
    }

    /// Active cases (or every case with `all`), most vulnerable first.
    pub async fn beneficiaries(&self, limit: i64, all: bool) -> Result<Vec<Beneficiary>> {
        Ok(sqlx::query_as(&format!("{SELECT_BENEFICIARY} WHERE ($2 OR status = 'active') ORDER BY status, score DESC, file_no LIMIT $1"))
            .bind(limit.clamp(1, 500))
            .bind(all)
            .fetch_all(&self.pool)
            .await?)
    }

    /// Records aid and posts it to the ledger, enforcing Shari'ah eligibility of the
    /// fund's money, restricted-fund balances and budgets.
    pub async fn distribute(&self, d: &NewDistribution, actor: &str) -> Result<Distribution> {
        if d.amount <= Decimal::ZERO {
            return Err(DomainError::NonPositiveAmount { line: 0, amount: d.amount }.into());
        }
        if d.description.trim().is_empty() {
            return Err(DomainError::Invalid("description is required".into()).into());
        }
        let base = self.base_currency;
        let secret = self.voucher_secret();
        serializable!(self, |conn| distribute_in(conn, base, &secret, d, actor).await)
    }

    /// Marks a voucher redeemed by a partner shop and settles it from the bank.
    pub async fn redeem_voucher(&self, code: &str, redeemed_on: NaiveDate, shop: &str, actor: &str) -> Result<PostedEntry> {
        let canonical = voucher::verify(&self.voucher_secret(), code)?;
        let base = self.base_currency;
        serializable!(self, |conn| {
            let row: Option<(Uuid, String, Uuid, Option<Uuid>, Decimal, String)> = sqlx::query_as(
                "SELECT id, voucher_status::text, fund_id, project_id, amount, currency::text FROM aid_distributions
                  WHERE voucher_code = $1 FOR UPDATE",
            )
            .bind(&canonical)
            .fetch_optional(&mut *conn)
            .await?;
            let (id, status, fund_id, project_id, amount, currency) = row.ok_or(InfraError::NotFound("voucher"))?;
            if status != "issued" {
                return Err(InfraError::Conflict("voucher_not_redeemable", format!("voucher is {status}")));
            }
            let liability = account_id(conn, codes::VOUCHERS_OUTSTANDING).await?;
            let bank = account_id(conn, codes::BANK).await?;
            let currency: Currency = currency.parse()?;
            let line = |account_id, side| LineDraft { account_id, fund_id, project_id, side, amount, currency, memo: Some(canonical.clone()) };
            let draft = EntryDraft {
                entry_date: redeemed_on,
                description: format!("Voucher {canonical} redeemed at {shop}"),
                reference: Some(canonical.clone()),
                lines: vec![line(liability, Side::Debit), line(bank, Side::Credit)],
            };
            let entry = post_in(conn, base, &draft, actor, "voucher_redemption", None).await?;
            sqlx::query("UPDATE aid_distributions SET voucher_status = 'redeemed', redeemed_at = now(), redeemed_by = $2 WHERE id = $1")
                .bind(id)
                .bind(shop)
                .execute(&mut *conn)
                .await?;
            Ok(entry)
        })
    }
}

async fn distribute_in(conn: &mut PgConnection, base: Currency, secret: &[u8], d: &NewDistribution, actor: &str) -> Result<Distribution> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT category::text, status::text, full_name FROM beneficiaries WHERE id = $1 FOR UPDATE")
            .bind(d.beneficiary_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (category, status, name) = row.ok_or(InfraError::NotFound("beneficiary"))?;
    if status != "active" {
        return Err(InfraError::Conflict("case_not_active", format!("case is {status}")));
    }
    let category: Asnaf = parse_label(&category)?;
    let f = fund(conn, d.fund_id).await?;
    // The fund's giving type decides who may receive its money; general funds accept anyone.
    f.donation_kind.unwrap_or(DonationKind::General).check_recipient(category)?;

    let (expense, credit) = match d.kind {
        AidKind::Cash => (codes::CASH_AID, if d.paid_from_bank { codes::BANK } else { codes::CASH }),
        AidKind::InKind => (codes::IN_KIND_AID, codes::INVENTORY),
        AidKind::Voucher => (codes::VOUCHER_AID, codes::VOUCHERS_OUTSTANDING),
    };
    let voucher_code = (d.kind == AidKind::Voucher).then(|| {
        let mut random = [0u8; 10];
        rand::rngs::OsRng.fill_bytes(&mut random);
        voucher::issue(secret, random)
    });
    let expense = account_id(conn, expense).await?;
    let credit = account_id(conn, credit).await?;
    let line = |account_id, side| LineDraft {
        account_id,
        fund_id: d.fund_id,
        project_id: d.project_id,
        side,
        amount: d.amount,
        currency: d.currency,
        memo: Some(d.description.trim().to_owned()),
    };
    let draft = EntryDraft {
        entry_date: d.distributed_on,
        description: format!("Aid to {name}: {}", d.description.trim()),
        reference: voucher_code.clone(),
        lines: vec![line(expense, Side::Debit), line(credit, Side::Credit)],
    };
    let entry = post_in(conn, base, &draft, actor, "aid", None).await?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO aid_distributions (id, beneficiary_id, kind, fund_id, project_id, amount, currency, description, items,
                                        voucher_code, voucher_status, journal_entry_id, distributed_on, created_by)
         VALUES ($1, $2, $3::aid_kind, $4, $5, $6, $7, $8, $9, $10, $11::voucher_status, $12, $13, $14)",
    )
    .bind(id)
    .bind(d.beneficiary_id)
    .bind(match d.kind {
        AidKind::Cash => "cash",
        AidKind::InKind => "in_kind",
        AidKind::Voucher => "voucher",
    })
    .bind(d.fund_id)
    .bind(d.project_id)
    .bind(d.amount)
    .bind(d.currency.as_str())
    .bind(d.description.trim())
    .bind(&d.items)
    .bind(&voucher_code)
    .bind(voucher_code.as_ref().map(|_| "issued"))
    .bind(entry.id)
    .bind(d.distributed_on)
    .bind(actor)
    .execute(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO case_events (beneficiary_id, event_type, note, actor) VALUES ($1, 'distribution', $2, $3)")
        .bind(d.beneficiary_id)
        .bind(format!("{} {} {} — {}", d.amount, d.currency, d.description.trim(), entry.entry_no))
        .bind(actor)
        .execute(&mut *conn)
        .await?;
    Ok(Distribution { id, voucher_code, entry })
}
