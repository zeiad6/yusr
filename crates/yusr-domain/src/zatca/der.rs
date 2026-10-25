//! Just enough DER to read what ZATCA's XAdES block needs from the CSID certificate:
//! issuer name, serial number and the certificate's own signature.

use crate::DomainError;

fn err(msg: &str) -> DomainError {
    DomainError::Zatca(format!("certificate: {msg}"))
}

/// One TLV element: (tag, content, rest-of-input).
fn read(input: &[u8]) -> Result<(u8, &[u8], &[u8]), DomainError> {
    let (&tag, rest) = input.split_first().ok_or_else(|| err("truncated tag"))?;
    let (&first, mut rest) = rest.split_first().ok_or_else(|| err("truncated length"))?;
    let len = if first < 0x80 {
        usize::from(first)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 || rest.len() < n {
            return Err(err("bad length"));
        }
        let len = rest[..n].iter().fold(0usize, |acc, b| (acc << 8) | usize::from(*b));
        rest = &rest[n..];
        len
    };
    if rest.len() < len {
        return Err(err("truncated content"));
    }
    Ok((tag, &rest[..len], &rest[len..]))
}

fn children(mut content: &[u8]) -> Result<Vec<(u8, &[u8])>, DomainError> {
    let mut out = Vec::new();
    while !content.is_empty() {
        let (tag, value, rest) = read(content)?;
        out.push((tag, value));
        content = rest;
    }
    Ok(out)
}

fn oid_to_string(bytes: &[u8]) -> String {
    let mut parts = Vec::new();
    if let Some(&first) = bytes.first() {
        parts.push(u64::from(first / 40));
        parts.push(u64::from(first % 40));
    }
    let mut acc = 0u64;
    for &b in bytes.iter().skip(1) {
        acc = (acc << 7) | u64::from(b & 0x7f);
        if b & 0x80 == 0 {
            parts.push(acc);
            acc = 0;
        }
    }
    parts.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
}

fn attribute_name(oid: &str) -> String {
    match oid {
        "2.5.4.3" => "CN".into(),
        "2.5.4.6" => "C".into(),
        "2.5.4.7" => "L".into(),
        "2.5.4.8" => "ST".into(),
        "2.5.4.10" => "O".into(),
        "2.5.4.11" => "OU".into(),
        "0.9.2342.19200300.100.1.25" => "DC".into(),
        other => other.into(),
    }
}

/// Big-endian unsigned integer bytes to a decimal string.
fn to_decimal(bytes: &[u8]) -> String {
    let mut digits = vec![0u8]; // little-endian base-10 digits
    for &b in bytes {
        let mut carry = u32::from(b);
        for d in digits.iter_mut() {
            let v = u32::from(*d) * 256 + carry;
            *d = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    let s: String = digits.iter().rev().map(|d| char::from(b'0' + d)).collect();
    let t = s.trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.into() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertInfo {
    /// RFC 4514 string, most specific RDN first, e.g. `CN=TSZEINVOICE-SubCA-1, DC=extgazt, DC=gov, DC=local`.
    pub issuer: String,
    /// Serial number in decimal, as XAdES `X509SerialNumber` requires.
    pub serial: String,
    /// The CA's signature over the certificate (QR tag 9).
    pub signature: Vec<u8>,
}

pub fn parse_certificate(der: &[u8]) -> Result<CertInfo, DomainError> {
    let (tag, cert, _) = read(der)?;
    if tag != 0x30 {
        return Err(err("not a SEQUENCE"));
    }
    let parts = children(cert)?;
    let [(0x30, tbs), (0x30, _alg), (0x03, sig)] = parts.as_slice() else {
        return Err(err("unexpected Certificate structure"));
    };
    let signature = sig.get(1..).ok_or_else(|| err("empty signature"))?.to_vec();

    let mut fields = children(tbs)?.into_iter();
    let mut next = fields.next().ok_or_else(|| err("empty TBSCertificate"))?;
    if next.0 == 0xa0 {
        next = fields.next().ok_or_else(|| err("missing serial"))?; // skip [0] version
    }
    if next.0 != 0x02 {
        return Err(err("missing serial"));
    }
    let serial = to_decimal(next.1);
    let _sig_alg = fields.next().ok_or_else(|| err("missing algorithm"))?;
    let (issuer_tag, issuer) = fields.next().ok_or_else(|| err("missing issuer"))?;
    if issuer_tag != 0x30 {
        return Err(err("bad issuer"));
    }

    let mut rdns = Vec::new();
    for (set_tag, set) in children(issuer)? {
        if set_tag != 0x31 {
            return Err(err("bad RDN"));
        }
        for (_, atv) in children(set)? {
            let kv = children(atv)?;
            let [(0x06, oid), (_, value)] = kv.as_slice() else {
                return Err(err("bad attribute"));
            };
            let value = String::from_utf8_lossy(value);
            rdns.push(format!("{}={}", attribute_name(&oid_to_string(oid)), value));
        }
    }
    rdns.reverse();
    Ok(CertInfo { issuer: rdns.join(", "), serial, signature })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_conversion() {
        assert_eq!(to_decimal(&[0x00]), "0");
        assert_eq!(to_decimal(&[0x01, 0x00]), "256");
        assert_eq!(to_decimal(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]), "18446744073709551615");
    }

    #[test]
    fn oid_decoding() {
        assert_eq!(oid_to_string(&[0x55, 0x04, 0x03]), "2.5.4.3");
        assert_eq!(oid_to_string(&[0x09, 0x92, 0x26, 0x89, 0x93, 0xf2, 0x2c, 0x64, 0x01, 0x19]), "0.9.2342.19200300.100.1.25");
    }
}
