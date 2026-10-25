//! Phase 2 signing: invoice hash, ECDSA secp256k1 signature, XAdES-BES block and QR.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::NaiveDateTime;
use k256::ecdsa::signature::{Signer, Verifier};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};
use k256::pkcs8::{DecodePrivateKey, EncodePublicKey};
use sha2::{Digest, Sha256};

use super::der::parse_certificate;
use super::invoice::{Envelope, Invoice, InvoiceType, money, render, text};
use super::tlv::{QrFields, encode_base64};
use crate::DomainError;

/// PIH of a device's first invoice: Base64 of the hex SHA-256 of "0", fixed by ZATCA.
pub const INITIAL_PIH: &str = "NWZlY2ViNjZmZmM4NmYzOGQ5NTI3ODZjNmQ2OTZjNzljMmRiYzIzOWRkNGU5MWI0NjcyOWQ3M2EyN2ZiNTdlOQ==";

/// The EGS unit's cryptographic identity, issued by ZATCA's onboarding (CSID) API.
pub struct Credentials {
    pub key: SigningKey,
    /// DER bytes of the X.509 CSID certificate.
    pub certificate_der: Vec<u8>,
}

impl Credentials {
    /// `key_pem` may be SEC1 (`BEGIN EC PRIVATE KEY`, what ZATCA's SDK generates) or PKCS#8.
    /// `certificate` is the Base64 body ZATCA returns as `binarySecurityToken`, decoded once,
    /// or a PEM certificate.
    pub fn from_pem(key_pem: &str, certificate: &str) -> Result<Self, DomainError> {
        let key = k256::SecretKey::from_sec1_pem(key_pem)
            .or_else(|_| k256::SecretKey::from_pkcs8_pem(key_pem))
            .map_err(|e| DomainError::Zatca(format!("private key: {e}")))?;
        let body: String = certificate
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect::<Vec<_>>()
            .concat();
        let certificate_der = STANDARD.decode(body.trim()).map_err(|e| DomainError::Zatca(format!("certificate base64: {e}")))?;
        Ok(Self { key: SigningKey::from(key), certificate_der })
    }

    pub fn public_key_der(&self) -> Result<Vec<u8>, DomainError> {
        Ok(self
            .key
            .verifying_key()
            .to_public_key_der()
            .map_err(|e| DomainError::Zatca(format!("public key: {e}")))?
            .as_bytes()
            .to_vec())
    }
}

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    match content.len() {
        n if n < 0x80 => out.push(n as u8),
        n if n < 0x100 => out.extend([0x81, n as u8]),
        n => out.extend([0x82, (n >> 8) as u8, n as u8]),
    }
    out.extend_from_slice(content);
    out
}

fn name_attr(oid: &[u8], string_tag: u8, value: &str) -> Vec<u8> {
    der(0x31, &der(0x30, &[der(0x06, oid), der(string_tag, value.as_bytes())].concat()))
}

impl Credentials {
    /// Self-signed credentials for local development and tests before ZATCA onboarding.
    /// ZATCA rejects them; production must use the CSID issued by the onboarding API.
    pub fn self_signed_for_development(key: SigningKey, common_name: &str) -> Self {
        let ecdsa_sha256 = der(0x30, &der(0x06, &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02]));
        let name = der(
            0x30,
            &[name_attr(&[0x55, 4, 6], 0x13, "SA"), name_attr(&[0x55, 4, 10], 0x0c, "Yusr"), name_attr(&[0x55, 4, 3], 0x0c, common_name)].concat(),
        );
        let validity = der(0x30, &[der(0x17, b"250101000000Z"), der(0x17, b"350101000000Z")].concat());
        let spki = key.verifying_key().to_public_key_der().expect("secp256k1 keys encode").as_bytes().to_vec();
        let tbs = der(
            0x30,
            &[
                der(0xa0, &der(0x02, &[2])),
                der(0x02, &[0x11, 0x22, 0x10, 0xf4, 0x7d, 0xe9, 0x81, 0x15]), // serial 1234567890123456789
                ecdsa_sha256.clone(),
                name.clone(),
                validity,
                name,
                spki,
            ]
            .concat(),
        );
        let sig: Signature = key.sign(&tbs);
        let cert = der(0x30, &[tbs, ecdsa_sha256, der(0x03, &[&[0u8][..], sig.to_der().as_bytes()].concat())].concat());
        Credentials { key, certificate_der: cert }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignedInvoice {
    pub xml: String,
    /// Base64 SHA-256 of the canonical invoice; becomes the next invoice's PIH.
    pub invoice_hash: String,
    pub signature: String,
    pub qr: String,
}

/// Invoice hash = Base64(SHA-256(canonical XML without UBLExtensions, Signature and QR)).
pub fn invoice_hash(inv: &Invoice) -> String {
    let canonical = render(inv, &inv.totals(), &Envelope { extensions: None, qr_b64: None, with_signature_ref: false });
    STANDARD.encode(Sha256::digest(canonical.as_bytes()))
}

fn signed_properties(signing_time: &str, cert_digest: &str, issuer: &str, serial: &str, with_ns: bool) -> String {
    let ns = if with_ns { " xmlns:xades=\"http://uri.etsi.org/01903/v1.3.2#\" xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\"" } else { "" };
    format!(
        "<xades:SignedProperties{ns} Id=\"xadesSignedProperties\"><xades:SignedSignatureProperties>\
<xades:SigningTime>{}</xades:SigningTime><xades:SigningCertificate><xades:Cert><xades:CertDigest>\
<ds:DigestMethod Algorithm=\"http://www.w3.org/2001/04/xmlenc#sha256\"></ds:DigestMethod><ds:DigestValue>{}</ds:DigestValue>\
</xades:CertDigest><xades:IssuerSerial><ds:X509IssuerName>{}</ds:X509IssuerName><ds:X509SerialNumber>{}</ds:X509SerialNumber>\
</xades:IssuerSerial></xades:Cert></xades:SigningCertificate></xades:SignedSignatureProperties></xades:SignedProperties>",
        text(signing_time),
        text(cert_digest),
        text(issuer),
        text(serial),
    )
}

/// Signs an invoice. `signing_time` is the local issue timestamp (KSA has no DST).
pub fn sign_invoice(inv: &Invoice, creds: &Credentials, signing_time: NaiveDateTime) -> Result<SignedInvoice, DomainError> {
    inv.validate()?;
    let totals = inv.totals();
    let hash_b64 = invoice_hash(inv);
    let hash_bytes = STANDARD.decode(&hash_b64).expect("we just encoded it");

    // ECDSA over SHA-256 of the invoice-hash bytes (SHA256withECDSA), DER-encoded.
    let sig: Signature = creds.key.sign(&hash_bytes);
    let signature_b64 = STANDARD.encode(sig.to_der().as_bytes());

    let cert = parse_certificate(&creds.certificate_der)?;
    let cert_b64 = STANDARD.encode(&creds.certificate_der);
    // ZATCA digests the certificate's Base64 text and encodes the hex digest in Base64.
    let cert_digest = STANDARD.encode(hex::encode(Sha256::digest(cert_b64.as_bytes())));
    let time = signing_time.format("%Y-%m-%dT%H:%M:%S").to_string();
    let props_for_digest = signed_properties(&time, &cert_digest, &cert.issuer, &cert.serial, true);
    let props_digest = STANDARD.encode(hex::encode(Sha256::digest(props_for_digest.as_bytes())));

    let extensions = format!(
        "<ext:UBLExtensions><ext:UBLExtension><ext:ExtensionURI>urn:oasis:names:specification:ubl:dsig:enveloped:xades</ext:ExtensionURI><ext:ExtensionContent>\
<sig:UBLDocumentSignatures xmlns:sac=\"urn:oasis:names:specification:ubl:schema:xsd:SignatureAggregateComponents-2\" \
xmlns:sbc=\"urn:oasis:names:specification:ubl:schema:xsd:SignatureBasicComponents-2\" \
xmlns:sig=\"urn:oasis:names:specification:ubl:schema:xsd:CommonSignatureComponents-2\"><sac:SignatureInformation>\
<cbc:ID>urn:oasis:names:specification:ubl:signature:1</cbc:ID><sbc:ReferencedSignatureID>urn:oasis:names:specification:ubl:signature:Invoice</sbc:ReferencedSignatureID>\
<ds:Signature xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\" Id=\"signature\"><ds:SignedInfo>\
<ds:CanonicalizationMethod Algorithm=\"http://www.w3.org/2006/12/xml-c14n11\"></ds:CanonicalizationMethod>\
<ds:SignatureMethod Algorithm=\"http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256\"></ds:SignatureMethod>\
<ds:Reference Id=\"invoiceSignedData\" URI=\"\"><ds:Transforms>\
<ds:Transform Algorithm=\"http://www.w3.org/TR/1999/REC-xpath-19991116\"><ds:XPath>not(//ancestor-or-self::ext:UBLExtensions)</ds:XPath></ds:Transform>\
<ds:Transform Algorithm=\"http://www.w3.org/TR/1999/REC-xpath-19991116\"><ds:XPath>not(//ancestor-or-self::cac:Signature)</ds:XPath></ds:Transform>\
<ds:Transform Algorithm=\"http://www.w3.org/TR/1999/REC-xpath-19991116\"><ds:XPath>not(//ancestor-or-self::cac:AdditionalDocumentReference[cbc:ID='QR'])</ds:XPath></ds:Transform>\
<ds:Transform Algorithm=\"http://www.w3.org/2006/12/xml-c14n11\"></ds:Transform></ds:Transforms>\
<ds:DigestMethod Algorithm=\"http://www.w3.org/2001/04/xmlenc#sha256\"></ds:DigestMethod><ds:DigestValue>{hash_b64}</ds:DigestValue></ds:Reference>\
<ds:Reference Type=\"http://www.w3.org/2000/09/xmldsig#SignatureProperties\" URI=\"#xadesSignedProperties\">\
<ds:DigestMethod Algorithm=\"http://www.w3.org/2001/04/xmlenc#sha256\"></ds:DigestMethod><ds:DigestValue>{props_digest}</ds:DigestValue></ds:Reference>\
</ds:SignedInfo><ds:SignatureValue>{signature_b64}</ds:SignatureValue><ds:KeyInfo><ds:X509Data><ds:X509Certificate>{cert_b64}</ds:X509Certificate></ds:X509Data></ds:KeyInfo>\
<ds:Object><xades:QualifyingProperties xmlns:xades=\"http://uri.etsi.org/01903/v1.3.2#\" Target=\"signature\">{}</xades:QualifyingProperties></ds:Object>\
</ds:Signature></sac:SignatureInformation></sig:UBLDocumentSignatures></ext:ExtensionContent></ext:UBLExtension></ext:UBLExtensions>",
        signed_properties(&time, &cert_digest, &cert.issuer, &cert.serial, false),
    );

    let simplified = inv.invoice_type == InvoiceType::Simplified;
    let qr = encode_base64(&QrFields {
        seller_name: inv.seller.registration_name.clone(),
        vat_number: inv.seller.vat_number.clone().unwrap_or_default(),
        timestamp: format!("{}T{}", inv.issue_date.format("%Y-%m-%d"), inv.issue_time.format("%H:%M:%S")),
        total_with_vat: money(totals.tax_inclusive),
        vat_total: money(totals.vat),
        invoice_hash_b64: Some(hash_b64.clone()),
        signature_b64: Some(signature_b64.clone()),
        public_key_der: Some(creds.public_key_der()?),
        certificate_signature: simplified.then(|| cert.signature.clone()),
    })?;

    let body = render(inv, &totals, &Envelope { extensions: Some(&extensions), qr_b64: Some(&qr), with_signature_ref: true });
    Ok(SignedInvoice { xml: format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{body}"), invoice_hash: hash_b64, signature: signature_b64, qr })
}

/// Verifies an invoice signature against the signer's public key (SubjectPublicKeyInfo DER).
pub fn verify_signature(invoice_hash_b64: &str, signature_b64: &str, public_key_der: &[u8]) -> Result<(), DomainError> {
    use k256::pkcs8::DecodePublicKey;
    let key = VerifyingKey::from_public_key_der(public_key_der).map_err(|e| DomainError::Zatca(format!("public key: {e}")))?;
    let hash = STANDARD.decode(invoice_hash_b64).map_err(|e| DomainError::Zatca(format!("hash: {e}")))?;
    let sig_der = STANDARD.decode(signature_b64).map_err(|e| DomainError::Zatca(format!("signature: {e}")))?;
    let sig = Signature::from_der(&sig_der).map_err(|e| DomainError::Zatca(format!("signature: {e}")))?;
    key.verify(&hash, &sig).map_err(|_| DomainError::Zatca("signature does not verify".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zatca::invoice::tests::sample;
    use crate::zatca::tlv::decode_base64;

    fn creds() -> Credentials {
        Credentials::self_signed_for_development(SigningKey::from_bytes(&[7u8; 32].into()).unwrap(), "Yusr Test CA")
    }

    #[test]
    fn parses_certificate_fields() {
        let info = parse_certificate(&creds().certificate_der).unwrap();
        assert_eq!(info.issuer, "CN=Yusr Test CA, O=Yusr, C=SA");
        assert_eq!(info.serial, "1234567890123456789");
        assert!(Signature::from_der(&info.signature).is_ok());
    }

    #[test]
    fn loads_pem_credentials() {
        use k256::pkcs8::EncodePrivateKey;
        let c = creds();
        let pem = k256::SecretKey::from(&c.key).to_pkcs8_pem(Default::default()).unwrap();
        let cert_b64 = STANDARD.encode(&c.certificate_der);
        let loaded = Credentials::from_pem(&pem, &format!("-----BEGIN CERTIFICATE-----\n{cert_b64}\n-----END CERTIFICATE-----\n")).unwrap();
        assert_eq!(loaded.certificate_der, c.certificate_der);
        assert_eq!(loaded.public_key_der().unwrap(), c.public_key_der().unwrap());
    }

    #[test]
    fn initial_pih_is_hex_sha256_of_zero() {
        assert_eq!(STANDARD.encode(hex::encode(Sha256::digest(b"0"))), INITIAL_PIH);
    }

    #[test]
    fn signs_and_verifies_simplified_invoice() {
        let inv = sample();
        let c = creds();
        let t = NaiveDateTime::new(inv.issue_date, inv.issue_time);
        let s = sign_invoice(&inv, &c, t).unwrap();

        verify_signature(&s.invoice_hash, &s.signature, &c.public_key_der().unwrap()).unwrap();
        assert_eq!(s.invoice_hash, invoice_hash(&inv));
        assert!(s.xml.contains(&format!("<ds:DigestValue>{}</ds:DigestValue>", s.invoice_hash)));
        assert!(s.xml.contains("<cbc:ID>QR</cbc:ID>"));
        assert!(s.xml.contains("<ds:X509IssuerName>CN=Yusr Test CA, O=Yusr, C=SA</ds:X509IssuerName>"));

        let tags = decode_base64(&s.qr).unwrap();
        let tag = |n: u8| tags.iter().find(|(t, _)| *t == n).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(String::from_utf8(tag(1)).unwrap(), "جمعية البر الخيرية");
        assert_eq!(String::from_utf8(tag(4)).unwrap(), "126.56");
        assert_eq!(String::from_utf8(tag(5)).unwrap(), "16.51");
        assert_eq!(String::from_utf8(tag(6)).unwrap(), s.invoice_hash);
        assert_eq!(tag(8), c.public_key_der().unwrap());
        assert!(!tag(9).is_empty());
    }

    #[test]
    fn hash_changes_with_content_and_chains() {
        let a = sample();
        let mut b = sample();
        b.lines[0].quantity = rust_decimal::Decimal::from(4);
        assert_ne!(invoice_hash(&a), invoice_hash(&b));

        let first = sign_invoice(&a, &creds(), NaiveDateTime::new(a.issue_date, a.issue_time)).unwrap();
        let mut next = sample();
        next.counter = 2;
        next.previous_hash = first.invoice_hash.clone();
        let second = sign_invoice(&next, &creds(), NaiveDateTime::new(next.issue_date, next.issue_time)).unwrap();
        assert!(second.xml.contains(&first.invoice_hash));
    }

    #[test]
    fn tampered_hash_fails_verification() {
        let inv = sample();
        let c = creds();
        let s = sign_invoice(&inv, &c, NaiveDateTime::new(inv.issue_date, inv.issue_time)).unwrap();
        let mut other = sample();
        other.number = "INV-2".into();
        assert!(verify_signature(&invoice_hash(&other), &s.signature, &c.public_key_der().unwrap()).is_err());
    }
}
