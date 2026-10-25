//! Historical exchange rates. A transaction always converts at the latest rate on or
//! before its own date, so back-dated entries use the rate that applied then.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use yusr_domain::DomainError;
use yusr_domain::money::Currency;

use crate::{Result, Store, audit, serializable};

pub async fn rate_on(conn: &mut PgConnection, currency: Currency, date: NaiveDate) -> Result<Decimal> {
    let row: Option<(Decimal,)> = sqlx::query_as(
        "SELECT rate FROM exchange_rates WHERE currency = $1 AND rate_date <= $2 ORDER BY rate_date DESC LIMIT 1",
    )
    .bind(currency.as_str())
    .bind(date)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|(r,)| r)
        .ok_or_else(|| DomainError::Invalid(format!("no {currency} exchange rate on or before {date}")).into())
}

impl Store {
    pub async fn set_rate(&self, currency: Currency, date: NaiveDate, rate: Decimal, source: &str, actor: &str) -> Result<()> {
        if currency == self.base_currency {
            return Err(DomainError::Invalid("the base currency has no exchange rate".into()).into());
        }
        if rate <= Decimal::ZERO {
            return Err(DomainError::InvalidRate { line: 0, rate }.into());
        }
        serializable!(self, |conn| {
            sqlx::query(
                "INSERT INTO exchange_rates (currency, rate_date, rate, source) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (currency, rate_date) DO UPDATE SET rate = EXCLUDED.rate, source = EXCLUDED.source",
            )
            .bind(currency.as_str())
            .bind(date)
            .bind(rate)
            .bind(source)
            .execute(&mut *conn)
            .await?;
            audit::append(
                conn,
                actor,
                "set_rate",
                "exchange_rate",
                &format!("{currency}:{date}"),
                serde_json::json!({ "rate": rate.to_string(), "source": source }),
            )
            .await?;
            Ok(())
        })
    }

    pub async fn rate_on(&self, currency: Currency, date: NaiveDate) -> Result<Decimal> {
        if currency == self.base_currency {
            return Ok(Decimal::ONE);
        }
        let mut conn = self.pool.acquire().await?;
        rate_on(&mut conn, currency, date).await
    }
}
