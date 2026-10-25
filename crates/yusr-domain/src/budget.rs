//! Over-spend prevention for funds and projects.
//!
//! The infrastructure locks the budget row (`SELECT … FOR UPDATE`) inside the posting
//! transaction and calls [`BudgetPosition::authorize`], so two concurrent payments can
//! never both pass a check that only one of them fits.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BudgetPosition {
    pub approved: Decimal,
    /// Expenses already posted against the budget.
    pub spent: Decimal,
    /// Approved purchase orders and pledges not yet posted.
    pub committed: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    Ok,
    /// Utilisation after this spend crosses the warning threshold.
    Warning,
}

/// Utilisation (spent + committed) / approved at which a warning is raised: 90 %.
pub const WARNING_THRESHOLD: Decimal = Decimal::from_parts(90, 0, 0, false, 2);

impl BudgetPosition {
    pub fn available(&self) -> Decimal {
        self.approved - self.spent - self.committed
    }

    pub fn utilisation(&self) -> Decimal {
        if self.approved.is_zero() {
            Decimal::ONE
        } else {
            (self.spent + self.committed) / self.approved
        }
    }

    /// Approves `amount` or rejects it when it would exceed the budget.
    pub fn authorize(&self, amount: Decimal) -> Result<BudgetStatus, DomainError> {
        if amount > self.available() {
            return Err(DomainError::BudgetExceeded { available: self.available(), requested: amount });
        }
        let after = BudgetPosition { spent: self.spent + amount, ..*self };
        Ok(if after.utilisation() >= WARNING_THRESHOLD { BudgetStatus::Warning } else { BudgetStatus::Ok })
    }
}

/// A restricted fund may never be overdrawn: spending must be backed by its own
/// net assets, not borrowed silently from another donor's money.
pub fn check_fund_balance(
    fund: uuid::Uuid,
    restricted: bool,
    balance: Decimal,
    requested: Decimal,
) -> Result<(), DomainError> {
    if restricted && requested > balance {
        Err(DomainError::InsufficientFundBalance { fund, balance, requested })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn authorizes_within_budget_and_warns_near_limit() {
        let b = BudgetPosition { approved: dec!(1000), spent: dec!(500), committed: dec!(100) };
        assert_eq!(b.authorize(dec!(100)).unwrap(), BudgetStatus::Ok);
        assert_eq!(b.authorize(dec!(300)).unwrap(), BudgetStatus::Warning);
        assert_eq!(b.authorize(dec!(400)).unwrap(), BudgetStatus::Warning);
        let e = b.authorize(dec!(400.01)).unwrap_err();
        assert_eq!(e, DomainError::BudgetExceeded { available: dec!(400), requested: dec!(400.01) });
    }

    #[test]
    fn restricted_funds_cannot_be_overdrawn() {
        let f = uuid::Uuid::nil();
        assert!(check_fund_balance(f, true, dec!(50), dec!(50)).is_ok());
        assert!(check_fund_balance(f, true, dec!(50), dec!(50.01)).is_err());
        assert!(check_fund_balance(f, false, dec!(0), dec!(10)).is_ok());
    }
}
