//! Tag-Length-Value QR payload. Phase 1 uses tags 1–5; Phase 2 simplified invoices add
//! the invoice hash (6), the ECDSA signature (7), the public key (8) and, for simplified
//! invoices, the CSID certificate's signature (9).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrFields {
    pub seller_name: String,
    pub vat_number: String,
    /// ISO 8601 issue timestamp, e.g. `2025-03-01T10:15:00`.
    pub timestamp: String,
    /// Invoice total including VAT, two decimals.
    pub total_with_vat: String,
    pub vat_total: String,
    /// Phase 2 fields; empty for Phase 1 codes.
    pub invoice_hash_b64: Option<String>,
    pub signature_b64: Option<String>,
    pub public_key_der: Option<Vec<u8>>,
    pub certificate_signature: Option<Vec<u8>>,
}

fn push(out: &mut Vec<u8>, tag: u8, value: &[u8]) -> Result<(), DomainError> {
    let len = u8::try_from(value.len()).map_err(|_| DomainError::Zatca(format!("QR tag {tag} longer than 255 bytes")))?;
    out.push(tag);
    out.push(len);
    out.extend_from_slice(value);
    Ok(())
}

pub fn encode(f: &QrFields) -> Result<Vec<u8>, DomainError> {
    let mut out = Vec::with_capacity(512);
    push(&mut out, 1, f.seller_name.as_bytes())?;
    push(&mut out, 2, f.vat_number.as_bytes())?;
    push(&mut out, 3, f.timestamp.as_bytes())?;
    push(&mut out, 4, f.total_with_vat.as_bytes())?;
    push(&mut out, 5, f.vat_total.as_bytes())?;
    if let Some(v) = &f.invoice_hash_b64 {
        push(&mut out, 6, v.as_bytes())?;
    }
    if let Some(v) = &f.signature_b64 {
        push(&mut out, 7, v.as_bytes())?;
    }
    if let Some(v) = &f.public_key_der {
        push(&mut out, 8, v)?;
    }
    if let Some(v) = &f.certificate_signature {
        push(&mut out, 9, v)?;
    }
    Ok(out)
}

pub fn encode_base64(f: &QrFields) -> Result<String, DomainError> {
    Ok(STANDARD.encode(encode(f)?))
}

/// Decodes a Base64 TLV payload into (tag, value) pairs — used by tests and by the
/// receipt-verification screen.
pub fn decode_base64(s: &str) -> Result<Vec<(u8, Vec<u8>)>, DomainError> {
    let bytes = STANDARD.decode(s.trim()).map_err(|e| DomainError::Zatca(format!("QR base64: {e}")))?;
    let mut out = Vec::new();
    let mut rest = bytes.as_slice();
    while let [tag, len, tail @ ..] = rest {
        let len = usize::from(*len);
        if tail.len() < len {
            return Err(DomainError::Zatca("QR TLV truncated".into()));
        }
        out.push((*tag, tail[..len].to_vec()));
        rest = &tail[len..];
    }
    if !rest.is_empty() {
        return Err(DomainError::Zatca("QR TLV trailing byte".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_matches_zatca_reference_vector() {
        // Example from ZATCA's "QR code creation" guideline.
        let f = QrFields {
            seller_name: "Bobs Records".into(),
            vat_number: "310122393500003".into(),
            timestamp: "2022-04-25T15:30:00Z".into(),
            total_with_vat: "1000.00".into(),
            vat_total: "150.00".into(),
            invoice_hash_b64: None,
            signature_b64: None,
            public_key_der: None,
            certificate_signature: None,
        };
        assert_eq!(
            encode_base64(&f).unwrap(),
            "AQxCb2JzIFJlY29yZHMCDzMxMDEyMjM5MzUwMDAwMwMUMjAyMi0wNC0yNVQxNTozMDowMFoEBzEwMDAuMDAFBjE1MC4wMA=="
        );
    }

    #[test]
    fn arabic_names_count_bytes_not_chars() {
        let f = QrFields {
            seller_name: "جمعية البر".into(),
            vat_number: "300000000000003".into(),
            timestamp: "2025-03-01T10:00:00".into(),
            total_with_vat: "115.00".into(),
            vat_total: "15.00".into(),
            invoice_hash_b64: Some("aGFzaA==".into()),
            signature_b64: Some("c2ln".into()),
            public_key_der: Some(vec![1, 2, 3]),
            certificate_signature: Some(vec![4, 5]),
        };
        let back = decode_base64(&encode_base64(&f).unwrap()).unwrap();
        assert_eq!(back[0].1.len(), "جمعية البر".len());
        assert_eq!(String::from_utf8(back[0].1.clone()).unwrap(), "جمعية البر");
        assert_eq!(back.iter().map(|(t, _)| *t).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
