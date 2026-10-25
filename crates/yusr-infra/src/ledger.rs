//! Double-entry posting engine.
//!
//! [`post_in`] runs inside a caller-owned SERIALIZABLE transaction, so a use case (a
//! donation, an aid distribution) commits its own rows, the journal entry and the audit
//! record atomically. Funds touched by the entry are locked `FOR UPDATE` in id order,
//! which serialises concurrent spending from one fund and makes the balance and budget
//! checks race-free.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::budget::{BudgetPosition, BudgetStatus, check_fund_balance};
use yusr_domain::charity::{DonationKind, FundRestriction};
use yusr_domain::hijri::HijriDate;
use yusr_domain::ledger::{AccountType, EntryDraft, PostedLine, Side, prepare_entry, reversal_of};
use yusr_domain::money::Currency;

use crate::{InfraError, Result, Store, audit, fx, parse_label, serializable};

/// Account codes the use cases post to (seeded by migration 0002).
pub mod codes {
    pub const CASH: &str = "1100";
    pub const BANK: &str = "1110";
    pub const GATEWAY_CLEARING: &str = "1120";
    pub const INVENTORY: &str = "1300";
    pub const VOUCHERS_OUTSTANDING: &str = "2200";
    pub const FX_ROUNDING: &str = "4900";
    pub const CASH_AID: &str = "5100";
    pub const IN_KIND_AID: &str = "5200";
    pub const VOUCHER_AID: &str = "5300";
    pub const GATEWAY_FEES: &str = "6300";

    pub fn revenue_for(kind: yusr_domain::charity::DonationKind) -> &'static str {
        use yusr_domain::charity::DonationKind::*;
        match kind {
            Zakat => "4100",
            Sadaqah => "4200",
            Kaffarah => "4300",
            Waqf => "4400",
            General => "4500",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Fund {
    pub id: Uuid,
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub restriction: FundRestriction,
    pub donation_kind: Option<DonationKind>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: Uuid,
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub account_type: AccountType,
    pub parent_id: Option<Uuid>,
    pub is_postable: bool,
    pub is_active: bool,
    pub functional_class: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostedEntry {
    pub id: Uuid,
    pub entry_no: i64,
    pub entry_date: NaiveDate,
    pub hijri_date: String,
    pub lines: Vec<PostedLine>,
    /// Hash of the audit record that seals this entry, hex-encoded.
    pub audit_hash: String,
    /// Budgets that crossed the warning threshold with this entry.
    pub budget_warnings: Vec<Uuid>,
}

type FundRow = (Uuid, String, String, String, String, Option<String>);

fn fund_from((id, code, name_ar, name_en, restriction, kind): FundRow) -> Result<Fund> {
    Ok(Fund {
        id,
        code,
        name_ar,
        name_en,
        restriction: parse_label(&restriction)?,
        donation_kind: kind.as_deref().map(parse_label).transpose()?,
    })
}

const FUND_COLUMNS: &str = "id, code, name_ar, name_en, restriction::text, donation_kind::text";

pub async fn fund(conn: &mut PgConnection, id: Uuid) -> Result<Fund> {
    let row: Option<FundRow> = sqlx::query_as(&format!("SELECT {FUND_COLUMNS} FROM funds WHERE id = $1"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    fund_from(row.ok_or(InfraError::NotFound("fund"))?)
}

pub async fn account_id(conn: &mut PgConnection, code: &str) -> Result<Uuid> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM accounts WHERE code = $1").bind(code).fetch_optional(&mut *conn).await?;
    row.map(|(id,)| id).ok_or(InfraError::NotFound("account"))
}

/// Validates and posts `draft` on `conn`, which must be inside a SERIALIZABLE transaction.
pub async fn post_in(
    conn: &mut PgConnection,
    base: Currency,
    draft: &EntryDraft,
    actor: &str,
    source: &str,
    reverses: Option<Uuid>,
) -> Result<PostedEntry> {
    let hijri = HijriDate::from_gregorian(draft.entry_date)?;

    // Accounts: must exist, be active, and be postable (not headers).
    let account_ids: Vec<Uuid> = draft.lines.iter().map(|l| l.account_id).collect::<BTreeSet<_>>().into_iter().collect();
    let rows: Vec<(Uuid, String, bool, bool)> =
        sqlx::query_as("SELECT id, account_type::text, is_postable, is_active FROM accounts WHERE id = ANY($1)")
            .bind(&account_ids)
            .fetch_all(&mut *conn)
            .await?;
    if rows.len() != account_ids.len() {
        return Err(InfraError::NotFound("account"));
    }
    let mut account_types: HashMap<Uuid, AccountType> = HashMap::new();
    for (id, t, postable, active) in rows {
        if !postable || !active {
            return Err(DomainError::Invalid(format!("account {id} is a header or inactive")).into());
        }
        account_types.insert(id, parse_label(&t)?);
    }

    // Funds: locked in id order so concurrent postings cannot deadlock.
    let fund_ids: Vec<Uuid> = draft.lines.iter().map(|l| l.fund_id).collect::<BTreeSet<_>>().into_iter().collect();
    let rows: Vec<FundRow> = sqlx::query_as(&format!(
        "SELECT {FUND_COLUMNS} FROM funds WHERE id = ANY($1) AND is_active ORDER BY id FOR UPDATE"
    ))
    .bind(&fund_ids)
    .fetch_all(&mut *conn)
    .await?;
    if rows.len() != fund_ids.len() {
        return Err(InfraError::NotFound("active fund"));
    }
    let funds: HashMap<Uuid, Fund> = rows.into_iter().map(fund_from).map(|f| f.map(|f| (f.id, f))).collect::<Result<_>>()?;

    // Projects must belong to the line's fund.
    for l in &draft.lines {
        if let Some(p) = l.project_id {
            let ok: Option<(bool,)> = sqlx::query_as("SELECT fund_id = $2 FROM projects WHERE id = $1 AND is_active")
                .bind(p)
                .bind(l.fund_id)
                .fetch_optional(&mut *conn)
                .await?;
            if ok != Some((true,)) {
                return Err(DomainError::Invalid(format!("project {p} is inactive or not part of fund {}", l.fund_id)).into());
            }
        }
    }

    // Historical exchange rates, one lookup per foreign currency.
    let mut rates: HashMap<Currency, Decimal> = HashMap::new();
    for c in draft.lines.iter().map(|l| l.currency).filter(|c| *c != base).collect::<BTreeSet<_>>() {
        rates.insert(c, fx::rate_on(conn, c, draft.entry_date).await?);
    }
    let rounding = account_id(conn, codes::FX_ROUNDING).await?;
    account_types.insert(rounding, AccountType::Revenue);
    let lines = prepare_entry(draft, base, rounding, |_, c| Ok(rates[&c]))?;

    // Endowment principal is never spent.
    for l in &lines {
        if funds[&l.fund_id].restriction == FundRestriction::PermanentlyRestricted
            && account_types[&l.account_id] == AccountType::Expense
            && l.side == Side::Debit
        {
            return Err(DomainError::EndowmentPrincipalSpend.into());
        }
    }

    // Budgets: net expense per (fund, project) must fit every budget that covers it.
    let mut spend: BTreeMap<(Uuid, Option<Uuid>), Decimal> = BTreeMap::new();
    for l in lines.iter().filter(|l| account_types[&l.account_id] == AccountType::Expense) {
        *spend.entry((l.fund_id, l.project_id)).or_default() += l.side.sign() * l.base_amount;
    }
    let mut budget_warnings = Vec::new();
    for ((fund_id, project_id), amount) in spend.into_iter().filter(|(_, a)| *a > Decimal::ZERO) {
        let budgets: Vec<(Uuid, Option<Uuid>, NaiveDate, NaiveDate, Decimal, Decimal)> = sqlx::query_as(
            "SELECT id, project_id, period_start, period_end, approved, committed FROM budgets
              WHERE fund_id = $1 AND (project_id IS NULL OR project_id = $2)
                AND $3 BETWEEN period_start AND period_end
              ORDER BY id FOR UPDATE",
        )
        .bind(fund_id)
        .bind(project_id)
        .bind(draft.entry_date)
        .fetch_all(&mut *conn)
        .await?;
        for (budget_id, b_project, start, end, approved, committed) in budgets {
            let (spent,): (Decimal,) = sqlx::query_as(
                "SELECT COALESCE(SUM(l.signed_base), 0) FROM journal_lines l
                   JOIN journal_entries e ON e.id = l.entry_id
                   JOIN accounts a ON a.id = l.account_id
                  WHERE a.account_type = 'expense' AND l.fund_id = $1
                    AND ($2::uuid IS NULL OR l.project_id = $2)
                    AND e.entry_date BETWEEN $3 AND $4",
            )
            .bind(fund_id)
            .bind(b_project)
            .bind(start)
            .bind(end)
            .fetch_one(&mut *conn)
            .await?;
            let position = BudgetPosition { approved, spent, committed };
            if position.authorize(amount)? == BudgetStatus::Warning {
                budget_warnings.push(budget_id);
            }
        }
    }

    let (entry_id, entry_no): (Uuid, i64) = sqlx::query_as(
        "INSERT INTO journal_entries (entry_date, hijri_date, description, reference, reverses_id, source, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id, entry_no",
    )
    .bind(draft.entry_date)
    .bind(hijri.to_string())
    .bind(draft.description.trim())
    .bind(&draft.reference)
    .bind(reverses)
    .bind(source)
    .bind(actor)
    .fetch_one(&mut *conn)
    .await?;

    for (i, l) in lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO journal_lines (entry_id, line_no, account_id, fund_id, project_id, side, amount, currency, fx_rate, base_amount, memo)
             VALUES ($1, $2, $3, $4, $5, $6::entry_side, $7, $8, $9, $10, $11)",
        )
        .bind(entry_id)
        .bind(i as i32 + 1)
        .bind(l.account_id)
        .bind(l.fund_id)
        .bind(l.project_id)
        .bind(l.side.as_str())
        .bind(l.amount)
        .bind(l.currency.as_str())
        .bind(l.fx_rate)
        .bind(l.base_amount)
        .bind(&l.memo)
        .execute(&mut *conn)
        .await?;
    }

    // Restricted funds may not be overdrawn: their net assets must stay non-negative
    // whenever this entry reduced them.
    let mut delta: HashMap<Uuid, Decimal> = HashMap::new();
    for l in &lines {
        if matches!(account_types[&l.account_id], AccountType::Asset | AccountType::Liability) {
            *delta.entry(l.fund_id).or_default() += l.side.sign() * l.base_amount;
        }
    }
    for (fund_id, d) in delta.into_iter().filter(|(_, d)| *d < Decimal::ZERO) {
        let fund = &funds[&fund_id];
        if fund.restriction == FundRestriction::Unrestricted {
            continue;
        }
        let (after,): (Decimal,) = sqlx::query_as(
            "SELECT COALESCE(SUM(l.signed_base), 0) FROM journal_lines l JOIN accounts a ON a.id = l.account_id
              WHERE l.fund_id = $1 AND a.account_type IN ('asset', 'liability')",
        )
        .bind(fund_id)
        .fetch_one(&mut *conn)
        .await?;
        check_fund_balance(fund_id, true, after - d, -d)?;
    }

    let payload = serde_json::json!({
        "entry_no": entry_no,
        "entry_date": draft.entry_date,
        "hijri_date": hijri.to_string(),
        "description": draft.description.trim(),
        "reference": draft.reference,
        "reverses": reverses,
        "source": source,
        "lines": lines.iter().map(|l| serde_json::json!({
            "account_id": l.account_id,
            "fund_id": l.fund_id,
            "project_id": l.project_id,
            "side": l.side.as_str(),
            "amount": l.amount.to_string(),
            "currency": l.currency.as_str(),
            "fx_rate": l.fx_rate.to_string(),
            "base_amount": l.base_amount.to_string(),
        })).collect::<Vec<_>>(),
    });
    let hash = audit::append(conn, actor, "post", "journal_entry", &entry_id.to_string(), payload).await?;

    Ok(PostedEntry {
        id: entry_id,
        entry_no,
        entry_date: draft.entry_date,
        hijri_date: hijri.to_string(),
        lines,
        audit_hash: hex::encode(hash),
        budget_warnings,
    })
}

type LineRow = (Uuid, Uuid, Option<Uuid>, String, Decimal, String, Decimal, Decimal, Option<String>);

async fn load_lines(conn: &mut PgConnection, entry_id: Uuid) -> Result<Vec<PostedLine>> {
    let rows: Vec<LineRow> = sqlx::query_as(
        "SELECT account_id, fund_id, project_id, side::text, amount, currency::text, fx_rate, base_amount, memo
           FROM journal_lines WHERE entry_id = $1 ORDER BY line_no",
    )
    .bind(entry_id)
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|(account_id, fund_id, project_id, side, amount, currency, fx_rate, base_amount, memo)| {
            Ok(PostedLine {
                account_id,
                fund_id,
                project_id,
                side: parse_label(&side)?,
                amount,
                currency: currency.parse()?,
                fx_rate,
                base_amount,
                memo,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub id: Uuid,
    pub entry_no: i64,
    pub entry_date: NaiveDate,
    pub hijri_date: String,
    pub description: String,
    pub reference: Option<String>,
    pub status: String,
    pub source: String,
    pub created_by: String,
    pub reverses_id: Option<Uuid>,
    pub lines: Vec<PostedLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrialBalanceRow {
    pub account_id: Uuid,
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub account_type: String,
    pub debit: Decimal,
    pub credit: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct FundBalance {
    pub fund_id: Uuid,
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub restriction: String,
    pub net_assets: Decimal,
    pub revenue: Decimal,
    pub expenses: Decimal,
}

impl Store {
    /// Posts a manual journal entry.
    pub async fn post_entry(&self, draft: &EntryDraft, actor: &str) -> Result<PostedEntry> {
        let base = self.base_currency;
        serializable!(self, |conn| post_in(conn, base, draft, actor, "manual", None).await)
    }

    /// Reverses a posted entry on `date` with mirrored lines and marks the original reversed.
    pub async fn reverse_entry(&self, entry_id: Uuid, date: NaiveDate, reason: &str, actor: &str) -> Result<PostedEntry> {
        let base = self.base_currency;
        serializable!(self, |conn| {
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT entry_no, status::text FROM journal_entries WHERE id = $1 FOR UPDATE")
                    .bind(entry_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            let (entry_no, status) = row.ok_or(InfraError::NotFound("journal entry"))?;
            if status != "posted" {
                return Err(InfraError::Conflict("already_reversed", format!("entry {entry_no} is already reversed")));
            }
            let original = load_lines(conn, entry_id).await?;
            // Reverse in the original currencies and rates so the reversal nets to zero exactly.
            let draft = EntryDraft {
                entry_date: date,
                description: format!("Reversal of #{entry_no}: {reason}"),
                reference: Some(format!("REV-{entry_no}")),
                lines: reversal_of(&original)
                    .into_iter()
                    .map(|l| yusr_domain::ledger::LineDraft {
                        account_id: l.account_id,
                        fund_id: l.fund_id,
                        project_id: l.project_id,
                        side: l.side,
                        amount: l.base_amount,
                        currency: base,
                        memo: l.memo,
                    })
                    .collect(),
            };
            let posted = post_in(conn, base, &draft, actor, "reversal", Some(entry_id)).await?;
            sqlx::query("UPDATE journal_entries SET status = 'reversed' WHERE id = $1").bind(entry_id).execute(&mut *conn).await?;
            Ok(posted)
        })
    }

    pub async fn entry(&self, id: Uuid) -> Result<EntryView> {
        let mut conn = self.pool.acquire().await?;
        let row: Option<(Uuid, i64, NaiveDate, String, String, Option<String>, String, String, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT id, entry_no, entry_date, hijri_date, description, reference, status::text, source, created_by, reverses_id
               FROM journal_entries WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        let (id, entry_no, entry_date, hijri_date, description, reference, status, source, created_by, reverses_id) =
            row.ok_or(InfraError::NotFound("journal entry"))?;
        let lines = load_lines(&mut conn, id).await?;
        Ok(EntryView { id, entry_no, entry_date, hijri_date, description, reference, status, source, created_by, reverses_id, lines })
    }

    pub async fn list_entries(&self, from: NaiveDate, to: NaiveDate, limit: i64) -> Result<Vec<EntryView>> {
        let ids: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM journal_entries WHERE entry_date BETWEEN $1 AND $2 ORDER BY entry_no DESC LIMIT $3",
        )
        .bind(from)
        .bind(to)
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(ids.len());
        for (id,) in ids {
            out.push(self.entry(id).await?);
        }
        Ok(out)
    }

    pub async fn accounts(&self) -> Result<Vec<Account>> {
        let rows: Vec<(Uuid, String, String, String, String, Option<Uuid>, bool, bool, Option<String>)> = sqlx::query_as(
            "SELECT id, code, name_ar, name_en, account_type::text, parent_id, is_postable, is_active, functional_class::text
               FROM accounts ORDER BY code",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, code, name_ar, name_en, t, parent_id, is_postable, is_active, functional_class)| {
                Ok(Account { id, code, name_ar, name_en, account_type: parse_label(&t)?, parent_id, is_postable, is_active, functional_class })
            })
            .collect()
    }

    pub async fn create_account(&self, a: &NewAccount, actor: &str) -> Result<Uuid> {
        if a.functional_class.is_some() && a.account_type != AccountType::Expense {
            return Err(DomainError::Invalid("only expense accounts carry a functional class".into()).into());
        }
        serializable!(self, |conn| {
            if let Some(parent) = a.parent_id {
                let parent_type: Option<(String,)> = sqlx::query_as("SELECT account_type::text FROM accounts WHERE id = $1")
                    .bind(parent)
                    .fetch_optional(&mut *conn)
                    .await?;
                match parent_type {
                    Some((t,)) if t == a.account_type.as_str() => {}
                    Some(_) => return Err(DomainError::Invalid("a child account must have its parent's type".into()).into()),
                    None => return Err(InfraError::NotFound("parent account")),
                }
            }
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO accounts (code, name_ar, name_en, account_type, parent_id, is_postable, functional_class)
                 VALUES ($1, $2, $3, $4::account_type, $5, $6, $7::functional_class) RETURNING id",
            )
            .bind(&a.code)
            .bind(&a.name_ar)
            .bind(&a.name_en)
            .bind(a.account_type.as_str())
            .bind(a.parent_id)
            .bind(a.is_postable)
            .bind(&a.functional_class)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "account", &id.to_string(), serde_json::to_value(a).expect("serializes")).await?;
            Ok(id)
        })
    }

    pub async fn funds(&self) -> Result<Vec<Fund>> {
        let rows: Vec<FundRow> = sqlx::query_as(&format!("SELECT {FUND_COLUMNS} FROM funds WHERE is_active ORDER BY code"))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(fund_from).collect()
    }

    pub async fn create_fund(&self, f: &NewFund, actor: &str) -> Result<Uuid> {
        if let Some(kind) = f.donation_kind {
            // A dedicated fund accepts either the default class or the donor-designated one
            // (e.g. a restricted Sadaqah fund); anything else fails the domain check.
            let designated = f.restriction != kind.required_restriction(false);
            kind.check_fund(designated, f.restriction)?;
        }
        serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO funds (code, name_ar, name_en, restriction, donation_kind, purpose)
                 VALUES ($1, $2, $3, $4::fund_restriction, $5::donation_kind, $6) RETURNING id",
            )
            .bind(&f.code)
            .bind(&f.name_ar)
            .bind(&f.name_en)
            .bind(f.restriction.as_str())
            .bind(f.donation_kind.map(DonationKind::as_str))
            .bind(&f.purpose)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "fund", &id.to_string(), serde_json::to_value(f).expect("serializes")).await?;
            Ok(id)
        })
    }

    pub async fn trial_balance(&self, as_of: NaiveDate) -> Result<Vec<TrialBalanceRow>> {
        let rows: Vec<(Uuid, String, String, String, String, Decimal)> = sqlx::query_as(
            "SELECT a.id, a.code, a.name_ar, a.name_en, a.account_type::text, COALESCE(SUM(l.signed_base), 0)
               FROM accounts a
               JOIN journal_lines l ON l.account_id = a.id
               JOIN journal_entries e ON e.id = l.entry_id AND e.entry_date <= $1
              GROUP BY a.id ORDER BY a.code",
        )
        .bind(as_of)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(account_id, code, name_ar, name_en, account_type, net)| TrialBalanceRow {
                account_id,
                code,
                name_ar,
                name_en,
                account_type,
                debit: net.max(Decimal::ZERO),
                credit: (-net).max(Decimal::ZERO),
            })
            .collect())
    }

    /// Statement of net assets by fund (FASB 958 / IPSAS presentation input).
    pub async fn fund_balances(&self, as_of: NaiveDate) -> Result<Vec<FundBalance>> {
        let rows: Vec<(Uuid, String, String, String, String, Decimal, Decimal, Decimal)> = sqlx::query_as(
            "SELECT f.id, f.code, f.name_ar, f.name_en, f.restriction::text,
                    COALESCE(SUM(l.signed_base) FILTER (WHERE a.account_type IN ('asset', 'liability')), 0),
                    COALESCE(-SUM(l.signed_base) FILTER (WHERE a.account_type = 'revenue'), 0),
                    COALESCE(SUM(l.signed_base) FILTER (WHERE a.account_type = 'expense'), 0)
               FROM funds f
               LEFT JOIN journal_lines l ON l.fund_id = f.id
                     AND EXISTS (SELECT 1 FROM journal_entries e WHERE e.id = l.entry_id AND e.entry_date <= $1)
               LEFT JOIN accounts a ON a.id = l.account_id
              GROUP BY f.id ORDER BY f.code",
        )
        .bind(as_of)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(fund_id, code, name_ar, name_en, restriction, net_assets, revenue, expenses)| FundBalance {
                fund_id,
                code,
                name_ar,
                name_en,
                restriction,
                net_assets,
                revenue,
                expenses,
            })
            .collect())
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct NewAccount {
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub account_type: AccountType,
    pub parent_id: Option<Uuid>,
    #[serde(default = "yes")]
    pub is_postable: bool,
    pub functional_class: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct NewFund {
    pub code: String,
    pub name_ar: String,
    pub name_en: String,
    pub restriction: FundRestriction,
    pub donation_kind: Option<DonationKind>,
    pub purpose: Option<String>,
}
