//! UBL 2.1 invoice model with KSA rules (BR-KSA) for totals and rounding.

use chrono::{NaiveDate, NaiveTime};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::validate_vat_number;
use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceType {
    /// B2B — cleared by ZATCA before it is shared with the buyer.
    Standard,
    /// B2C — reported to ZATCA within 24 hours; carries the full Phase 2 QR.
    Simplified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Invoice,
    DebitNote,
    CreditNote,
}

impl DocumentKind {
    /// UNTDID 1001 code.
    fn code(self) -> &'static str {
        match self {
            Self::Invoice => "388",
            Self::DebitNote => "383",
            Self::CreditNote => "381",
        }
    }
}

/// UN/ECE 5305 VAT category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VatCategory {
    /// Standard rate.
    S,
    /// Zero rated.
    Z,
    /// Exempt.
    E,
    /// Outside the scope of VAT — e.g. a pure donation with nothing given in return.
    O,
}

impl VatCategory {
    fn code(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::Z => "Z",
            Self::E => "E",
            Self::O => "O",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Address {
    pub street: String,
    pub building_number: String,
    pub district: String,
    pub city: String,
    pub postal_zone: String,
    /// ISO 3166-1 alpha-2.
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Party {
    pub registration_name: String,
    pub vat_number: Option<String>,
    /// Other ID (e.g. commercial registration `CRN`) as (scheme, value).
    pub other_id: Option<(String, String)>,
    pub address: Address,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub name: String,
    pub quantity: Decimal,
    /// Unit price excluding VAT.
    pub unit_price: Decimal,
    pub vat_category: VatCategory,
    /// Percent, e.g. 15.
    pub vat_percent: Decimal,
    /// Required for categories Z, E and O (VATEX code and text).
    #[serde(default)]
    pub exemption: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invoice {
    pub number: String,
    pub uuid: Uuid,
    pub issue_date: NaiveDate,
    pub issue_time: NaiveTime,
    pub invoice_type: InvoiceType,
    pub kind: DocumentKind,
    /// Original invoice number, required for debit and credit notes.
    pub billing_reference: Option<String>,
    /// Reason for issuing a debit or credit note (KSA-10).
    pub note_reason: Option<String>,
    /// Invoice counter value (ICV): strictly increasing per device.
    pub counter: u64,
    /// Previous invoice hash (PIH), Base64.
    pub previous_hash: String,
    pub seller: Party,
    pub buyer: Option<Party>,
    pub currency: String,
    /// UNTDID 4461 payment means, e.g. `10` cash, `48` card, `30` transfer.
    pub payment_means: String,
    pub lines: Vec<InvoiceLine>,
}

fn r2(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineTotals {
    pub net: Decimal,
    pub vat: Decimal,
    pub gross: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Subtotal {
    pub category: VatCategory,
    pub percent: Decimal,
    pub taxable: Decimal,
    pub vat: Decimal,
    pub exemption: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Totals {
    pub lines: Vec<LineTotals>,
    pub subtotals: Vec<Subtotal>,
    pub line_extension: Decimal,
    pub vat: Decimal,
    pub tax_inclusive: Decimal,
}

impl Invoice {
    pub fn validate(&self) -> Result<(), DomainError> {
        let z = |m: &str| Err(DomainError::Zatca(m.into()));
        if self.lines.is_empty() {
            return z("an invoice needs at least one line");
        }
        if let Some(v) = &self.seller.vat_number {
            validate_vat_number(v)?;
        } else {
            return z("seller VAT number is required");
        }
        if self.invoice_type == InvoiceType::Standard && self.buyer.is_none() {
            return z("standard (B2B) invoices need a buyer");
        }
        if self.kind != DocumentKind::Invoice && (self.billing_reference.is_none() || self.note_reason.is_none()) {
            return z("debit/credit notes need the original invoice number and a reason");
        }
        if self.counter == 0 {
            return z("invoice counter starts at 1");
        }
        for (i, l) in self.lines.iter().enumerate() {
            if l.quantity <= Decimal::ZERO || l.unit_price < Decimal::ZERO {
                return Err(DomainError::Zatca(format!("line {}: quantity must be positive and price non-negative", i + 1)));
            }
            match l.vat_category {
                VatCategory::S if l.vat_percent <= Decimal::ZERO => {
                    return Err(DomainError::Zatca(format!("line {}: standard-rated line needs a VAT rate", i + 1)));
                }
                VatCategory::Z | VatCategory::E | VatCategory::O if !l.vat_percent.is_zero() || l.exemption.is_none() => {
                    return Err(DomainError::Zatca(format!("line {}: zero/exempt/out-of-scope lines need 0 % and an exemption reason", i + 1)));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Totals per BR-KSA: line amounts rounded to 2 dp, VAT per category computed on the
    /// category's taxable total (not summed from line VAT) and rounded to 2 dp.
    pub fn totals(&self) -> Totals {
        let lines: Vec<LineTotals> = self
            .lines
            .iter()
            .map(|l| {
                let net = r2(l.quantity * l.unit_price);
                let vat = r2(net * l.vat_percent / Decimal::ONE_HUNDRED);
                LineTotals { net, vat, gross: net + vat }
            })
            .collect();

        let mut subtotals: Vec<Subtotal> = Vec::new();
        for (l, t) in self.lines.iter().zip(&lines) {
            match subtotals.iter_mut().find(|s| s.category == l.vat_category && s.percent == l.vat_percent) {
                Some(s) => s.taxable += t.net,
                None => subtotals.push(Subtotal {
                    category: l.vat_category,
                    percent: l.vat_percent,
                    taxable: t.net,
                    vat: Decimal::ZERO,
                    exemption: l.exemption.clone(),
                }),
            }
        }
        for s in &mut subtotals {
            s.vat = r2(s.taxable * s.percent / Decimal::ONE_HUNDRED);
        }
        let line_extension: Decimal = lines.iter().map(|l| l.net).sum();
        let vat: Decimal = subtotals.iter().map(|s| s.vat).sum();
        Totals { lines, subtotals, line_extension, vat, tax_inclusive: line_extension + vat }
    }
}

/// Text-node escaping per C14N.
pub(crate) fn text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\r', "&#xD;")
}

/// Attribute-value escaping per C14N.
pub(crate) fn attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace('\t', "&#x9;")
        .replace('\n', "&#xA;")
        .replace('\r', "&#xD;")
}

fn el(name: &str, value: &str) -> String {
    format!("<{name}>{}</{name}>", text(value))
}

fn amount(name: &str, cur: &str, v: Decimal) -> String {
    format!("<{name} currencyID=\"{}\">{}</{name}>", attr(cur), v.round_dp(2).to_string_fixed2())
}

trait Fixed2 {
    fn to_string_fixed2(&self) -> String;
}

impl Fixed2 for Decimal {
    fn to_string_fixed2(&self) -> String {
        format!("{:.2}", self)
    }
}

pub(crate) fn money(v: Decimal) -> String {
    v.to_string_fixed2()
}

fn party(p: &Party) -> String {
    let id = p
        .other_id
        .as_ref()
        .map(|(scheme, v)| format!("<cac:PartyIdentification><cbc:ID schemeID=\"{}\">{}</cbc:ID></cac:PartyIdentification>", attr(scheme), text(v)))
        .unwrap_or_default();
    let a = &p.address;
    let tax = p
        .vat_number
        .as_ref()
        .map(|v| format!("<cac:PartyTaxScheme>{}<cac:TaxScheme><cbc:ID>VAT</cbc:ID></cac:TaxScheme></cac:PartyTaxScheme>", el("cbc:CompanyID", v)))
        .unwrap_or_default();
    format!(
        "<cac:Party>{id}<cac:PostalAddress>{}{}{}{}{}<cac:Country>{}</cac:Country></cac:PostalAddress>{tax}<cac:PartyLegalEntity>{}</cac:PartyLegalEntity></cac:Party>",
        el("cbc:StreetName", &a.street),
        el("cbc:BuildingNumber", &a.building_number),
        el("cbc:CitySubdivisionName", &a.district),
        el("cbc:CityName", &a.city),
        el("cbc:PostalZone", &a.postal_zone),
        el("cbc:IdentificationCode", &a.country),
        el("cbc:RegistrationName", &p.registration_name),
    )
}

fn tax_category(tag: &str, category: VatCategory, percent: Decimal, exemption: Option<&(String, String)>) -> String {
    let ex = exemption
        .map(|(code, reason)| format!("{}{}", el("cbc:TaxExemptionReasonCode", code), el("cbc:TaxExemptionReason", reason)))
        .unwrap_or_default();
    format!(
        "<cac:{tag}><cbc:ID schemeAgencyID=\"6\" schemeID=\"UN/ECE 5305\">{}</cbc:ID><cbc:Percent>{}</cbc:Percent>{ex}<cac:TaxScheme><cbc:ID schemeAgencyID=\"6\" schemeID=\"UN/ECE 5153\">VAT</cbc:ID></cac:TaxScheme></cac:{tag}>",
        category.code(),
        money(percent),
    )
}

pub(crate) const NS: &str = "xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\" \
xmlns:cac=\"urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2\" \
xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\" \
xmlns:ext=\"urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2\"";

/// Pieces that differ between the hashed form and the final signed document.
pub(crate) struct Envelope<'a> {
    pub extensions: Option<&'a str>,
    pub qr_b64: Option<&'a str>,
    pub with_signature_ref: bool,
}

/// Renders the invoice. With an empty envelope this is exactly the canonical form that
/// ZATCA hashes (UBLExtensions, the QR reference and cac:Signature removed).
pub(crate) fn render(inv: &Invoice, totals: &Totals, env: &Envelope<'_>) -> String {
    let cur = inv.currency.as_str();
    let type_name = match inv.invoice_type {
        InvoiceType::Standard => "0100000",
        InvoiceType::Simplified => "0200000",
    };
    let mut x = String::with_capacity(8 * 1024);
    x.push_str(&format!("<Invoice {NS}>"));
    if let Some(ext) = env.extensions {
        x.push_str(ext);
    }
    x.push_str(&el("cbc:ProfileID", "reporting:1.0"));
    x.push_str(&el("cbc:ID", &inv.number));
    x.push_str(&el("cbc:UUID", &inv.uuid.to_string()));
    x.push_str(&el("cbc:IssueDate", &inv.issue_date.format("%Y-%m-%d").to_string()));
    x.push_str(&el("cbc:IssueTime", &inv.issue_time.format("%H:%M:%S").to_string()));
    x.push_str(&format!("<cbc:InvoiceTypeCode name=\"{type_name}\">{}</cbc:InvoiceTypeCode>", inv.kind.code()));
    x.push_str(&el("cbc:DocumentCurrencyCode", cur));
    x.push_str(&el("cbc:TaxCurrencyCode", "SAR"));
    if let Some(r) = &inv.billing_reference {
        x.push_str(&format!("<cac:BillingReference><cac:InvoiceDocumentReference>{}</cac:InvoiceDocumentReference></cac:BillingReference>", el("cbc:ID", r)));
    }
    x.push_str(&format!("<cac:AdditionalDocumentReference>{}{}</cac:AdditionalDocumentReference>", el("cbc:ID", "ICV"), el("cbc:UUID", &inv.counter.to_string())));
    x.push_str(&format!(
        "<cac:AdditionalDocumentReference>{}<cac:Attachment><cbc:EmbeddedDocumentBinaryObject mimeCode=\"text/plain\">{}</cbc:EmbeddedDocumentBinaryObject></cac:Attachment></cac:AdditionalDocumentReference>",
        el("cbc:ID", "PIH"),
        text(&inv.previous_hash)
    ));
    if let Some(qr) = env.qr_b64 {
        x.push_str(&format!(
            "<cac:AdditionalDocumentReference>{}<cac:Attachment><cbc:EmbeddedDocumentBinaryObject mimeCode=\"text/plain\">{}</cbc:EmbeddedDocumentBinaryObject></cac:Attachment></cac:AdditionalDocumentReference>",
            el("cbc:ID", "QR"),
            text(qr)
        ));
    }
    if env.with_signature_ref {
        x.push_str("<cac:Signature><cbc:ID>urn:oasis:names:specification:ubl:signature:Invoice</cbc:ID><cbc:SignatureMethod>urn:oasis:names:specification:ubl:dsig:enveloped:xades</cbc:SignatureMethod></cac:Signature>");
    }
    x.push_str(&format!("<cac:AccountingSupplierParty>{}</cac:AccountingSupplierParty>", party(&inv.seller)));
    match &inv.buyer {
        Some(b) => x.push_str(&format!("<cac:AccountingCustomerParty>{}</cac:AccountingCustomerParty>", party(b))),
        None => x.push_str("<cac:AccountingCustomerParty></cac:AccountingCustomerParty>"),
    }
    if inv.invoice_type == InvoiceType::Standard {
        x.push_str(&format!("<cac:Delivery>{}</cac:Delivery>", el("cbc:ActualDeliveryDate", &inv.issue_date.format("%Y-%m-%d").to_string())));
    }
    x.push_str("<cac:PaymentMeans>");
    x.push_str(&el("cbc:PaymentMeansCode", &inv.payment_means));
    if let Some(reason) = &inv.note_reason {
        x.push_str(&el("cbc:InstructionNote", reason));
    }
    x.push_str("</cac:PaymentMeans>");
    // KSA requires two TaxTotal blocks: one in the tax currency, one with the breakdown.
    x.push_str(&format!("<cac:TaxTotal>{}</cac:TaxTotal>", amount("cbc:TaxAmount", "SAR", totals.vat)));
    x.push_str("<cac:TaxTotal>");
    x.push_str(&amount("cbc:TaxAmount", cur, totals.vat));
    for s in &totals.subtotals {
        x.push_str(&format!(
            "<cac:TaxSubtotal>{}{}{}</cac:TaxSubtotal>",
            amount("cbc:TaxableAmount", cur, s.taxable),
            amount("cbc:TaxAmount", cur, s.vat),
            tax_category("TaxCategory", s.category, s.percent, s.exemption.as_ref()),
        ));
    }
    x.push_str("</cac:TaxTotal>");
    x.push_str(&format!(
        "<cac:LegalMonetaryTotal>{}{}{}{}{}{}</cac:LegalMonetaryTotal>",
        amount("cbc:LineExtensionAmount", cur, totals.line_extension),
        amount("cbc:TaxExclusiveAmount", cur, totals.line_extension),
        amount("cbc:TaxInclusiveAmount", cur, totals.tax_inclusive),
        amount("cbc:AllowanceTotalAmount", cur, Decimal::ZERO),
        amount("cbc:PrepaidAmount", cur, Decimal::ZERO),
        amount("cbc:PayableAmount", cur, totals.tax_inclusive),
    ));
    for (i, (l, t)) in inv.lines.iter().zip(&totals.lines).enumerate() {
        x.push_str(&format!(
            "<cac:InvoiceLine>{}<cbc:InvoicedQuantity unitCode=\"PCE\">{}</cbc:InvoicedQuantity>{}<cac:TaxTotal>{}{}</cac:TaxTotal><cac:Item>{}{}</cac:Item><cac:Price>{}</cac:Price></cac:InvoiceLine>",
            el("cbc:ID", &(i + 1).to_string()),
            l.quantity.normalize(),
            amount("cbc:LineExtensionAmount", cur, t.net),
            amount("cbc:TaxAmount", cur, t.vat),
            amount("cbc:RoundingAmount", cur, t.gross),
            el("cbc:Name", &l.name),
            tax_category("ClassifiedTaxCategory", l.vat_category, l.vat_percent, None),
            amount("cbc:PriceAmount", cur, l.unit_price),
        ));
    }
    x.push_str("</Invoice>");
    x
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    pub fn sample() -> Invoice {
        Invoice {
            number: "INV-1".into(),
            uuid: Uuid::parse_str("3cf5ee18-ee25-44ea-a444-2c37ba7f28be").unwrap(),
            issue_date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            issue_time: NaiveTime::from_hms_opt(10, 15, 0).unwrap(),
            invoice_type: InvoiceType::Simplified,
            kind: DocumentKind::Invoice,
            billing_reference: None,
            note_reason: None,
            counter: 1,
            previous_hash: super::super::sign::INITIAL_PIH.into(),
            seller: Party {
                registration_name: "جمعية البر الخيرية".into(),
                vat_number: Some("300000000000003".into()),
                other_id: Some(("CRN".into(), "1010010000".into())),
                address: Address {
                    street: "King Fahd Rd".into(),
                    building_number: "1234".into(),
                    district: "Al Olaya".into(),
                    city: "Riyadh".into(),
                    postal_zone: "12211".into(),
                    country: "SA".into(),
                },
            },
            buyer: None,
            currency: "SAR".into(),
            payment_means: "10".into(),
            lines: vec![
                InvoiceLine {
                    name: "Charity bazaar item".into(),
                    quantity: dec!(3),
                    unit_price: dec!(33.333),
                    vat_category: VatCategory::S,
                    vat_percent: dec!(15),
                    exemption: None,
                },
                InvoiceLine {
                    name: "Event ticket".into(),
                    quantity: dec!(1),
                    unit_price: dec!(10.05),
                    vat_category: VatCategory::S,
                    vat_percent: dec!(15),
                    exemption: None,
                },
            ],
        }
    }

    #[test]
    fn totals_follow_ksa_rounding() {
        let t = sample().totals();
        assert_eq!(t.lines[0].net, dec!(100.00)); // 99.999 → 100.00
        assert_eq!(t.lines[1].vat, dec!(1.51)); // 1.5075 → 1.51
        assert_eq!(t.line_extension, dec!(110.05));
        // Category VAT is computed on the taxable total: 110.05 × 15 % = 16.5075 → 16.51.
        assert_eq!(t.vat, dec!(16.51));
        assert_eq!(t.tax_inclusive, dec!(126.56));
    }

    #[test]
    fn validation_rules() {
        let mut inv = sample();
        inv.validate().unwrap();
        inv.invoice_type = InvoiceType::Standard;
        assert!(inv.validate().is_err());
        let mut inv = sample();
        inv.kind = DocumentKind::CreditNote;
        assert!(inv.validate().is_err());
        let mut inv = sample();
        inv.lines[0].vat_category = VatCategory::O;
        assert!(inv.validate().is_err());
        inv.lines[0].vat_percent = Decimal::ZERO;
        inv.lines[0].exemption = Some(("VATEX-SA-OOS".into(), "Donation outside VAT scope".into()));
        inv.validate().unwrap();
    }

    #[test]
    fn canonical_form_has_no_shorthand_or_whitespace() {
        let inv = sample();
        let xml = render(&inv, &inv.totals(), &Envelope { extensions: None, qr_b64: None, with_signature_ref: false });
        assert!(!xml.contains("/>"));
        assert!(!xml.contains("\n"));
        assert!(!xml.contains("cac:Signature"));
        assert!(xml.contains("<cbc:InvoiceTypeCode name=\"0200000\">388</cbc:InvoiceTypeCode>"));
        assert!(xml.contains("<cbc:PayableAmount currencyID=\"SAR\">126.56</cbc:PayableAmount>"));
    }
}
