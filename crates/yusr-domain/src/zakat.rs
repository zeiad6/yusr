//! Zakat assessment on net zakatable assets.
//!
//! Defaults follow the contemporary consensus used across the region (AAOIFI Shari'ah
//! Standard No. 35 and the rulings of the International Islamic Fiqh Academy):
//! - nisab = 85 g of pure gold, or 595 g of silver when the policy chooses silver;
//! - rate = 2.5 % per lunar year, 2.5775 % when the books close on a solar year;
//! - deductible liabilities are those due within the coming twelve months;
//! - receivables count only when they are expected to be collected;
//! - fixed (non-trade) assets carry no Zakat; trade goods count at market value.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::DomainError;
use crate::hijri::HijriDate;

pub const GOLD_NISAB_GRAMS: Decimal = Decimal::from_parts(85, 0, 0, false, 0);
pub const SILVER_NISAB_GRAMS: Decimal = Decimal::from_parts(595, 0, 0, false, 0);
/// 2.5 %
pub const LUNAR_RATE: Decimal = Decimal::from_parts(25, 0, 0, false, 3);
/// 2.5775 % — the lunar rate scaled by 365.25 / 354.367 (AAOIFI).
pub const SOLAR_RATE: Decimal = Decimal::from_parts(25775, 0, 0, false, 6);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NisabBasis {
    #[default]
    Gold,
    Silver,
    /// The lower of the two — the more cautious view that makes Zakat due sooner.
    LowerOfGoldAndSilver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum YearBasis {
    #[default]
    Lunar,
    Solar,
}

/// Everything is in the assessment currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ZakatInput {
    pub cash_and_bank: Decimal,
    /// Grams of gold held (any karat converted to 24k by the caller).
    #[serde(default)]
    pub gold_grams: Decimal,
    #[serde(default)]
    pub silver_grams: Decimal,
    pub gold_price_per_gram: Decimal,
    pub silver_price_per_gram: Decimal,
    /// Inventory held for sale, at market (not cost) value.
    #[serde(default)]
    pub trade_goods_market_value: Decimal,
    /// Receivables the holder reasonably expects to collect.
    #[serde(default)]
    pub collectible_receivables: Decimal,
    /// Shares and funds held for trading, at market value.
    #[serde(default)]
    pub trading_investments: Decimal,
    /// The zakatable share of long-term investments (e.g. the investee's zakatable assets
    /// per share × shares held), computed by the caller.
    #[serde(default)]
    pub zakatable_share_of_long_term_investments: Decimal,
    /// Liabilities due within twelve months.
    #[serde(default)]
    pub current_liabilities: Decimal,
    #[serde(default)]
    pub nisab_basis: NisabBasis,
    #[serde(default)]
    pub year_basis: YearBasis,
    /// Start of the hawl; when given, Zakat is due only once a full lunar year has passed.
    #[serde(default)]
    pub hawl_start: Option<NaiveDate>,
    pub assessment_date: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZakatAssessment {
    pub gross_zakatable_assets: Decimal,
    pub deductible_liabilities: Decimal,
    pub net_zakatable_assets: Decimal,
    pub nisab_value: Decimal,
    pub rate: Decimal,
    pub hawl_complete: bool,
    /// Hijri date on which the hawl completes, when a hawl start was given.
    pub hawl_due_hijri: Option<HijriDate>,
    pub zakat_due: Decimal,
    /// Human-readable reason when no Zakat is due.
    pub exemption_reason: Option<&'static str>,
}

pub fn assess(input: &ZakatInput) -> Result<ZakatAssessment, DomainError> {
    let non_negative = [
        ("cash_and_bank", input.cash_and_bank),
        ("gold_grams", input.gold_grams),
        ("silver_grams", input.silver_grams),
        ("gold_price_per_gram", input.gold_price_per_gram),
        ("silver_price_per_gram", input.silver_price_per_gram),
        ("trade_goods_market_value", input.trade_goods_market_value),
        ("collectible_receivables", input.collectible_receivables),
        ("trading_investments", input.trading_investments),
        ("zakatable_share_of_long_term_investments", input.zakatable_share_of_long_term_investments),
        ("current_liabilities", input.current_liabilities),
    ];
    if let Some((name, _)) = non_negative.iter().find(|(_, v)| v.is_sign_negative()) {
        return Err(DomainError::Invalid(format!("{name} cannot be negative")));
    }
    if input.gold_price_per_gram.is_zero() && input.nisab_basis != NisabBasis::Silver {
        return Err(DomainError::Invalid("gold_price_per_gram is required for the nisab".into()));
    }
    if input.silver_price_per_gram.is_zero() && input.nisab_basis != NisabBasis::Gold {
        return Err(DomainError::Invalid("silver_price_per_gram is required for the nisab".into()));
    }

    let gross = input.cash_and_bank
        + input.gold_grams * input.gold_price_per_gram
        + input.silver_grams * input.silver_price_per_gram
        + input.trade_goods_market_value
        + input.collectible_receivables
        + input.trading_investments
        + input.zakatable_share_of_long_term_investments;
    let deductible = input.current_liabilities.min(gross);
    let net = gross - deductible;

    let gold_nisab = GOLD_NISAB_GRAMS * input.gold_price_per_gram;
    let silver_nisab = SILVER_NISAB_GRAMS * input.silver_price_per_gram;
    let nisab = match input.nisab_basis {
        NisabBasis::Gold => gold_nisab,
        NisabBasis::Silver => silver_nisab,
        NisabBasis::LowerOfGoldAndSilver => gold_nisab.min(silver_nisab),
    };
    let rate = match input.year_basis {
        YearBasis::Lunar => LUNAR_RATE,
        YearBasis::Solar => SOLAR_RATE,
    };

    let (hawl_complete, hawl_due_hijri) = match input.hawl_start {
        None => (true, None),
        Some(start) => {
            if start > input.assessment_date {
                return Err(DomainError::Invalid("hawl_start is after the assessment date".into()));
            }
            let due = HijriDate::from_gregorian(start)?.add_years(1)?;
            (input.assessment_date >= due.to_gregorian()?, Some(due))
        }
    };

    let (zakat_due, exemption_reason) = if !hawl_complete {
        (Decimal::ZERO, Some("hawl_not_complete"))
    } else if net < nisab {
        (Decimal::ZERO, Some("below_nisab"))
    } else {
        ((net * rate).round_dp(2), None)
    };

    Ok(ZakatAssessment {
        gross_zakatable_assets: gross,
        deductible_liabilities: deductible,
        net_zakatable_assets: net,
        nisab_value: nisab,
        rate,
        hawl_complete,
        hawl_due_hijri,
        zakat_due,
        exemption_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn base() -> ZakatInput {
        ZakatInput {
            cash_and_bank: dec!(100000),
            gold_price_per_gram: dec!(300),
            silver_price_per_gram: dec!(3.5),
            assessment_date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn standard_lunar_assessment() {
        let mut i = base();
        i.trade_goods_market_value = dec!(50000);
        i.current_liabilities = dec!(30000);
        let a = assess(&i).unwrap();
        assert_eq!(a.net_zakatable_assets, dec!(120000));
        assert_eq!(a.nisab_value, dec!(25500));
        assert_eq!(a.zakat_due, dec!(3000.00));
        assert!(a.exemption_reason.is_none());
    }

    #[test]
    fn solar_rate_and_silver_nisab() {
        let mut i = base();
        i.year_basis = YearBasis::Solar;
        i.cash_and_bank = dec!(10000);
        i.nisab_basis = NisabBasis::Silver;
        let a = assess(&i).unwrap();
        assert_eq!(a.nisab_value, dec!(2082.5));
        assert_eq!(a.zakat_due, dec!(257.75));
    }

    #[test]
    fn below_nisab_is_exempt_and_liabilities_cannot_go_negative() {
        let mut i = base();
        i.cash_and_bank = dec!(20000);
        i.current_liabilities = dec!(50000);
        let a = assess(&i).unwrap();
        assert_eq!(a.net_zakatable_assets, Decimal::ZERO);
        assert_eq!(a.zakat_due, Decimal::ZERO);
        assert_eq!(a.exemption_reason, Some("below_nisab"));
    }

    #[test]
    fn hawl_must_complete_in_lunar_terms() {
        let mut i = base();
        // 1 Ramadan 1445 = 2024-03-11, so the hawl completes on 1 Ramadan 1446 = 2025-03-01.
        i.hawl_start = Some(NaiveDate::from_ymd_opt(2024, 3, 11).unwrap());
        let a = assess(&i).unwrap();
        assert!(a.hawl_complete);
        assert_eq!(a.hawl_due_hijri.unwrap().to_string(), "1446-09-01");

        i.assessment_date = NaiveDate::from_ymd_opt(2025, 2, 28).unwrap();
        let a = assess(&i).unwrap();
        assert!(!a.hawl_complete);
        assert_eq!(a.exemption_reason, Some("hawl_not_complete"));
    }

    #[test]
    fn rejects_negative_inputs() {
        let mut i = base();
        i.cash_and_bank = dec!(-1);
        assert!(assess(&i).is_err());
    }
}
