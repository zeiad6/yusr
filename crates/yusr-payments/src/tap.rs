//! Tap Payments (tap.company): KNET, Benefit, mada, Apple Pay and cards on one hosted page.

use std::collections::BTreeMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;

use crate::{
    Checkout, CheckoutRequest, PaymentError, PaymentEvent, PaymentGateway, PaymentMethod, PaymentStatus, decimal_field, fixed, http,
    str_field,
};

const API: &str = "https://api.tap.company/v2";

pub struct Tap {
    secret_key: String,
    client: reqwest::Client,
}

impl Tap {
    pub fn new(secret_key: String) -> Self {
        Self { secret_key, client: http() }
    }
}

fn source_id(method: PaymentMethod) -> Option<&'static str> {
    match method {
        PaymentMethod::Any | PaymentMethod::Mada | PaymentMethod::ApplePay => Some("src_all"),
        PaymentMethod::Card => Some("src_card"),
        PaymentMethod::Knet => Some("src_kw.knet"),
        PaymentMethod::Benefit => Some("src_bh.benefit"),
        PaymentMethod::FawryReference => None,
    }
}

pub fn charge_body(req: &CheckoutRequest) -> Result<serde_json::Value, PaymentError> {
    let source = source_id(req.method).ok_or(PaymentError::Unsupported { gateway: "tap", method: req.method })?;
    let (first, last) = req.donor_name.split_once(' ').unwrap_or((&req.donor_name, ""));
    Ok(json!({
        "amount": fixed(req.amount, req.currency).parse::<f64>().unwrap_or_default(),
        "currency": req.currency.as_str(),
        "description": req.description,
        "reference": { "transaction": req.intent_id.to_string(), "order": req.intent_id.to_string() },
        "customer": {
            "first_name": first,
            "last_name": last,
            "email": req.donor_email,
            "phone": req.donor_phone.as_ref().map(|p| json!({ "number": p })),
        },
        "source": { "id": source },
        "redirect": { "url": req.return_url },
        "post": { "url": req.webhook_url },
        "metadata": { "intent_id": req.intent_id.to_string() },
    }))
}

/// Tap's `hashstring`: HMAC-SHA256 (secret key) over
/// `x_id…x_amount…x_currency…x_gateway_reference…x_payment_reference…x_status…x_created…`,
/// with the amount formatted to the currency's decimals.
pub fn webhook_hash(secret_key: &str, charge: &serde_json::Value) -> Result<String, PaymentError> {
    let currency: yusr_domain::money::Currency =
        str_field(charge, "currency")?.parse().map_err(|_| PaymentError::Malformed("currency".into()))?;
    let amount = fixed(decimal_field(charge, "amount")?, currency);
    let reference = &charge["reference"];
    let created = match &charge["transaction"]["created"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    let message = format!(
        "x_id{}x_amount{}x_currency{}x_gateway_reference{}x_payment_reference{}x_status{}x_created{}",
        str_field(charge, "id")?,
        amount,
        currency,
        reference["gateway"].as_str().unwrap_or_default(),
        reference["payment"].as_str().unwrap_or_default(),
        str_field(charge, "status")?,
        created,
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes()).expect("any key length");
    mac.update(message.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn status(s: &str) -> PaymentStatus {
    match s {
        "CAPTURED" | "AUTHORIZED" => PaymentStatus::Paid,
        "INITIATED" | "IN_PROGRESS" => PaymentStatus::Pending,
        "REFUNDED" | "REVERSED" => PaymentStatus::Refunded,
        _ => PaymentStatus::Failed,
    }
}

pub fn verify_webhook(secret_key: &str, headers: &BTreeMap<String, String>, body: &[u8]) -> Result<PaymentEvent, PaymentError> {
    let charge: serde_json::Value = serde_json::from_slice(body).map_err(|e| PaymentError::Malformed(e.to_string()))?;
    let given = headers.get("hashstring").ok_or(PaymentError::BadSignature)?;
    let expected = webhook_hash(secret_key, &charge)?;
    // Constant-time comparison.
    let mut mac = Hmac::<Sha256>::new_from_slice(b"yusr-compare").expect("any key length");
    mac.update(expected.as_bytes());
    let tag = mac.finalize().into_bytes();
    let mut mac = Hmac::<Sha256>::new_from_slice(b"yusr-compare").expect("any key length");
    mac.update(given.trim().to_ascii_lowercase().as_bytes());
    mac.verify_slice(&tag).map_err(|_| PaymentError::BadSignature)?;

    Ok(PaymentEvent {
        gateway: "tap",
        gateway_ref: str_field(&charge, "id")?.to_owned(),
        status: status(str_field(&charge, "status")?),
        amount: decimal_field(&charge, "amount")?,
        currency: str_field(&charge, "currency")?.parse().map_err(|_| PaymentError::Malformed("currency".into()))?,
        raw: charge,
    })
}

#[async_trait]
impl PaymentGateway for Tap {
    fn id(&self) -> &'static str {
        "tap"
    }

    fn supports(&self, method: PaymentMethod) -> bool {
        source_id(method).is_some()
    }

    async fn create_checkout(&self, req: &CheckoutRequest) -> Result<Checkout, PaymentError> {
        let res: serde_json::Value = self
            .client
            .post(format!("{API}/charges"))
            .bearer_auth(&self.secret_key)
            .json(&charge_body(req)?)
            .send()
            .await?
            .json()
            .await?;
        let id = res["id"].as_str().ok_or_else(|| PaymentError::Rejected(res["errors"].to_string()))?;
        Ok(Checkout {
            gateway_ref: id.to_owned(),
            redirect_url: res["transaction"]["url"].as_str().map(str::to_owned),
            instructions: None,
        })
    }

    async fn parse_webhook(&self, headers: &BTreeMap<String, String>, body: &[u8]) -> Result<PaymentEvent, PaymentError> {
        verify_webhook(&self.secret_key, headers, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use uuid::Uuid;
    use yusr_domain::money::Currency;

    fn charge() -> serde_json::Value {
        json!({
            "id": "chg_TS01A1234567890",
            "amount": 1,
            "currency": "KWD",
            "status": "CAPTURED",
            "reference": { "gateway": "123456", "payment": "pay_1" },
            "transaction": { "created": "1700000000000" }
        })
    }

    #[test]
    fn hash_formats_amount_with_currency_decimals() {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"sk_test").unwrap();
        mac.update(b"x_idchg_TS01A1234567890x_amount1.000x_currencyKWDx_gateway_reference123456x_payment_referencepay_1x_statusCAPTUREDx_created1700000000000");
        assert_eq!(webhook_hash("sk_test", &charge()).unwrap(), hex::encode(mac.finalize().into_bytes()));
    }

    #[test]
    fn verifies_genuine_and_rejects_forged_webhooks() {
        let body = serde_json::to_vec(&charge()).unwrap();
        let mut h = BTreeMap::new();
        h.insert("hashstring".to_owned(), webhook_hash("sk_test", &charge()).unwrap());
        let e = verify_webhook("sk_test", &h, &body).unwrap();
        assert_eq!(e.status, PaymentStatus::Paid);
        assert_eq!(e.amount, dec!(1));
        assert_eq!(e.currency, Currency::KWD);

        let mut forged = charge();
        forged["amount"] = json!(100);
        let forged = serde_json::to_vec(&forged).unwrap();
        assert!(matches!(verify_webhook("sk_test", &h, &forged), Err(PaymentError::BadSignature)));
        assert!(matches!(verify_webhook("sk_test", &BTreeMap::new(), &body), Err(PaymentError::BadSignature)));
    }

    #[test]
    fn knet_charge_body() {
        let req = CheckoutRequest {
            intent_id: Uuid::nil(),
            amount: dec!(10),
            currency: Currency::KWD,
            method: PaymentMethod::Knet,
            description: "Zakat".into(),
            donor_name: "Ahmad Ali".into(),
            donor_email: None,
            donor_phone: None,
            return_url: "https://example.org/thanks".into(),
            webhook_url: "https://example.org/api/payments/webhook/tap".into(),
        };
        let b = charge_body(&req).unwrap();
        assert_eq!(b["source"]["id"], "src_kw.knet");
        assert_eq!(b["amount"], 10.0);
        assert_eq!(b["customer"]["last_name"], "Ali");
        let fawry = CheckoutRequest { method: PaymentMethod::FawryReference, ..req };
        assert!(charge_body(&fawry).is_err());
    }
}
