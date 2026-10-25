//! Infrastructure layer: PostgreSQL persistence (sqlx) and the application use cases
//! that need it. Business rules come from `yusr-domain`; this crate adds transactions,
//! locking, and the audit chain around them.

pub mod aid;
pub mod archive;
pub mod audit;
pub mod backup;
pub mod budgets;
pub mod donations;
pub mod edits;
pub mod error;
pub mod fx;
pub mod ledger;
pub mod letters;
pub mod payments;
pub mod reports;
pub mod security;
pub mod sponsorships;
pub mod volunteers;
pub mod zatca;

use std::sync::Arc;
use std::time::Duration;

use rand::RngCore;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use yusr_domain::money::Currency;
use yusr_domain::receipt::Organization;

pub use error::InfraError;

pub type Result<T> = std::result::Result<T, InfraError>;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// Serialization failures are retried this many times before giving up.
pub const MAX_RETRIES: u32 = 5;

/// Runs `$body` in a SERIALIZABLE transaction bound to `$conn`, committing on success and
/// retrying on serialization failures and deadlocks with a short exponential backoff.
/// A macro rather than a closure so the body can borrow the connection freely.
#[macro_export]
macro_rules! serializable {
    ($store:expr, |$conn:ident| $body:expr) => {{
        let mut attempt = 0u32;
        loop {
            let mut tx = $store.pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
            let $conn: &mut sqlx::PgConnection = &mut *tx;
            // An async block so `?` and `return` in the body end this attempt, not the caller:
            // errors reach the retry check below and an early `Ok` still commits.
            let result: $crate::Result<_> = async { $body }.await;
            let result = match result {
                Ok(v) => tx.commit().await.map(|_| v).map_err($crate::InfraError::from),
                Err(e) => Err(e), // dropping `tx` rolls back
            };
            match result {
                Err(e) if e.is_retryable() && attempt < $crate::MAX_RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(10u64 << attempt)).await;
                }
                other => break other,
            }
        }
    }};
}

/// Handle to the database plus the organisation settings every use case needs.
#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
    pub base_currency: Currency,
    // Behind a lock because restoring a backup brings that installation's secret with it.
    voucher_secret: Arc<std::sync::RwLock<Vec<u8>>>,
}

impl Store {
    /// Connects, applies pending migrations and creates the settings row on first run.
    pub async fn open(database_url: &str, default_org: &Organization) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(10))
            .connect(database_url)
            .await?;
        MIGRATOR.run(&pool).await.map_err(|e| InfraError::Config(format!("migration failed: {e}")))?;

        let mut secret = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        sqlx::query("INSERT INTO settings (organization, voucher_secret) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
            .bind(serde_json::to_value(default_org).expect("organization serializes"))
            .bind(&secret)
            .execute(&pool)
            .await?;

        let (base, secret): (String, Vec<u8>) =
            sqlx::query_as("SELECT base_currency::text, voucher_secret FROM settings").fetch_one(&pool).await?;
        Ok(Self { pool, base_currency: base.parse()?, voucher_secret: Arc::new(std::sync::RwLock::new(secret)) })
    }

    pub async fn organization(&self) -> Result<Organization> {
        let (v,): (serde_json::Value,) = sqlx::query_as("SELECT organization FROM settings").fetch_one(&self.pool).await?;
        serde_json::from_value(v).map_err(|e| InfraError::Config(format!("settings.organization: {e}")))
    }

    pub async fn update_organization(&self, org: &Organization, actor: &str) -> Result<()> {
        use yusr_domain::DomainError::Invalid;
        use yusr_domain::receipt::{ORG_TYPES, valid_logo};
        if org.logo_data_url.as_deref().is_some_and(|l| !valid_logo(l)) || org.stamp_data_url.as_deref().is_some_and(|l| !valid_logo(l)) {
            return Err(Invalid("logo and stamp must be PNG, JPEG or WebP images of at most 300 KB".into()).into());
        }
        if !(1..=12).contains(&org.fiscal_year_start) {
            return Err(Invalid("fiscal_year_start must be a month from 1 to 12".into()).into());
        }
        if org.org_type.as_deref().is_some_and(|t| !ORG_TYPES.contains(&t)) {
            return Err(Invalid("unknown organisation type".into()).into());
        }
        if org.website.as_deref().is_some_and(|w| !w.is_empty() && !(w.starts_with("https://") || w.starts_with("http://"))) {
            return Err(Invalid("website must start with https://".into()).into());
        }
        let value = serde_json::to_value(org).expect("organization serializes");
        serializable!(self, |conn| {
            sqlx::query("UPDATE settings SET organization = $1, updated_at = now()").bind(&value).execute(&mut *conn).await?;
            audit::append(conn, actor, "update", "settings", "organization", value.clone()).await?;
            Ok(())
        })
    }

    pub(crate) fn voucher_secret(&self) -> Vec<u8> {
        self.voucher_secret.read().expect("voucher secret lock").clone()
    }
}

/// Parses a PostgreSQL enum label (selected as text) into its domain enum, whose serde
/// names match the labels.
pub(crate) fn parse_label<T: serde::de::DeserializeOwned>(label: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(label.to_owned()))
        .map_err(|_| InfraError::Config(format!("unknown enum label `{label}`")))
}
