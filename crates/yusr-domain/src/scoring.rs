//! Multi-factor vulnerability scoring for beneficiary cases.
//!
//! Each factor yields a 0–1 sub-score; the weighted sum is scaled to 0–100 and mapped to
//! a priority tier. Weights are data, so a charity's board can tune them without code
//! changes; the defaults weight income adequacy highest, as most GCC social-aid
//! programmes do.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Housing {
    Owned,
    Rented,
    /// Living with relatives or in donated housing.
    Hosted,
    Homeless,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseProfile {
    pub household_size: u32,
    pub monthly_income: Decimal,
    /// Monthly per-person poverty line in the same currency.
    pub poverty_line_per_person: Decimal,
    pub dependents_under_18: u32,
    pub members_with_disability: u32,
    pub members_with_chronic_illness: u32,
    pub outstanding_debt: Decimal,
    pub housing: Housing,
    pub monthly_rent: Decimal,
    pub head_of_household_employed: bool,
    /// Widow, divorcee or orphan-headed household.
    pub female_or_orphan_headed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Weights {
    pub income: f64,
    pub dependents: f64,
    pub health: f64,
    pub debt: f64,
    pub housing: f64,
    pub employment: f64,
    pub household_head: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            income: 0.35,
            dependents: 0.10,
            health: 0.15,
            debt: 0.10,
            housing: 0.15,
            employment: 0.05,
            household_head: 0.10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Score {
    /// 0–100, higher means more vulnerable.
    pub score: u8,
    pub priority: Priority,
    pub factors: Vec<(&'static str, f64)>,
}

fn ratio(n: Decimal, d: Decimal) -> f64 {
    if d <= Decimal::ZERO { 1.0 } else { (n / d).to_f64().unwrap_or(1.0).clamp(0.0, 1.0) }
}

pub fn score(p: &CaseProfile, w: &Weights) -> Result<Score, DomainError> {
    if p.household_size == 0 {
        return Err(DomainError::Invalid("household_size must be at least 1".into()));
    }
    let total = w.income + w.dependents + w.health + w.debt + w.housing + w.employment + w.household_head;
    if !(0.999..=1.001).contains(&total) || [w.income, w.dependents, w.health, w.debt, w.housing, w.employment, w.household_head].iter().any(|x| *x < 0.0) {
        return Err(DomainError::Invalid(format!("weights must be non-negative and sum to 1, got {total}")));
    }
    let size = Decimal::from(p.household_size);
    let need = p.poverty_line_per_person * size;
    // Rent eats into income before anything else.
    let disposable = (p.monthly_income - p.monthly_rent).max(Decimal::ZERO);
    let income = 1.0 - ratio(disposable, need);
    let dependents = (f64::from(p.dependents_under_18) / f64::from(p.household_size)).min(1.0);
    let health = (f64::from(p.members_with_disability) * 1.0 + f64::from(p.members_with_chronic_illness) * 0.6)
        .min(f64::from(p.household_size))
        / f64::from(p.household_size);
    // Debt measured against one year of the household's poverty-line need.
    let debt = ratio(p.outstanding_debt, need * Decimal::from(12));
    let housing = match p.housing {
        Housing::Owned => 0.0,
        Housing::Hosted => 0.5,
        Housing::Rented => 0.3 + 0.7 * ratio(p.monthly_rent, p.monthly_income.max(Decimal::ONE)),
        Housing::Homeless => 1.0,
    };
    let employment = if p.head_of_household_employed { 0.0 } else { 1.0 };
    let head = if p.female_or_orphan_headed { 1.0 } else { 0.0 };

    let factors = vec![
        ("income", income),
        ("dependents", dependents),
        ("health", health),
        ("debt", debt),
        ("housing", housing),
        ("employment", employment),
        ("household_head", head),
    ];
    let raw = income * w.income
        + dependents * w.dependents
        + health * w.health
        + debt * w.debt
        + housing * w.housing
        + employment * w.employment
        + head * w.household_head;
    let score = (raw * 100.0).round().clamp(0.0, 100.0) as u8;
    let priority = match score {
        75..=100 => Priority::Critical,
        55..=74 => Priority::High,
        35..=54 => Priority::Medium,
        _ => Priority::Low,
    };
    Ok(Score { score, priority, factors })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn family() -> CaseProfile {
        CaseProfile {
            household_size: 6,
            monthly_income: dec!(0),
            poverty_line_per_person: dec!(1000),
            dependents_under_18: 4,
            members_with_disability: 1,
            members_with_chronic_illness: 1,
            outstanding_debt: dec!(80000),
            housing: Housing::Rented,
            monthly_rent: dec!(0),
            head_of_household_employed: false,
            female_or_orphan_headed: true,
        }
    }

    #[test]
    fn destitute_widow_household_is_critical() {
        let s = score(&family(), &Weights::default()).unwrap();
        assert_eq!(s.priority, Priority::Critical);
        assert_eq!(s.score, 75);
    }

    #[test]
    fn self_sufficient_household_is_low() {
        let p = CaseProfile {
            household_size: 3,
            monthly_income: dec!(15000),
            dependents_under_18: 1,
            members_with_disability: 0,
            members_with_chronic_illness: 0,
            outstanding_debt: dec!(0),
            housing: Housing::Owned,
            head_of_household_employed: true,
            female_or_orphan_headed: false,
            ..family()
        };
        let s = score(&p, &Weights::default()).unwrap();
        assert_eq!(s.priority, Priority::Low);
        assert!(s.score < 10, "{}", s.score);
    }

    #[test]
    fn rejects_bad_weights_and_empty_household() {
        let w = Weights { income: 0.9, ..Weights::default() };
        assert!(score(&family(), &w).is_err());
        let p = CaseProfile { household_size: 0, ..family() };
        assert!(score(&p, &Weights::default()).is_err());
    }
}
