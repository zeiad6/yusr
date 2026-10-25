//! Double-entry rules for fund accounting.
//!
//! Every fund is a self-balancing set of accounts: an entry must balance in base
//! currency *within each fund*, not only in total. Money moving between funds goes
//! through due-to / due-from accounts, which keeps each fund's net assets auditable.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DomainError;
use crate::money::{Currency, to_base};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Asset,
    Liability,
    /// Fund balance (net assets) — the charity equivalent of equity.
    NetAssets,
    Revenue,
    Expense,
}

impl AccountType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::NetAssets => "net_assets",
            Self::Revenue => "revenue",
            Self::Expense => "expense",
        }
    }

    pub fn normal_side(self) -> Side {
        match self {
            Self::Asset | Self::Expense => Side::Debit,
            Self::Liability | Self::NetAssets | Self::Revenue => Side::Credit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Debit,
    Credit,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }

    /// +1 for debit, −1 for credit: the sign convention of the `signed` columns.
    pub fn sign(self) -> Decimal {
        match self {
            Self::Debit => Decimal::ONE,
            Self::Credit => Decimal::NEGATIVE_ONE,
        }
    }
}

/// One line as submitted by a user or a use case, in its own currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineDraft {
    pub account_id: Uuid,
    pub fund_id: Uuid,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub side: Side,
    pub amount: Decimal,
    pub currency: Currency,
    #[serde(default)]
    pub memo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryDraft {
    pub entry_date: NaiveDate,
    pub description: String,
    #[serde(default)]
    pub reference: Option<String>,
    pub lines: Vec<LineDraft>,
}

/// A line after currency conversion, ready to persist.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostedLine {
    pub account_id: Uuid,
    pub fund_id: Uuid,
    pub project_id: Option<Uuid>,
    pub side: Side,
    pub amount: Decimal,
    pub currency: Currency,
    pub fx_rate: Decimal,
    pub base_amount: Decimal,
    pub memo: Option<String>,
}

/// Validates a draft and converts it to base currency.
///
/// `rate_for(line_index, currency)` returns base units per transaction unit on the entry
/// date (the infrastructure looks up the historical rate). Conversion rounding can leave
/// a fund a few minor units out of balance; up to one minor unit per line is posted to
/// `rounding_account`, anything larger is rejected as a genuine imbalance.
pub fn prepare_entry(
    draft: &EntryDraft,
    base: Currency,
    rounding_account: Uuid,
    mut rate_for: impl FnMut(usize, Currency) -> Result<Decimal, DomainError>,
) -> Result<Vec<PostedLine>, DomainError> {
    if draft.lines.len() < 2 {
        return Err(DomainError::TooFewLines);
    }
    if draft.description.trim().is_empty() {
        return Err(DomainError::Invalid("description is required".into()));
    }

    let mut lines = Vec::with_capacity(draft.lines.len() + 1);
    for (i, l) in draft.lines.iter().enumerate() {
        if l.amount <= Decimal::ZERO {
            return Err(DomainError::NonPositiveAmount { line: i, amount: l.amount });
        }
        if l.amount != l.currency.round(l.amount) {
            return Err(DomainError::Invalid(format!(
                "line {i}: {} has more decimals than {} allows",
                l.amount, l.currency
            )));
        }
        let rate = if l.currency == base { Decimal::ONE } else { rate_for(i, l.currency)? };
        if rate <= Decimal::ZERO {
            return Err(DomainError::InvalidRate { line: i, rate });
        }
        lines.push(PostedLine {
            account_id: l.account_id,
            fund_id: l.fund_id,
            project_id: l.project_id,
            side: l.side,
            amount: l.amount,
            currency: l.currency,
            fx_rate: rate,
            base_amount: to_base(l.amount, rate, base),
            memo: l.memo.clone(),
        });
    }

    let mut per_fund: BTreeMap<Uuid, (Decimal, usize, bool)> = BTreeMap::new();
    for l in &lines {
        let e = per_fund.entry(l.fund_id).or_insert((Decimal::ZERO, 0, false));
        e.0 += l.side.sign() * l.base_amount;
        e.1 += 1;
        e.2 |= l.currency != base;
    }
    for (fund, (diff, count, converted)) in per_fund {
        if diff.is_zero() {
            continue;
        }
        let tolerance = base.minor_unit() * Decimal::from(count);
        if !converted || diff.abs() > tolerance {
            return Err(DomainError::FundOutOfBalance { fund, difference: diff });
        }
        lines.push(PostedLine {
            account_id: rounding_account,
            fund_id: fund,
            project_id: None,
            // A positive difference means debits exceed credits, so the plug is a credit.
            side: if diff > Decimal::ZERO { Side::Credit } else { Side::Debit },
            amount: diff.abs(),
            currency: base,
            fx_rate: Decimal::ONE,
            base_amount: diff.abs(),
            memo: Some("FX rounding".into()),
        });
    }
    Ok(lines)
}

/// Builds the reversing entry for a posted entry: same lines with sides swapped.
/// Posted entries are never edited; corrections are always reversals.
pub fn reversal_of(lines: &[PostedLine]) -> Vec<PostedLine> {
    lines
        .iter()
        .map(|l| PostedLine {
            side: match l.side {
                Side::Debit => Side::Credit,
                Side::Credit => Side::Debit,
            },
            ..l.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn line(fund: Uuid, side: Side, amount: Decimal, cur: Currency) -> LineDraft {
        LineDraft {
            account_id: Uuid::new_v4(),
            fund_id: fund,
            project_id: None,
            side,
            amount,
            currency: cur,
            memo: None,
        }
    }

    fn draft(lines: Vec<LineDraft>) -> EntryDraft {
        EntryDraft {
            entry_date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            description: "test".into(),
            reference: None,
            lines,
        }
    }

    #[test]
    fn balanced_single_currency_passes() {
        let f = Uuid::new_v4();
        let d = draft(vec![
            line(f, Side::Debit, dec!(100), Currency::SAR),
            line(f, Side::Credit, dec!(100), Currency::SAR),
        ]);
        let out = prepare_entry(&d, Currency::SAR, Uuid::nil(), |_, _| unreachable!()).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|l| l.fx_rate == Decimal::ONE));
    }

    #[test]
    fn each_fund_must_self_balance() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let d = draft(vec![
            line(a, Side::Debit, dec!(100), Currency::SAR),
            line(b, Side::Credit, dec!(100), Currency::SAR),
        ]);
        let err = prepare_entry(&d, Currency::SAR, Uuid::nil(), |_, _| unreachable!()).unwrap_err();
        assert_eq!(err.code(), "fund_out_of_balance");
    }

    #[test]
    fn fx_rounding_is_plugged_within_tolerance() {
        let f = Uuid::new_v4();
        let rounding = Uuid::new_v4();
        // 3 × 33.33 USD at 3.75 → 124.99 each (124.9875); credit 99.99 USD → 374.96 (374.9625).
        let d = draft(vec![
            line(f, Side::Debit, dec!(33.33), Currency::USD),
            line(f, Side::Debit, dec!(33.33), Currency::USD),
            line(f, Side::Debit, dec!(33.33), Currency::USD),
            line(f, Side::Credit, dec!(99.99), Currency::USD),
        ]);
        let out = prepare_entry(&d, Currency::SAR, rounding, |_, _| Ok(dec!(3.75))).unwrap();
        let plug = out.iter().find(|l| l.account_id == rounding).expect("rounding line");
        assert_eq!(plug.base_amount, dec!(0.01));
        assert_eq!(plug.side, Side::Credit);
        let net: Decimal = out.iter().map(|l| l.side.sign() * l.base_amount).sum();
        assert!(net.is_zero());
    }

    #[test]
    fn rejects_real_imbalance_and_bad_amounts() {
        let f = Uuid::new_v4();
        let d = draft(vec![
            line(f, Side::Debit, dec!(100), Currency::SAR),
            line(f, Side::Credit, dec!(99.99), Currency::SAR),
        ]);
        assert!(prepare_entry(&d, Currency::SAR, Uuid::nil(), |_, _| unreachable!()).is_err());

        let d = draft(vec![
            line(f, Side::Debit, dec!(1.001), Currency::SAR),
            line(f, Side::Credit, dec!(1.001), Currency::SAR),
        ]);
        assert_eq!(
            prepare_entry(&d, Currency::SAR, Uuid::nil(), |_, _| unreachable!()).unwrap_err().code(),
            "invalid"
        );

        let d = draft(vec![line(f, Side::Debit, dec!(0), Currency::SAR), line(f, Side::Credit, dec!(0), Currency::SAR)]);
        assert_eq!(
            prepare_entry(&d, Currency::SAR, Uuid::nil(), |_, _| unreachable!()).unwrap_err().code(),
            "non_positive_amount"
        );
    }

    #[test]
    fn reversal_swaps_sides() {
        let f = Uuid::new_v4();
        let d = draft(vec![
            line(f, Side::Debit, dec!(5), Currency::SAR),
            line(f, Side::Credit, dec!(5), Currency::SAR),
        ]);
        let out = prepare_entry(&d, Currency::SAR, Uuid::nil(), |_, _| unreachable!()).unwrap();
        let rev = reversal_of(&out);
        assert_eq!(rev[0].side, Side::Credit);
        assert_eq!(rev[1].side, Side::Debit);
    }
}
