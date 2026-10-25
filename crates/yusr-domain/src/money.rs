use std::fmt;
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::DomainError;

/// ISO 4217 currency code, validated as three ASCII uppercase letters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency([u8; 3]);

impl Currency {
    pub const SAR: Currency = Currency(*b"SAR");
    pub const KWD: Currency = Currency(*b"KWD");
    pub const BHD: Currency = Currency(*b"BHD");
    pub const EGP: Currency = Currency(*b"EGP");
    pub const USD: Currency = Currency(*b"USD");

    pub fn as_str(&self) -> &str {
        // Construction guarantees ASCII.
        std::str::from_utf8(&self.0).expect("currency code is ASCII")
    }

    /// Number of minor units (decimal places) per ISO 4217.
    pub fn minor_units(&self) -> u32 {
        match &self.0 {
            b"KWD" | b"BHD" | b"OMR" | b"JOD" | b"IQD" | b"LYD" | b"TND" => 3,
            b"JPY" | b"KRW" => 0,
            _ => 2,
        }
    }

    /// Rounds to the currency's minor unit using banker's rounding, the IFRS-neutral choice.
    pub fn round(&self, amount: Decimal) -> Decimal {
        amount.round_dp_with_strategy(self.minor_units(), RoundingStrategy::MidpointNearestEven)
    }

    /// Smallest representable amount, e.g. 0.01 SAR or 0.001 KWD.
    pub fn minor_unit(&self) -> Decimal {
        Decimal::new(1, self.minor_units())
    }
}

impl FromStr for Currency {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() == 3 && b.iter().all(u8::is_ascii_uppercase) {
            Ok(Currency([b[0], b[1], b[2]]))
        } else {
            Err(DomainError::InvalidCurrency(s.to_owned()))
        }
    }
}

impl TryFrom<String> for Currency {
    type Error = DomainError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Currency> for String {
    fn from(c: Currency) -> Self {
        c.as_str().to_owned()
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Currency({})", self.as_str())
    }
}

/// Converts a transaction-currency amount to the base currency at `rate`
/// (base units per one transaction unit), rounded to the base currency's minor unit.
pub fn to_base(amount: Decimal, rate: Decimal, base: Currency) -> Decimal {
    base.round(amount * rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_and_rejects_codes() {
        assert_eq!("SAR".parse::<Currency>().unwrap(), Currency::SAR);
        assert!("sar".parse::<Currency>().is_err());
        assert!("SARR".parse::<Currency>().is_err());
    }

    #[test]
    fn rounds_per_minor_units() {
        assert_eq!(Currency::KWD.round(dec!(1.23456)), dec!(1.235));
        assert_eq!(Currency::SAR.round(dec!(1.005)), dec!(1.00));
        assert_eq!(Currency::SAR.round(dec!(1.015)), dec!(1.02));
        assert_eq!(to_base(dec!(100), dec!(12.2345), Currency::SAR), dec!(1223.45));
    }

    #[test]
    fn serde_roundtrip() {
        let c: Currency = serde_json::from_str("\"BHD\"").unwrap();
        assert_eq!(c.minor_units(), 3);
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"BHD\"");
        assert!(serde_json::from_str::<Currency>("\"bh\"").is_err());
    }
}
