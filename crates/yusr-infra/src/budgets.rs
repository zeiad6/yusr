//! Budget definitions. Enforcement happens inside the posting engine.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yusr_domain::DomainError;

use crate::{Result, Store, audit, serializable};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewBudget {
    pub fund_id: Uuid,
    pub project_id: Option<Uuid>,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub approved: Decimal,
    #[serde(default)]
    pub committed: Decimal,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BudgetUsage {
    pub id: Uuid,
    pub fund_code: String,
    pub project_code: Option<String>,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub approved: Decimal,
    pub committed: Decimal,
    pub spent: Decimal,
}

impl Store {
    pub async fn create_budget(&self, b: &NewBudget, actor: &str) -> Result<Uuid> {
        if b.period_end < b.period_start || b.approved < Decimal::ZERO || b.committed < Decimal::ZERO {
            return Err(DomainError::Invalid("budget needs a valid period and non-negative amounts".into()).into());
        }
        serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO budgets (fund_id, project_id, period_start, period_end, approved, committed)
                 VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            )
            .bind(b.fund_id)
            .bind(b.project_id)
            .bind(b.period_start)
            .bind(b.period_end)
            .bind(b.approved)
            .bind(b.committed)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "budget", &id.to_string(), serde_json::to_value(b).expect("serializes")).await?;
            Ok(id)
        })
    }

    pub async fn budget_usage(&self, on: NaiveDate) -> Result<Vec<BudgetUsage>> {
        Ok(sqlx::query_as(
            "SELECT b.id, f.code AS fund_code, p.code AS project_code, b.period_start, b.period_end, b.approved, b.committed,
                    COALESCE((SELECT SUM(l.signed_base) FROM journal_lines l
                                JOIN journal_entries e ON e.id = l.entry_id
                                JOIN accounts a ON a.id = l.account_id
                               WHERE a.account_type = 'expense' AND l.fund_id = b.fund_id
                                 AND (b.project_id IS NULL OR l.project_id = b.project_id)
                                 AND e.entry_date BETWEEN b.period_start AND b.period_end), 0) AS spent
               FROM budgets b JOIN funds f ON f.id = b.fund_id LEFT JOIN projects p ON p.id = b.project_id
              WHERE $1 BETWEEN b.period_start AND b.period_end
              ORDER BY f.code, p.code NULLS FIRST",
        )
        .bind(on)
        .fetch_all(&self.pool)
        .await?)
    }
}
