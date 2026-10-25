//! Moyasar (Saudi Arabia): mada, Apple Pay, Visa/Mastercard via hosted invoices.
//!
//! Moyasar's invoice callback is not signed, so the webhook body is used only to learn the
//! invoice id; status and amount are then read back from the API with the secret key.

use std::collections::BTreeMap;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::json;

use crate::{Checkout, CheckoutRequest, PaymentError, PaymentEvent, PaymentGateway, PaymentMethod, PaymentStatus, http, str_field};

const API: &str = "https://api.moyasar.com/v1";

pub struct Moyasar {
    secret_key: String,
    client: reqwest::Client,
}

impl Moyasar {
    pub fn new(secret_key: String) -> Self {
        Self { secret_key, client: http() }
    }
}

/// Amount in the smallest currency unit (halalas for SAR), as Moyasar requires.
pub fn minor_units(amount: Decimal, currency: yusr_domain::money::Currency) -> Result<i64, PaymentError> {
    let scaled = currency.round(amount) * Decimal::from(10i64.pow(currency.minor_units()));
    i64::try_from(scaled.trunc()).map_err(|_| PaymentError::Malformed("amount out of range".into()))
}

pub fn invoice_body(req: &CheckoutRequest) -> Result<serde_json::Value, PaymentError> {
    let amount = minor_units(req.amount, req.currency)?;
    if amount < 100 {
        return Err(PaymentError::Rejected("Moyasar's minimum amount is 100 minor units".into()));
    }
    Ok(json!({
        "amount": amount,
        "currency": req.currency.as_str(),
        "description": req.description,
        "callback_url": req.webhook_url,
        "success_url": req.return_url,
        "back_url": req.return_url,
        "metadata": { "intent_id": req.intent_id.to_string() },
    }))
}

pub fn event_from_invoice(invoice: serde_json::Value) -> Result<PaymentEvent, PaymentError> {
    let currency: yusr_domain::money::Currency =
        str_field(&invoice, "currency")?.parse().map_err(|_| PaymentError::Malformed("currency".into()))?;
    let minor = invoice["amount"].as_i64().ok_or_else(|| PaymentError::Malformed("amount".into()))?;
    let status = match str_field(&invoice, "status")? {
        "paid" => PaymentStatus::Paid,
        "initiated" | "on_hold" => PaymentStatus::Pending,
        "refunded" => PaymentStatus::Refunded,
        _ => PaymentStatus::Failed,
    };
    Ok(PaymentEvent {
        gateway: "moyasar",
        gateway_ref: str_field(&invoice, "id")?.to_owned(),
        status,
        amount: Decimal::new(minor, currency.minor_units()),
        currency,
        raw: invoice,
    })
}

#[async_trait]
impl PaymentGateway for Moyasar {
    fn id(&self) -> &'static str {
        "moyasar"
    }

    fn supports(&self, method: PaymentMethod) -> bool {
        matches!(method, PaymentMethod::Any | PaymentMethod::Card | PaymentMethod::Mada | PaymentMethod::ApplePay)
    }

    async fn create_checkout(&self, req: &CheckoutRequest) -> Result<Checkout, PaymentError> {
        if !self.supports(req.method) {
            return Err(PaymentError::Unsupported { gateway: "moyasar", method: req.method });
        }
        let res: serde_json::Value = self
            .client
            .post(format!("{API}/invoices"))
            .basic_auth(&self.secret_key, Some(""))
            .json(&invoice_body(req)?)
            .send()
            .await?
            .json()
            .await?;
        let id = res["id"].as_str().ok_or_else(|| PaymentError::Rejected(res["message"].to_string()))?;
        Ok(Checkout { gateway_ref: id.to_owned(), redirect_url: res["url"].as_str().map(str::to_owned), instructions: None })
    }

    async fn parse_webhook(&self, _headers: &BTreeMap<String, String>, body: &[u8]) -> Result<PaymentEvent, PaymentError> {
        let posted: serde_json::Value = serde_json::from_slice(body).map_err(|e| PaymentError::Malformed(e.to_string()))?;
        let id = str_field(&posted, "id")?;
        // Ids are UUIDs; refusing anything else keeps the path below injection-free.
        uuid::Uuid::parse_str(id).map_err(|_| PaymentError::Malformed("invoice id".into()))?;
        let res = self.client.get(format!("{API}/invoices/{id}")).basic_auth(&self.secret_key, Some("")).send().await?;
        if !res.status().is_success() {
            return Err(PaymentError::BadSignature);
        }
        event_from_invoice(res.json().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use yusr_domain::money::Currency;

    #[test]
    fn amounts_in_minor_units() {
        assert_eq!(minor_units(dec!(10.5), Currency::SAR).unwrap(), 1050);
        assert_eq!(minor_units(dec!(1.234), Currency::KWD).unwrap(), 1234);
    }

    #[test]
    fn invoice_event_mapping() {
        let e = event_from_invoice(json!({"id": "9f2c…", "amount": 25000, "currency": "SAR", "status": "paid"})).unwrap();
        assert_eq!(e.status, PaymentStatus::Paid);
        assert_eq!(e.amount, dec!(250.00));
        let e = event_from_invoice(json!({"id": "x", "amount": 100, "currency": "SAR", "status": "expired"})).unwrap();
        assert_eq!(e.status, PaymentStatus::Failed);
    }

    #[test]
    fn rejects_amounts_below_minimum() {
        let req = CheckoutRequest {
            intent_id: uuid::Uuid::nil(),
            amount: dec!(0.99),
            currency: Currency::SAR,
            method: PaymentMethod::Mada,
            description: "d".into(),
            donor_name: "n".into(),
            donor_email: None,
            donor_phone: None,
            return_url: "https://e/r".into(),
            webhook_url: "https://e/w".into(),
        };
        assert!(invoice_body(&req).is_err());
        let ok = invoice_body(&CheckoutRequest { amount: dec!(1), ..req }).unwrap();
        assert_eq!(ok["amount"], 100);
    }
}
