//! Pluggable payment gateways.
//!
//! Every adapter implements [`PaymentGateway`]: it creates a hosted checkout (so card data
//! never touches our servers and PCI scope stays minimal) and turns a webhook into an
//! authenticated [`PaymentEvent`]. A webhook is never trusted on its body alone: it is
//! either HMAC/SHA-verified with the merchant secret or re-fetched from the gateway API.
//!
//! Adapters are compiled in with cargo features and switched on at runtime by
//! environment variables (see [`Registry::from_env`]).

#[cfg(feature = "fawry")]
pub mod fawry;
#[cfg(feature = "moyasar")]
pub mod moyasar;
#[cfg(feature = "tap")]
pub mod tap;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use yusr_domain::money::Currency;

#[derive(Debug, Error)]
pub enum PaymentError {
    #[error("gateway `{0}` is not configured")]
    UnknownGateway(String),
    #[error("{gateway} does not support {method:?}")]
    Unsupported { gateway: &'static str, method: PaymentMethod },
    #[error("webhook authentication failed")]
    BadSignature,
    #[error("malformed gateway payload: {0}")]
    Malformed(String),
    #[error("gateway rejected the request: {0}")]
    Rejected(String),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    /// Every method the merchant enabled, chosen on the hosted page.
    Any,
    Card,
    Mada,
    ApplePay,
    Knet,
    Benefit,
    FawryReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutRequest {
    /// Our payment-intent id; echoed back by the gateway as the merchant reference.
    pub intent_id: Uuid,
    pub amount: Decimal,
    pub currency: Currency,
    pub method: PaymentMethod,
    pub description: String,
    pub donor_name: String,
    pub donor_email: Option<String>,
    pub donor_phone: Option<String>,
    /// Where the donor returns after paying.
    pub return_url: String,
    /// Our webhook endpoint for this gateway.
    pub webhook_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkout {
    pub gateway_ref: String,
    /// Hosted payment page, when the method has one.
    pub redirect_url: Option<String>,
    /// Instructions instead of a page, e.g. a Fawry reference number to pay at an outlet.
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,
    Paid,
    Failed,
    Refunded,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaymentEvent {
    pub gateway: &'static str,
    pub gateway_ref: String,
    pub status: PaymentStatus,
    pub amount: Decimal,
    pub currency: Currency,
    pub raw: serde_json::Value,
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, method: PaymentMethod) -> bool;
    async fn create_checkout(&self, req: &CheckoutRequest) -> Result<Checkout, PaymentError>;
    /// Authenticates and parses a webhook. `headers` names are lower-case.
    async fn parse_webhook(&self, headers: &BTreeMap<String, String>, body: &[u8]) -> Result<PaymentEvent, PaymentError>;
}

/// The gateways enabled for this deployment.
#[derive(Clone, Default)]
pub struct Registry {
    gateways: BTreeMap<&'static str, Arc<dyn PaymentGateway>>,
}

impl Registry {
    pub fn register(&mut self, g: Arc<dyn PaymentGateway>) {
        self.gateways.insert(g.id(), g);
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn PaymentGateway>, PaymentError> {
        self.gateways.get(id).cloned().ok_or_else(|| PaymentError::UnknownGateway(id.to_owned()))
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.gateways.keys().copied().collect()
    }

    /// Enables each compiled-in gateway whose credentials are present:
    /// `YUSR_TAP_SECRET_KEY`; `YUSR_MOYASAR_SECRET_KEY`;
    /// `YUSR_FAWRY_MERCHANT_CODE` + `YUSR_FAWRY_SECURE_KEY` (+ `YUSR_FAWRY_BASE_URL` for staging).
    pub fn from_env() -> Self {
        // reqwest is built without a bundled crypto provider; use ring process-wide.
        let _ = rustls::crypto::ring::default_provider().install_default();
        #[allow(unused_mut)]
        let mut r = Registry::default();
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        #[cfg(feature = "tap")]
        if let Some(key) = var("YUSR_TAP_SECRET_KEY") {
            r.register(Arc::new(tap::Tap::new(key)));
        }
        #[cfg(feature = "moyasar")]
        if let Some(key) = var("YUSR_MOYASAR_SECRET_KEY") {
            r.register(Arc::new(moyasar::Moyasar::new(key)));
        }
        #[cfg(feature = "fawry")]
        if let (Some(code), Some(key)) = (var("YUSR_FAWRY_MERCHANT_CODE"), var("YUSR_FAWRY_SECURE_KEY")) {
            let base = var("YUSR_FAWRY_BASE_URL").unwrap_or_else(|| fawry::PRODUCTION.to_owned());
            r.register(Arc::new(fawry::Fawry::new(code, key, base)));
        }
        let _ = var;
        r
    }
}

/// Formats an amount with the currency's minor-unit decimals ("10.00", "1.000").
pub fn fixed(amount: Decimal, currency: Currency) -> String {
    format!("{:.*}", currency.minor_units() as usize, currency.round(amount))
}

pub(crate) fn decimal_field(v: &serde_json::Value, name: &str) -> Result<Decimal, PaymentError> {
    let s = match &v[name] {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => return Err(PaymentError::Malformed(format!("missing `{name}`"))),
    };
    s.parse().map_err(|_| PaymentError::Malformed(format!("`{name}` is not a number")))
}

pub(crate) fn str_field<'a>(v: &'a serde_json::Value, name: &str) -> Result<&'a str, PaymentError> {
    v[name].as_str().ok_or_else(|| PaymentError::Malformed(format!("missing `{name}`")))
}

pub(crate) fn http() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("TLS backend initialises")
}
