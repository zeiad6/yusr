//! Read models for the executive dashboard and statutory statements.

use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::budgets::BudgetUsage;
use crate::ledger::{FundBalance, codes};
use crate::{Result, Store, audit, serializable};

#[derive(Debug, Clone, Serialize)]
pub struct KindTotal {
    pub kind: String,
    pub total: Decimal,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthFlow {
    /// `YYYY-MM`
    pub month: String,
    pub inflow: Decimal,
    pub outflow: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionalExpense {
    pub class: String,
    pub total: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    pub as_of: NaiveDate,
    pub hijri_as_of: String,
    pub base_currency: String,
    pub cash_position: Decimal,
    pub donations_this_month: Vec<KindTotal>,
    pub aid_this_month: Decimal,
    pub active_cases: i64,
    pub critical_cases: i64,
    pub funds: Vec<FundBalance>,
    pub cashflow: Vec<MonthFlow>,
    pub functional_expenses_ytd: Vec<FunctionalExpense>,
    pub budgets: Vec<BudgetUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProject {
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub fund_id: Uuid,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub fund_id: Uuid,
}

impl Store {
    pub async fn create_project(&self, p: &NewProject, actor: &str) -> Result<Uuid> {
        serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as("INSERT INTO projects (code, name_ar, name_en, fund_id) VALUES ($1, $2, $3, $4) RETURNING id")
                .bind(&p.code)
                .bind(&p.name_ar)
                .bind(&p.name_en)
                .bind(p.fund_id)
                .fetch_one(&mut *conn)
                .await?;
            audit::append(conn, actor, "create", "project", &id.to_string(), serde_json::to_value(p).expect("serializes")).await?;
            Ok(id)
        })
    }

    pub async fn projects(&self) -> Result<Vec<Project>> {
        Ok(sqlx::query_as("SELECT id, code, name_ar, name_en, fund_id FROM projects WHERE is_active ORDER BY code")
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn dashboard(&self, as_of: NaiveDate) -> Result<Dashboard> {
        let month_start = as_of.with_day(1).expect("day 1 exists");
        let year_start = NaiveDate::from_ymd_opt(as_of.year(), 1, 1).expect("valid");
        let cash_codes = [codes::CASH, codes::BANK, codes::GATEWAY_CLEARING];

        let (cash_position,): (Decimal,) = sqlx::query_as(
            "SELECT COALESCE(SUM(l.signed_base), 0) FROM journal_lines l
               JOIN accounts a ON a.id = l.account_id JOIN journal_entries e ON e.id = l.entry_id
              WHERE a.code = ANY($1) AND e.entry_date <= $2",
        )
        .bind(&cash_codes[..])
        .bind(as_of)
        .fetch_one(&self.pool)
        .await?;

        let rows: Vec<(String, Decimal, i64)> = sqlx::query_as(
            "SELECT kind::text, SUM(base_amount), COUNT(*) FROM donations
              WHERE donation_date BETWEEN $1 AND $2 GROUP BY kind ORDER BY 2 DESC",
        )
        .bind(month_start)
        .bind(as_of)
        .fetch_all(&self.pool)
        .await?;
        let donations_this_month = rows.into_iter().map(|(kind, total, count)| KindTotal { kind, total, count }).collect();

        let (aid_this_month,): (Decimal,) = sqlx::query_as(
            "SELECT COALESCE(SUM(l.base_amount), 0) FROM aid_distributions d
               JOIN journal_lines l ON l.entry_id = d.journal_entry_id AND l.side = 'debit'
              WHERE d.distributed_on BETWEEN $1 AND $2",
        )
        .bind(month_start)
        .bind(as_of)
        .fetch_one(&self.pool)
        .await?;

        let (active_cases, critical_cases): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COUNT(*) FILTER (WHERE priority = 'critical') FROM beneficiaries WHERE status = 'active'",
        )
        .fetch_one(&self.pool)
        .await?;

        let first_month = month_start.checked_sub_months(Months::new(11)).expect("in range");
        let rows: Vec<(String, Decimal, Decimal)> = sqlx::query_as(
            "SELECT to_char(e.entry_date, 'YYYY-MM'),
                    COALESCE(SUM(l.base_amount) FILTER (WHERE l.side = 'debit'), 0),
                    COALESCE(SUM(l.base_amount) FILTER (WHERE l.side = 'credit'), 0)
               FROM journal_lines l JOIN accounts a ON a.id = l.account_id JOIN journal_entries e ON e.id = l.entry_id
              WHERE a.code = ANY($1) AND e.entry_date BETWEEN $2 AND $3
              GROUP BY 1 ORDER BY 1",
        )
        .bind(&cash_codes[..])
        .bind(first_month)
        .bind(as_of)
        .fetch_all(&self.pool)
        .await?;
        // Emit all twelve months, zero-filled, so charts have a stable x-axis.
        let cashflow = (0..12)
            .map(|i| {
                let m = first_month.checked_add_months(Months::new(i)).expect("in range").format("%Y-%m").to_string();
                let (inflow, outflow) = rows.iter().find(|(k, _, _)| *k == m).map_or((Decimal::ZERO, Decimal::ZERO), |(_, i, o)| (*i, *o));
                MonthFlow { month: m, inflow, outflow }
            })
            .collect();

        let rows: Vec<(String, Decimal)> = sqlx::query_as(
            "SELECT a.functional_class::text, SUM(l.signed_base) FROM journal_lines l
               JOIN accounts a ON a.id = l.account_id JOIN journal_entries e ON e.id = l.entry_id
              WHERE a.functional_class IS NOT NULL AND e.entry_date BETWEEN $1 AND $2
              GROUP BY 1 ORDER BY 1",
        )
        .bind(year_start)
        .bind(as_of)
        .fetch_all(&self.pool)
        .await?;
        let functional_expenses_ytd = rows.into_iter().map(|(class, total)| FunctionalExpense { class, total }).collect();

        Ok(Dashboard {
            as_of,
            hijri_as_of: yusr_domain::hijri::HijriDate::from_gregorian(as_of).map(|h| h.to_string()).unwrap_or_default(),
            base_currency: self.base_currency.to_string(),
            cash_position,
            donations_this_month,
            aid_this_month,
            active_cases,
            critical_cases,
            funds: self.fund_balances(as_of).await?,
            cashflow,
            functional_expenses_ytd,
            budgets: self.budget_usage(as_of).await?,
        })
    }
}

/// One revenue or expense account in the statement of activities, split by fund restriction.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActivityRow {
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub account_type: String,
    pub restriction: String,
    /// In base currency, positive in the account's natural direction (credit for revenue).
    pub amount: Decimal,
}

/// A labelled total; `name_*` are set when the key is a fund or a donor.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GroupTotal {
    pub key: String,
    pub name_ar: Option<String>,
    pub name_en: Option<String>,
    pub total: Decimal,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DonationSummary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub total: Decimal,
    pub count: i64,
    pub donors: i64,
    pub by_kind: Vec<GroupTotal>,
    pub by_fund: Vec<GroupTotal>,
    pub by_method: Vec<GroupTotal>,
    pub by_month: Vec<GroupTotal>,
    pub top_donors: Vec<GroupTotal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AidSummary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub total: Decimal,
    pub count: i64,
    pub beneficiaries: i64,
    pub by_kind: Vec<GroupTotal>,
    pub by_category: Vec<GroupTotal>,
    pub by_fund: Vec<GroupTotal>,
    pub by_month: Vec<GroupTotal>,
}

/// Most generous donors listed in the donation analysis.
const TOP_DONORS: i64 = 10;

/// `SELECT <key columns>, SUM(<amount>) AS total, COUNT(*) AS count FROM <table> <join> WHERE <filter> GROUP BY .. ORDER BY ..`
fn grouped(table: &str, amount: &str, filter: &str, select: &str, join: &str, group: &str, order: &str) -> String {
    format!("SELECT {select}, SUM({amount}) AS total, COUNT(*) AS count FROM {table} {join} WHERE {filter} GROUP BY {group} ORDER BY {order}")
}

impl Store {
    pub async fn activities(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<ActivityRow>> {
        Ok(sqlx::query_as(
            "SELECT a.code, a.name_ar, a.name_en, a.account_type::text AS account_type, f.restriction::text AS restriction,
                    CASE a.account_type WHEN 'revenue' THEN -SUM(l.signed_base) ELSE SUM(l.signed_base) END AS amount
               FROM journal_lines l
               JOIN journal_entries e ON e.id = l.entry_id
               JOIN accounts a ON a.id = l.account_id
               JOIN funds f ON f.id = l.fund_id
              WHERE a.account_type IN ('revenue', 'expense') AND e.entry_date BETWEEN $1 AND $2
              GROUP BY a.code, a.name_ar, a.name_en, a.account_type, f.restriction
             HAVING SUM(l.signed_base) <> 0
              ORDER BY a.code, f.restriction",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn group_totals(&self, sql: &str, from: NaiveDate, to: NaiveDate) -> Result<Vec<GroupTotal>> {
        Ok(sqlx::query_as(sql).bind(from).bind(to).fetch_all(&self.pool).await?)
    }

    pub async fn donation_summary(&self, from: NaiveDate, to: NaiveDate) -> Result<DonationSummary> {
        const T: &str = "donations d";
        const A: &str = "d.base_amount";
        const W: &str = "d.donation_date BETWEEN $1 AND $2";
        const NO_NAME: &str = "NULL::text AS name_ar, NULL::text AS name_en";
        let (total, count, donors): (Decimal, i64, i64) =
            sqlx::query_as(&format!("SELECT COALESCE(SUM({A}), 0), COUNT(*), COUNT(DISTINCT d.donor_id) FROM {T} WHERE {W}"))
                .bind(from)
                .bind(to)
                .fetch_one(&self.pool)
                .await?;
        let funds_join = "JOIN funds f ON f.id = d.fund_id";
        let donors_join = "JOIN donors o ON o.id = d.donor_id";
        Ok(DonationSummary {
            from,
            to,
            total,
            count,
            donors,
            by_kind: self.group_totals(&grouped(T, A, W, &format!("d.kind::text AS key, {NO_NAME}"), "", "d.kind", "total DESC"), from, to).await?,
            by_fund: self.group_totals(&grouped(T, A, W, "f.code AS key, f.name_ar, f.name_en", funds_join, "f.code, f.name_ar, f.name_en", "total DESC"), from, to).await?,
            by_method: self.group_totals(&grouped(T, A, W, &format!("d.payment_method AS key, {NO_NAME}"), "", "d.payment_method", "total DESC"), from, to).await?,
            by_month: self.group_totals(&grouped(T, A, W, &format!("to_char(d.donation_date, 'YYYY-MM') AS key, {NO_NAME}"), "", "1", "1"), from, to).await?,
            top_donors: self
                .group_totals(
                    &format!(
                        "{} LIMIT {TOP_DONORS}",
                        grouped(T, A, W, "o.id::text AS key, o.full_name AS name_ar, o.full_name AS name_en", donors_join, "o.id, o.full_name", "total DESC")
                    ),
                    from,
                    to,
                )
                .await?,
        })
    }

    pub async fn aid_summary(&self, from: NaiveDate, to: NaiveDate) -> Result<AidSummary> {
        const T: &str = "aid_distributions x";
        // Aid is stored in its own currency; the posted entry's debit side carries the base amount.
        const A: &str = "(SELECT SUM(l.base_amount) FROM journal_lines l WHERE l.entry_id = x.journal_entry_id AND l.side = 'debit')";
        const W: &str = "x.distributed_on BETWEEN $1 AND $2";
        const NO_NAME: &str = "NULL::text AS name_ar, NULL::text AS name_en";
        let (total, count, beneficiaries): (Decimal, i64, i64) =
            sqlx::query_as(&format!("SELECT COALESCE(SUM({A}), 0), COUNT(*), COUNT(DISTINCT x.beneficiary_id) FROM {T} WHERE {W}"))
                .bind(from)
                .bind(to)
                .fetch_one(&self.pool)
                .await?;
        let cases_join = "JOIN beneficiaries b ON b.id = x.beneficiary_id";
        let funds_join = "JOIN funds f ON f.id = x.fund_id";
        Ok(AidSummary {
            from,
            to,
            total,
            count,
            beneficiaries,
            by_kind: self.group_totals(&grouped(T, A, W, &format!("x.kind::text AS key, {NO_NAME}"), "", "x.kind", "total DESC"), from, to).await?,
            by_category: self
                .group_totals(&grouped(T, A, W, &format!("b.category::text AS key, {NO_NAME}"), cases_join, "b.category", "total DESC"), from, to)
                .await?,
            by_fund: self.group_totals(&grouped(T, A, W, "f.code AS key, f.name_ar, f.name_en", funds_join, "f.code, f.name_ar, f.name_en", "total DESC"), from, to).await?,
            by_month: self.group_totals(&grouped(T, A, W, &format!("to_char(x.distributed_on, 'YYYY-MM') AS key, {NO_NAME}"), "", "1", "1"), from, to).await?,
        })
    }
}
