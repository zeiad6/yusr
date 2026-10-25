//! Umm al-Qura (أم القرى) Hijri calendar, the official calendar of Saudi Arabia.
//!
//! Conversion is table-driven: month lengths for 1300–1600 AH are generated from ICU's
//! islamic-umalqura data by `tools/gen_umalqura.js`, so results match government dates
//! instead of an arithmetic approximation that drifts by a day or two.

use std::fmt;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::DomainError;
use crate::hijri_table::{EPOCH_DAYS, FIRST_YEAR, MONTH_LENGTHS};

const UNIX_EPOCH_CE_DAYS: i32 = 719_163; // NaiveDate::num_days_from_ce() of 1970-01-01

const MONTHS_AR: [&str; 12] = [
    "محرم", "صفر", "ربيع الأول", "ربيع الآخر", "جمادى الأولى", "جمادى الآخرة",
    "رجب", "شعبان", "رمضان", "شوال", "ذو القعدة", "ذو الحجة",
];
const MONTHS_EN: [&str; 12] = [
    "Muharram", "Safar", "Rabi al-Awwal", "Rabi al-Thani", "Jumada al-Ula", "Jumada al-Akhirah",
    "Rajab", "Shaban", "Ramadan", "Shawwal", "Dhu al-Qadah", "Dhu al-Hijjah",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HijriDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

pub const LAST_YEAR: i32 = FIRST_YEAR + MONTH_LENGTHS.len() as i32 - 1;

fn mask(year: i32) -> Result<u16, DomainError> {
    if (FIRST_YEAR..=LAST_YEAR).contains(&year) {
        Ok(MONTH_LENGTHS[(year - FIRST_YEAR) as usize])
    } else {
        Err(DomainError::HijriOutOfRange)
    }
}

pub fn month_length(year: i32, month: u32) -> Result<u32, DomainError> {
    if !(1..=12).contains(&month) {
        return Err(DomainError::InvalidHijriDate { year, month, day: 1 });
    }
    Ok(if mask(year)? & (1 << (month - 1)) != 0 { 30 } else { 29 })
}

pub fn year_length(year: i32) -> Result<u32, DomainError> {
    Ok(12 * 29 + mask(year)?.count_ones())
}

impl HijriDate {
    /// Validates the date against the Umm al-Qura month lengths.
    pub fn new(year: i32, month: u32, day: u32) -> Result<Self, DomainError> {
        let len = month_length(year, month)?;
        if day == 0 || day > len {
            return Err(DomainError::InvalidHijriDate { year, month, day });
        }
        Ok(Self { year, month, day })
    }

    pub fn from_gregorian(date: NaiveDate) -> Result<Self, DomainError> {
        let unix_days = i64::from(date.num_days_from_ce() - UNIX_EPOCH_CE_DAYS);
        let mut rem = unix_days - EPOCH_DAYS;
        if rem < 0 {
            return Err(DomainError::HijriOutOfRange);
        }
        let mut year = FIRST_YEAR;
        loop {
            let len = i64::from(year_length(year)?);
            if rem < len {
                break;
            }
            rem -= len;
            year += 1;
        }
        let mut month = 1;
        loop {
            let len = i64::from(month_length(year, month)?);
            if rem < len {
                break;
            }
            rem -= len;
            month += 1;
        }
        Ok(Self { year, month, day: rem as u32 + 1 })
    }

    pub fn to_gregorian(self) -> Result<NaiveDate, DomainError> {
        let h = Self::new(self.year, self.month, self.day)?;
        let mut days = EPOCH_DAYS;
        for y in FIRST_YEAR..h.year {
            days += i64::from(year_length(y)?);
        }
        for m in 1..h.month {
            days += i64::from(month_length(h.year, m)?);
        }
        days += i64::from(h.day - 1);
        let ce = i32::try_from(days).map_err(|_| DomainError::HijriOutOfRange)? + UNIX_EPOCH_CE_DAYS;
        NaiveDate::from_num_days_from_ce_opt(ce).ok_or(DomainError::HijriOutOfRange)
    }

    /// Same day `years` Hijri years later, clamped to the month's last day — used to find
    /// when a Zakat hawl (one full lunar year of ownership) completes.
    pub fn add_years(self, years: i32) -> Result<Self, DomainError> {
        let year = self.year + years;
        let day = self.day.min(month_length(year, self.month)?);
        Self::new(year, self.month, day)
    }

    pub fn month_name_ar(self) -> &'static str {
        MONTHS_AR[(self.month - 1) as usize]
    }

    pub fn month_name_en(self) -> &'static str {
        MONTHS_EN[(self.month - 1) as usize]
    }

    /// e.g. "١ رمضان ١٤٤٦ هـ".
    pub fn format_ar(self) -> String {
        format!("{} {} {} هـ", arabic_digits(self.day), self.month_name_ar(), arabic_digits(self.year))
    }

    /// e.g. "1 Ramadan 1446 AH".
    pub fn format_en(self) -> String {
        format!("{} {} {} AH", self.day, self.month_name_en(), self.year)
    }
}

impl fmt::Display for HijriDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl std::str::FromStr for HijriDate {
    type Err = DomainError;
    /// Parses `YYYY-MM-DD` (Hijri) and validates it.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || DomainError::Invalid(format!("expected Hijri date YYYY-MM-DD, got `{s}`"));
        let mut it = s.splitn(3, '-');
        let mut next = || it.next().ok_or_else(bad)?.parse::<i64>().map_err(|_| bad());
        let (y, m, d) = (next()?, next()?, next()?);
        let (y, m, d) = (i32::try_from(y).map_err(|_| bad())?, u32::try_from(m).map_err(|_| bad())?, u32::try_from(d).map_err(|_| bad())?);
        Self::new(y, m, d)
    }
}

/// Renders an integer with Arabic-Indic digits (٠-٩).
pub fn arabic_digits(n: impl fmt::Display) -> String {
    n.to_string()
        .chars()
        .map(|c| c.to_digit(10).map_or(c, |d| char::from_u32(0x0660 + d).unwrap_or(c)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn known_official_dates() {
        // Published Umm al-Qura dates.
        let cases = [
            (g(2024, 7, 7), (1446, 1, 1)),
            (g(2024, 3, 11), (1445, 9, 1)),
            (g(2025, 3, 1), (1446, 9, 1)),
            (g(2023, 6, 27), (1444, 12, 9)), // Day of Arafah 1444
            (g(1900, 4, 30), (1318, 1, 1)),
        ];
        for (greg, (y, m, d)) in cases {
            let h = HijriDate::from_gregorian(greg).unwrap();
            assert_eq!((h.year, h.month, h.day), (y, m, d), "{greg}");
            assert_eq!(h.to_gregorian().unwrap(), greg);
        }
    }

    #[test]
    fn roundtrip_every_day_over_a_century() {
        let mut d = g(1950, 1, 1);
        while d < g(2077, 1, 1) {
            let h = HijriDate::from_gregorian(d).unwrap();
            assert_eq!(h.to_gregorian().unwrap(), d);
            d = d.succ_opt().unwrap();
        }
    }

    #[test]
    fn validation_and_range() {
        assert!(HijriDate::new(1446, 13, 1).is_err());
        assert!(HijriDate::new(1446, 1, 31).is_err());
        assert!(HijriDate::new(1299, 1, 1).is_err());
        assert!(HijriDate::from_gregorian(g(1800, 1, 1)).is_err());
        assert_eq!("1446-09-01".parse::<HijriDate>().unwrap(), HijriDate { year: 1446, month: 9, day: 1 });
        assert!("1446-9".parse::<HijriDate>().is_err());
    }

    #[test]
    fn formatting() {
        let h = HijriDate::new(1446, 9, 1).unwrap();
        assert_eq!(h.format_ar(), "١ رمضان ١٤٤٦ هـ");
        assert_eq!(h.format_en(), "1 Ramadan 1446 AH");
        assert_eq!(h.to_string(), "1446-09-01");
    }

    #[test]
    fn add_years_clamps_day() {
        let h = HijriDate::new(1445, 1, 1).unwrap();
        assert_eq!(h.add_years(1).unwrap(), HijriDate::new(1446, 1, 1).unwrap());
        // 30th of a month whose next-year twin has 29 days clamps to 29.
        let (y, m) = (FIRST_YEAR..LAST_YEAR)
            .flat_map(|y| (1..=12).map(move |m| (y, m)))
            .find(|&(y, m)| month_length(y, m).unwrap() == 30 && month_length(y + 1, m).unwrap() == 29)
            .unwrap();
        assert_eq!(HijriDate::new(y, m, 30).unwrap().add_years(1).unwrap().day, 29);
    }
}
