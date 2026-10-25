//! Charity-specific classifications: fund restrictions, Islamic giving types and the
//! eight Zakat recipient categories (al-asnaf al-thamaniya, Qur'an 9:60).

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// Net-asset restriction class of a fund (FASB ASC 958 / IPSAS terminology).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundRestriction {
    Unrestricted,
    /// Restricted to a purpose or period; released once the condition is met.
    TemporarilyRestricted,
    /// Endowment (Waqf) principal: only its returns may be spent.
    PermanentlyRestricted,
}

impl FundRestriction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unrestricted => "unrestricted",
            Self::TemporarilyRestricted => "temporarily_restricted",
            Self::PermanentlyRestricted => "permanently_restricted",
        }
    }
}

/// Islamic giving type of a donation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DonationKind {
    /// زكاة — obligatory alms, payable only to the eight asnaf.
    Zakat,
    /// صدقة — voluntary charity.
    Sadaqah,
    /// كفارة — expiation, spent on feeding or clothing the poor and needy.
    Kaffarah,
    /// وقف — endowment; the principal is preserved forever.
    Waqf,
    /// Grants and other gifts that carry no religious classification.
    General,
}

impl DonationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zakat => "zakat",
            Self::Sadaqah => "sadaqah",
            Self::Kaffarah => "kaffarah",
            Self::Waqf => "waqf",
            Self::General => "general",
        }
    }

    pub fn arabic(self) -> &'static str {
        match self {
            Self::Zakat => "زكاة",
            Self::Sadaqah => "صدقة",
            Self::Kaffarah => "كفارة",
            Self::Waqf => "وقف",
            Self::General => "تبرع عام",
        }
    }

    /// The restriction a fund must carry to receive this kind of money.
    /// `donor_designated` marks Sadaqah or general gifts given for a stated purpose.
    pub fn required_restriction(self, donor_designated: bool) -> FundRestriction {
        match self {
            Self::Waqf => FundRestriction::PermanentlyRestricted,
            Self::Zakat | Self::Kaffarah => FundRestriction::TemporarilyRestricted,
            Self::Sadaqah | Self::General if donor_designated => FundRestriction::TemporarilyRestricted,
            Self::Sadaqah | Self::General => FundRestriction::Unrestricted,
        }
    }

    /// Rejects placing this money in a fund of the wrong restriction class, e.g. Zakat
    /// into the general operating fund, where it could be spent outside the asnaf.
    pub fn check_fund(self, donor_designated: bool, fund: FundRestriction) -> Result<(), DomainError> {
        if self.required_restriction(donor_designated) == fund {
            Ok(())
        } else {
            Err(DomainError::RestrictionMismatch { kind: self.as_str(), restriction: fund.as_str() })
        }
    }

    /// Checks that money of this kind may be paid to a recipient in `category`.
    pub fn check_recipient(self, category: Asnaf) -> Result<(), DomainError> {
        let ok = match self {
            Self::Zakat => true,
            // Expiation is fed or clothed to the poor and needy only (Qur'an 5:89).
            Self::Kaffarah => matches!(category, Asnaf::Fuqara | Asnaf::Masakin),
            Self::Waqf => return Err(DomainError::EndowmentPrincipalSpend),
            Self::Sadaqah | Self::General => true,
        };
        if ok {
            Ok(())
        } else {
            Err(DomainError::IneligibleRecipient { kind: self.as_str(), category: category.as_str() })
        }
    }

    /// Classifies a donor's free-text note (Arabic or English). Returns `None`
    /// when the note names no type, so the cashier must pick one explicitly.
    pub fn classify_note(note: &str) -> Option<DonationKind> {
        let n = normalize_arabic(&note.to_lowercase());
        // Order matters: "زكاة الفطر" must stay Zakat, "صدقة جارية" leans to Waqf.
        const RULES: &[(&[&str], DonationKind)] = &[
            (&["كفاره", "كفارات", "kaffara", "kaffarah", "fidya", "فديه"], DonationKind::Kaffarah),
            (&["زكاه", "زكوات", "zakat", "zakah", "فطره", "fitr"], DonationKind::Zakat),
            (&["وقف", "اوقاف", "waqf", "awqaf", "endowment", "صدقه جاريه", "sadaqah jariyah"], DonationKind::Waqf),
            (&["صدقه", "صدقات", "sadaqah", "sadaqa", "sadaka", "charity"], DonationKind::Sadaqah),
        ];
        RULES
            .iter()
            .find(|(words, _)| words.iter().any(|w| n.contains(w)))
            .map(|(_, kind)| *kind)
    }
}

/// The eight Zakat recipient categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Asnaf {
    /// الفقراء
    Fuqara,
    /// المساكين
    Masakin,
    /// العاملين عليها — Zakat administrators.
    Amilin,
    /// المؤلفة قلوبهم
    Muallafah,
    /// في الرقاب
    Riqab,
    /// الغارمين — debtors.
    Gharimin,
    /// في سبيل الله
    FiSabilillah,
    /// ابن السبيل — stranded travellers.
    IbnSabil,
}

impl Asnaf {
    pub const ALL: [Asnaf; 8] = [
        Self::Fuqara,
        Self::Masakin,
        Self::Amilin,
        Self::Muallafah,
        Self::Riqab,
        Self::Gharimin,
        Self::FiSabilillah,
        Self::IbnSabil,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fuqara => "fuqara",
            Self::Masakin => "masakin",
            Self::Amilin => "amilin",
            Self::Muallafah => "muallafah",
            Self::Riqab => "riqab",
            Self::Gharimin => "gharimin",
            Self::FiSabilillah => "fi_sabilillah",
            Self::IbnSabil => "ibn_sabil",
        }
    }

    pub fn arabic(self) -> &'static str {
        match self {
            Self::Fuqara => "الفقراء",
            Self::Masakin => "المساكين",
            Self::Amilin => "العاملون عليها",
            Self::Muallafah => "المؤلفة قلوبهم",
            Self::Riqab => "في الرقاب",
            Self::Gharimin => "الغارمون",
            Self::FiSabilillah => "في سبيل الله",
            Self::IbnSabil => "ابن السبيل",
        }
    }
}

/// Folds Arabic spelling variants so keyword matching ignores diacritics,
/// hamza forms, taa marbuta and alef maqsura.
pub fn normalize_arabic(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{0640}'))
        .map(|c| match c {
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ة' => 'ه',
            'ى' => 'ي',
            'ؤ' => 'و',
            'ئ' => 'ي',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_arabic_and_english_notes() {
        assert_eq!(DonationKind::classify_note("زَكَاةُ المال لعام ١٤٤٦"), Some(DonationKind::Zakat));
        assert_eq!(DonationKind::classify_note("زكاة الفطر"), Some(DonationKind::Zakat));
        assert_eq!(DonationKind::classify_note("صدقة جارية عن والدي"), Some(DonationKind::Waqf));
        assert_eq!(DonationKind::classify_note("صدقة"), Some(DonationKind::Sadaqah));
        assert_eq!(DonationKind::classify_note("كفارة يمين"), Some(DonationKind::Kaffarah));
        assert_eq!(DonationKind::classify_note("My ZAKAT for 2025"), Some(DonationKind::Zakat));
        assert_eq!(DonationKind::classify_note("for the school"), None);
    }

    #[test]
    fn restriction_mapping() {
        use FundRestriction::*;
        assert!(DonationKind::Zakat.check_fund(false, TemporarilyRestricted).is_ok());
        assert!(DonationKind::Zakat.check_fund(false, Unrestricted).is_err());
        assert!(DonationKind::Waqf.check_fund(false, PermanentlyRestricted).is_ok());
        assert!(DonationKind::Sadaqah.check_fund(false, Unrestricted).is_ok());
        assert!(DonationKind::Sadaqah.check_fund(true, TemporarilyRestricted).is_ok());
    }

    #[test]
    fn recipient_eligibility() {
        assert!(Asnaf::ALL.iter().all(|a| DonationKind::Zakat.check_recipient(*a).is_ok()));
        assert!(DonationKind::Kaffarah.check_recipient(Asnaf::Masakin).is_ok());
        assert_eq!(
            DonationKind::Kaffarah.check_recipient(Asnaf::Gharimin).unwrap_err().code(),
            "ineligible_recipient"
        );
        assert_eq!(
            DonationKind::Waqf.check_recipient(Asnaf::Fuqara).unwrap_err(),
            DomainError::EndowmentPrincipalSpend
        );
    }
}
