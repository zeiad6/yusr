//! Digital aid vouchers that partner shops can verify offline.
//!
//! A code is 10 random bytes plus a 5-byte HMAC-SHA256 tag keyed with the charity's
//! voucher secret, Crockford-base32 encoded in groups of four. Shops check the tag
//! without a network round-trip; redemption is still recorded centrally so a code
//! cannot be spent twice.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::DomainError;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const BODY: usize = 10;
const TAG: usize = 5;

fn tag(secret: &[u8], body: &[u8]) -> [u8; TAG] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(b"yusr-voucher-v1");
    mac.update(body);
    let full = mac.finalize().into_bytes();
    let mut out = [0u8; TAG];
    out.copy_from_slice(&full[..TAG]);
    out
}

fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).expect("ASCII")).collect::<Vec<_>>().join("-")
}

fn decode(code: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in code.chars().filter(|c| *c != '-' && !c.is_whitespace()) {
        // Crockford folds look-alike letters.
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        let v = ALPHABET.iter().position(|&a| a as char == c)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Issues a voucher code from 10 bytes of OS randomness supplied by the caller.
pub fn issue(secret: &[u8], random: [u8; BODY]) -> String {
    let mut bytes = random.to_vec();
    bytes.extend_from_slice(&tag(secret, &random));
    encode(&bytes)
}

/// Verifies a code's authenticity and returns its canonical form (the database key).
pub fn verify(secret: &[u8], code: &str) -> Result<String, DomainError> {
    let bytes = decode(code).ok_or(DomainError::InvalidVoucher)?;
    if bytes.len() != BODY + TAG {
        return Err(DomainError::InvalidVoucher);
    }
    let (body, given) = bytes.split_at(BODY);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(b"yusr-voucher-v1");
    mac.update(body);
    // Constant-time comparison of the truncated tag.
    mac.verify_truncated_left(given).map_err(|_| DomainError::InvalidVoucher)?;
    Ok(encode(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify() {
        let code = issue(b"secret", [7; 10]);
        assert_eq!(code.len(), 24 + 5, "{code}");
        assert_eq!(verify(b"secret", &code).unwrap(), code);
        assert_eq!(verify(b"secret", &code.to_lowercase().replace('-', "")).unwrap(), code);
    }

    #[test]
    fn rejects_forgery_and_typos() {
        let code = issue(b"secret", [9; 10]);
        assert!(verify(b"other", &code).is_err());
        let mut chars: Vec<char> = code.chars().collect();
        chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
        assert!(verify(b"secret", &chars.into_iter().collect::<String>()).is_err());
        assert!(verify(b"secret", "U!!!").is_err());
    }
}
