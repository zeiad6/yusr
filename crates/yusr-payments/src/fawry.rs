//! FawryPay (Egypt): "Pay at Fawry" reference codes, payable in cash at retail outlets
//! or from mobile wallets.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    Checkout, CheckoutRequest, PaymentError, PaymentEvent, PaymentGateway, PaymentMethod, PaymentStatus, decimal_field, fixed, http,
    str_field,
};

pub const PRODUCTION: &str = "https://www.atfawry.com";
pub const STAGING: &str = "https://atfawry.fawrystaging.com";

pub struct Fawry {
    merchant_code: String,
    secure_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl Fawry {
    pub fn new(merchant_code: String, secure_key: String, base_url: String) -> Self {
        Self { merchant_code, secure_key, base_url, client: http() }
    }
}

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Charge signature: SHA-256 of merchantCode + merchantRefNum + customerProfileId +
/// paymentMethod + amount (2 dp) + secureKey.
pub fn charge_body(merchant_code: &str, secure_key: &str, req: &CheckoutRequest) -> Result<serde_json::Value, PaymentError> {
    if req.method != PaymentMethod::FawryReference {
        return Err(PaymentError::Unsupported { gateway: "fawry", method: req.method });
    }
    if req.currency != yusr_domain::money::Currency::EGP {
        return Err(PaymentError::Rejected("Fawry charges are in EGP".into()));
    }
    let reference = req.intent_id.simple().to_string();
    let amount = fixed(req.amount, req.currency);
    let signature = sha256_hex(&format!("{merchant_code}{reference}{}PAYATFAWRY{amount}{secure_key}", ""));
    Ok(json!({
        "merchantCode": merchant_code,
        "merchantRefNum": reference,
        "customerProfileId": "",
        "customerName": req.donor_name,
        "customerMobile": req.donor_phone,
        "customerEmail": req.donor_email,
        "paymentMethod": "PAYATFAWRY",
        "amount": amount.parse::<f64>().unwrap_or_default(),
        "currencyCode": "EGP",
        "description": req.description,
        "chargeItems": [{ "itemId": "donation", "description": req.description, "price": amount, "quantity": 1 }],
        "orderWebHookUrl": req.webhook_url,
        "signature": signature,
    }))
}

/// Server notification (V2) signature: SHA-256 of fawryRefNumber + merchantRefNumber +
/// paymentAmount (2 dp) + orderAmount (2 dp) + orderStatus + paymentMethod +
/// paymentRefrenceNumber (sic) + secureKey.
pub fn notification_signature(secure_key: &str, n: &serde_json::Value) -> Result<String, PaymentError> {
    let two = |name: &str| -> Result<String, PaymentError> { Ok(format!("{:.2}", decimal_field(n, name)?)) };
    let text = |name: &str| match &n[name] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(x) => x.to_string(),
        _ => String::new(),
    };
    Ok(sha256_hex(&format!(
        "{}{}{}{}{}{}{}{secure_key}",
        text("fawryRefNumber"),
        text("merchantRefNumber"),
        two("paymentAmount")?,
        two("orderAmount")?,
        text("orderStatus"),
        text("paymentMethod"),
        text("paymentRefrenceNumber"),
    )))
}

/// Compares without an early exit, so response timing reveals nothing about the expected signature.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn verify_notification(secure_key: &str, body: &[u8]) -> Result<PaymentEvent, PaymentError> {
    let n: serde_json::Value = serde_json::from_slice(body).map_err(|e| PaymentError::Malformed(e.to_string()))?;
    let given = str_field(&n, "messageSignature")?;
    if !constant_time_eq(given.to_ascii_lowercase().as_bytes(), notification_signature(secure_key, &n)?.as_bytes()) {
        return Err(PaymentError::BadSignature);
    }
    let status = match str_field(&n, "orderStatus")? {
        "PAID" => PaymentStatus::Paid,
        "NEW" | "UNPAID" => PaymentStatus::Pending,
        "REFUNDED" | "PARTIALLY_REFUNDED" => PaymentStatus::Refunded,
        _ => PaymentStatus::Failed,
    };
    Ok(PaymentEvent {
        gateway: "fawry",
        // We look intents up by the reference Fawry returned at charge time.
        gateway_ref: match &n["fawryRefNumber"] {
            serde_json::Value::Number(x) => x.to_string(),
            v => v.as_str().unwrap_or_default().to_owned(),
        },
        status,
        amount: decimal_field(&n, "orderAmount")?,
        currency: yusr_domain::money::Currency::EGP,
        raw: n,
    })
}

#[async_trait]
impl PaymentGateway for Fawry {
    fn id(&self) -> &'static str {
        "fawry"
    }

    fn supports(&self, method: PaymentMethod) -> bool {
        method == PaymentMethod::FawryReference
    }

    async fn create_checkout(&self, req: &CheckoutRequest) -> Result<Checkout, PaymentError> {
        let body = charge_body(&self.merchant_code, &self.secure_key, req)?;
        let res: serde_json::Value = self
            .client
            .post(format!("{}/ECommerceWeb/Fawry/payments/charge", self.base_url))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if res["statusCode"].as_i64() != Some(200) {
            return Err(PaymentError::Rejected(res["statusDescription"].to_string()));
        }
        let reference = match &res["referenceNumber"] {
            serde_json::Value::Number(x) => x.to_string(),
            v => v.as_str().ok_or_else(|| PaymentError::Malformed("referenceNumber".into()))?.to_owned(),
        };
        Ok(Checkout {
            instructions: Some(format!("ادفع بالرقم المرجعي {reference} في أي منفذ فوري — Pay with Fawry reference {reference} at any Fawry outlet")),
            gateway_ref: reference,
            redirect_url: None,
        })
    }

    async fn parse_webhook(&self, _headers: &BTreeMap<String, String>, body: &[u8]) -> Result<PaymentEvent, PaymentError> {
        verify_notification(&self.secure_key, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use yusr_domain::money::Currency;

    #[test]
    fn charge_signature_order() {
        let req = CheckoutRequest {
            intent_id: uuid::Uuid::nil(),
            amount: dec!(250),
            currency: Currency::EGP,
            method: PaymentMethod::FawryReference,
            description: "Sadaqah".into(),
            donor_name: "Mona".into(),
            donor_email: None,
            donor_phone: Some("01000000000".into()),
            return_url: "https://e/r".into(),
            webhook_url: "https://e/w".into(),
        };
        let b = charge_body("MC", "SK", &req).unwrap();
        assert_eq!(b["signature"], sha256_hex("MC00000000000000000000000000000000PAYATFAWRY250.00SK"));
        assert!(charge_body("MC", "SK", &CheckoutRequest { currency: Currency::SAR, ..req }).is_err());
    }

    #[test]
    fn notification_is_verified() {
        let mut n = json!({
            "fawryRefNumber": "931337410", "merchantRefNumber": "abc", "paymentAmount": 250, "orderAmount": "250.00",
            "orderStatus": "PAID", "paymentMethod": "PAYATFAWRY", "paymentRefrenceNumber": ""
        });
        n["messageSignature"] = json!(sha256_hex("931337410abc250.00250.00PAIDPAYATFAWRYSK"));
        let e = verify_notification("SK", &serde_json::to_vec(&n).unwrap()).unwrap();
        assert_eq!(e.status, PaymentStatus::Paid);
        assert_eq!(e.gateway_ref, "931337410");
        n["orderAmount"] = json!("2500.00");
        assert!(matches!(verify_notification("SK", &serde_json::to_vec(&n).unwrap()), Err(PaymentError::BadSignature)));
    }
}
