//! Online donation intents: created before redirecting to a gateway, completed by the
//! gateway's authenticated webhook. Completion is idempotent — gateways retry webhooks.

use chrono::Utc;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::charity::DonationKind;
use yusr_domain::money::Currency;

use crate::donations::{NewDonation, record_in};
use crate::{InfraError, Result, Store, parse_label, serializable};

#[derive(Debug, Clone, Serialize)]
pub struct Intent {
    pub id: Uuid,
    pub donor_id: Uuid,
    pub kind: DonationKind,
    pub fund_id: Uuid,
    pub amount: Decimal,
    pub currency: Currency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The payment was recorded as a donation now.
    Recorded,
    /// Already handled by an earlier delivery of the same webhook.
    AlreadyRecorded,
    Failed,
    Pending,
}

impl Store {
    /// Reserves an intent; `gateway_ref` is a placeholder until [`Store::attach_checkout`].
    pub async fn create_intent(
        &self,
        gateway: &str,
        donor_id: Uuid,
        kind: DonationKind,
        fund_id: Option<Uuid>,
        amount: Decimal,
        currency: Currency,
    ) -> Result<Intent> {
        if amount <= Decimal::ZERO || amount != currency.round(amount) {
            return Err(DomainError::NonPositiveAmount { line: 0, amount }.into());
        }
        serializable!(self, |conn| {
            let fund_id = match fund_id {
                Some(f) => f,
                None => {
                    let code = match kind {
                        DonationKind::Zakat => "ZAKAT",
                        DonationKind::Kaffarah => "KAFF",
                        DonationKind::Waqf => "WAQF",
                        DonationKind::Sadaqah | DonationKind::General => "GEN",
                    };
                    let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM funds WHERE code = $1").bind(code).fetch_one(&mut *conn).await?;
                    id
                }
            };
            let f = crate::ledger::fund(conn, fund_id).await?;
            kind.check_fund(false, f.restriction)?;
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO payment_intents (id, gateway, gateway_ref, donor_id, kind, fund_id, amount, currency)
                 VALUES ($1, $2, $3, $4, $5::donation_kind, $6, $7, $8)",
            )
            .bind(id)
            .bind(gateway)
            .bind(format!("pending:{id}"))
            .bind(donor_id)
            .bind(kind.as_str())
            .bind(fund_id)
            .bind(amount)
            .bind(currency.as_str())
            .execute(&mut *conn)
            .await?;
            Ok(Intent { id, donor_id, kind, fund_id, amount, currency })
        })
    }

    pub async fn attach_checkout(&self, intent_id: Uuid, gateway_ref: &str, checkout_url: Option<&str>) -> Result<()> {
        let n = sqlx::query("UPDATE payment_intents SET gateway_ref = $2, checkout_url = $3 WHERE id = $1 AND status = 'pending'")
            .bind(intent_id)
            .bind(gateway_ref)
            .bind(checkout_url)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 1 { Ok(()) } else { Err(InfraError::NotFound("pending payment intent")) }
    }

    /// Applies an authenticated gateway event. A paid event records the donation (posted to
    /// gateway clearing) exactly once; amount or currency mismatches are refused.
    pub async fn complete_intent(
        &self,
        gateway: &str,
        gateway_ref: &str,
        paid: Option<bool>,
        amount: Decimal,
        currency: Currency,
        raw: &serde_json::Value,
    ) -> Result<Outcome> {
        let base = self.base_currency;
        serializable!(self, |conn| {
            type Row = (Uuid, Uuid, String, Uuid, Decimal, String, String);
            let row: Option<Row> = sqlx::query_as(
                "SELECT id, donor_id, kind::text, fund_id, amount, currency::text, status::text FROM payment_intents
                  WHERE gateway = $1 AND gateway_ref = $2 FOR UPDATE",
            )
            .bind(gateway)
            .bind(gateway_ref)
            .fetch_optional(&mut *conn)
            .await?;
            let (id, donor_id, kind, fund_id, expected, expected_currency, status) = row.ok_or(InfraError::NotFound("payment intent"))?;
            if status == "paid" {
                return Ok(Outcome::AlreadyRecorded);
            }
            match paid {
                None => return Ok(Outcome::Pending),
                Some(false) => {
                    sqlx::query("UPDATE payment_intents SET status = 'failed', raw_event = $2, completed_at = now() WHERE id = $1")
                        .bind(id)
                        .bind(raw)
                        .execute(&mut *conn)
                        .await?;
                    return Ok(Outcome::Failed);
                }
                Some(true) => {}
            }
            if amount != expected || currency.as_str() != expected_currency {
                return Err(InfraError::Conflict("payment_mismatch", format!("paid {amount} {currency}, expected {expected} {expected_currency}")));
            }
            let donation = NewDonation {
                donor_id,
                donation_date: Utc::now().date_naive(),
                kind: Some(parse_label(&kind)?),
                donor_designated: false,
                fund_id: Some(fund_id),
                project_id: None,
                amount,
                currency,
                payment_method: gateway.to_owned(),
                payment_ref: Some(gateway_ref.to_owned()),
                note: None,
            };
            let recorded = record_in(conn, base, &donation, &format!("gateway:{gateway}")).await?;
            sqlx::query("UPDATE payment_intents SET status = 'paid', donation_id = $2, raw_event = $3, completed_at = now() WHERE id = $1")
                .bind(id)
                .bind(recorded.donation_id)
                .bind(raw)
                .execute(&mut *conn)
                .await?;
            Ok(Outcome::Recorded)
        })
    }
}
