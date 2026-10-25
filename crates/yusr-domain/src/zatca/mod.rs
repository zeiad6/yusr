//! ZATCA (هيئة الزكاة والضريبة والجمارك) e-invoicing, Phase 2 ("integration").
//!
//! - [`tlv`]: the QR payload (tags 1–9), Base64-encoded.
//! - [`invoice`]: UBL 2.1 invoice model, KSA rounding rules and canonical XML.
//! - [`sign`]: invoice hash, ECDSA secp256k1 signature, XAdES block, ICV/PIH chaining.
//!
//! The XML is emitted directly in canonical (C14N 1.1) form — no indentation, no empty
//! element shorthand, sorted namespace declarations — so the bytes we hash are the
//! bytes any conforming canonicaliser produces after ZATCA's three XPath exclusions.

pub mod der;
pub mod invoice;
pub mod sign;
pub mod tlv;

use crate::DomainError;

/// KSA VAT registration numbers are 15 digits, starting and ending with 3.
pub fn validate_vat_number(vat: &str) -> Result<(), DomainError> {
    let ok = vat.len() == 15 && vat.bytes().all(|b| b.is_ascii_digit()) && vat.starts_with('3') && vat.ends_with('3');
    if ok { Ok(()) } else { Err(DomainError::InvalidVatNumber(vat.to_owned())) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn vat_numbers() {
        assert!(super::validate_vat_number("300000000000003").is_ok());
        assert!(super::validate_vat_number("310000000000002").is_err());
        assert!(super::validate_vat_number("30000000000003").is_err());
    }
}
