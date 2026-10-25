use rust_decimal::Decimal;
use thiserror::Error;
use uuid::Uuid;

/// Every business-rule violation the domain can report. Messages are English; the UI
/// translates by matching on [`DomainError::code`].
#[derive(Debug, Error, Clone, PartialEq)]
pub enum DomainError {
    #[error("invalid currency code `{0}`")]
    InvalidCurrency(String),
    #[error("a journal entry needs at least two lines")]
    TooFewLines,
    #[error("line {line}: amount must be positive, got {amount}")]
    NonPositiveAmount { line: usize, amount: Decimal },
    #[error("line {line}: exchange rate must be positive, got {rate}")]
    InvalidRate { line: usize, rate: Decimal },
    #[error("fund {fund} is out of balance by {difference} in base currency")]
    FundOutOfBalance { fund: Uuid, difference: Decimal },
    #[error("{kind} money cannot be placed in a {restriction} fund")]
    RestrictionMismatch { kind: &'static str, restriction: &'static str },
    #[error("{kind} funds cannot be disbursed to the `{category}` category")]
    IneligibleRecipient { kind: &'static str, category: &'static str },
    #[error("permanently restricted (endowment) principal cannot be spent")]
    EndowmentPrincipalSpend,
    #[error("budget exceeded: available {available}, requested {requested}")]
    BudgetExceeded { available: Decimal, requested: Decimal },
    #[error("restricted fund {fund} would go negative: balance {balance}, requested {requested}")]
    InsufficientFundBalance { fund: Uuid, balance: Decimal, requested: Decimal },
    #[error("invalid Hijri date {year}-{month}-{day}")]
    InvalidHijriDate { year: i32, month: u32, day: u32 },
    #[error("date outside the Umm al-Qura table (1300–1600 AH)")]
    HijriOutOfRange,
    #[error("invalid VAT registration number `{0}`")]
    InvalidVatNumber(String),
    #[error("ZATCA: {0}")]
    Zatca(String),
    #[error("audit chain broken at sequence {seq}: {reason}")]
    AuditChainBroken { seq: i64, reason: &'static str },
    #[error("invalid voucher code")]
    InvalidVoucher,
    #[error("{0}")]
    Invalid(String),
}

impl DomainError {
    /// Stable machine-readable code for API clients and UI translation tables.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCurrency(_) => "invalid_currency",
            Self::TooFewLines => "too_few_lines",
            Self::NonPositiveAmount { .. } => "non_positive_amount",
            Self::InvalidRate { .. } => "invalid_rate",
            Self::FundOutOfBalance { .. } => "fund_out_of_balance",
            Self::RestrictionMismatch { .. } => "restriction_mismatch",
            Self::IneligibleRecipient { .. } => "ineligible_recipient",
            Self::EndowmentPrincipalSpend => "endowment_principal_spend",
            Self::BudgetExceeded { .. } => "budget_exceeded",
            Self::InsufficientFundBalance { .. } => "insufficient_fund_balance",
            Self::InvalidHijriDate { .. } => "invalid_hijri_date",
            Self::HijriOutOfRange => "hijri_out_of_range",
            Self::InvalidVatNumber(_) => "invalid_vat_number",
            Self::Zatca(_) => "zatca",
            Self::AuditChainBroken { .. } => "audit_chain_broken",
            Self::InvalidVoucher => "invalid_voucher",
            Self::Invalid(_) => "invalid",
        }
    }
}
