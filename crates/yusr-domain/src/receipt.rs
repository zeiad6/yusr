//! Bilingual donation receipts and Zakat certificates rendered as self-contained HTML,
//! printable from any browser or the desktop app's print dialog.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::charity::DonationKind;
use crate::hijri::HijriDate;
use crate::money::Currency;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub name_ar: String,
    pub name_en: String,
    /// Charity licence / registration number.
    pub license_no: String,
    pub vat_number: Option<String>,
    pub address: String,
    /// Statement printed on receipts, e.g. that no goods or services were given in return.
    pub tax_statement_ar: String,
    pub tax_statement_en: String,
    /// `data:image/png|jpeg|webp;base64,…` printed on receipts and letterheads.
    #[serde(default)]
    pub logo_data_url: Option<String>,
    /// Stamp image placed in the signature area of issued letters.
    #[serde(default)]
    pub stamp_data_url: Option<String>,
    /// charity_association, foundation, waqf, zakat_committee, cooperative, mosque, other.
    #[serde(default)]
    pub org_type: Option<String>,
    /// Registration with the supervising authority (e.g. the NCNP number in Saudi Arabia).
    #[serde(default)]
    pub registration_no: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    /// Month (1–12) the fiscal year starts; reports default to the current fiscal year.
    #[serde(default = "january")]
    pub fiscal_year_start: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptData {
    pub receipt_no: String,
    pub date: NaiveDate,
    pub donor_name: String,
    pub donor_national_id: Option<String>,
    pub amount: Decimal,
    pub currency: Currency,
    pub kind: DonationKind,
    pub fund_name_ar: String,
    pub fund_name_en: String,
    pub payment_method: String,
    /// SHA-256 of the audit record that sealed this donation, printed for verification.
    pub audit_hash: String,
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn hijri_text(date: NaiveDate) -> (String, String) {
    HijriDate::from_gregorian(date).map_or_else(|_| (String::new(), String::new()), |h| (h.format_ar(), h.format_en()))
}

const STYLE: &str = "body{font-family:Cairo,Inter,system-ui,sans-serif;max-width:720px;margin:24px auto;color:#111}\
.cols{display:grid;grid-template-columns:1fr 1fr;gap:24px}.ar{direction:rtl;text-align:right}\
h1{font-size:20px;margin:0 0 4px}table{width:100%;border-collapse:collapse;margin:16px 0}\
td{border:1px solid #ccc;padding:6px 8px;font-size:14px}.amt{font-size:22px;font-weight:700;text-align:center}\
.foot{font-size:11px;color:#555;word-break:break-all}@media print{body{margin:0}}";

fn row(ar_label: &str, value: &str, en_label: &str) -> String {
    format!("<tr><td class=\"ar\">{ar_label}</td><td style=\"text-align:center\">{}</td><td>{en_label}</td></tr>", escape(value))
}

fn page(title: &str, org: &Organization, body: &str, footer: &str) -> String {
    format!(
        "<!doctype html><html lang=\"ar\"><head><meta charset=\"utf-8\"><title>{}</title><style>{STYLE}</style></head><body>\
{}<div class=\"cols\"><div><h1>{}</h1><div>Licence {}</div></div><div class=\"ar\"><h1>{}</h1><div>ترخيص رقم {}</div></div></div>\
{body}<p class=\"foot\">{}</p></body></html>",
        escape(title),
        logo_img(org),
        escape(&org.name_en),
        escape(&org.license_no),
        escape(&org.name_ar),
        escape(&org.license_no),
        footer,
    )
}

fn january() -> u32 {
    1
}

pub const ORG_TYPES: [&str; 7] = ["charity_association", "foundation", "waqf", "zakat_committee", "cooperative", "mosque", "other"];

/// Largest accepted logo as a data URL (about 300 KB of image).
pub const MAX_LOGO_CHARS: usize = 400_000;

/// Raster images only: an SVG logo could carry script into documents opened from the app.
pub fn valid_logo(data_url: &str) -> bool {
    data_url.len() <= MAX_LOGO_CHARS
        && ["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/webp;base64,"].iter().any(|p| data_url.starts_with(p))
        && data_url.split_once(',').is_some_and(|(_, b64)| b64.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=')))
}

fn logo_img(org: &Organization) -> String {
    match org.logo_data_url.as_deref().filter(|l| valid_logo(l)) {
        Some(l) => format!("<div style=\"text-align:center\"><img src=\"{l}\" alt=\"\" style=\"max-height:72px\"></div>"),
        None => String::new(),
    }
}

fn common_rows(r: &ReceiptData) -> String {
    let (h_ar, _) = hijri_text(r.date);
    [
        row("رقم الإيصال", &r.receipt_no, "Receipt no."),
        row("التاريخ", &format!("{} — {}", r.date, h_ar), "Date"),
        row("اسم المتبرع", &r.donor_name, "Donor"),
        row("رقم الهوية", r.donor_national_id.as_deref().unwrap_or("—"), "National ID"),
        row("نوع التبرع", &format!("{} / {}", r.kind.arabic(), r.kind.as_str()), "Type"),
        row("الصندوق", &format!("{} / {}", r.fund_name_ar, r.fund_name_en), "Fund"),
        row("طريقة الدفع", &r.payment_method, "Payment method"),
    ]
    .concat()
}

/// Tax-deductible donation receipt.
pub fn render_receipt(org: &Organization, r: &ReceiptData) -> String {
    let amount = format!("{:.*} {}", r.currency.minor_units() as usize, r.currency.round(r.amount), r.currency);
    let body = format!(
        "<table>{}</table><div class=\"amt\">{}</div>\
<div class=\"cols\"><p>{}</p><p class=\"ar\">{}</p></div>",
        common_rows(r),
        escape(&amount),
        escape(&org.tax_statement_en),
        escape(&org.tax_statement_ar),
    );
    page(&format!("Receipt {}", r.receipt_no), org, &body, &format!("Audit seal SHA-256: {}", escape(&r.audit_hash)))
}

/// Zakat receipt certificate: confirms receipt of Zakat and the charity's undertaking
/// to disburse it only to the eight Shari'ah categories.
pub fn render_zakat_certificate(org: &Organization, r: &ReceiptData) -> String {
    let amount = format!("{:.*} {}", r.currency.minor_units() as usize, r.currency.round(r.amount), r.currency);
    let (h_ar, h_en) = hijri_text(r.date);
    let body = format!(
        "<h2 style=\"text-align:center\">شهادة استلام زكاة — Zakat Receipt Certificate</h2>\
<table>{}</table><div class=\"amt\">{}</div>\
<div class=\"cols\"><p>{} certifies that it received the above Zakat on {} ({}) and undertakes to disburse it \
exclusively to the eligible categories named in Qur'an 9:60, kept in a separate restricted fund.</p>\
<p class=\"ar\">تشهد {} باستلام مبلغ الزكاة الموضح أعلاه بتاريخ {} وتتعهد بصرفه في مصارف الزكاة الثمانية \
الواردة في سورة التوبة (الآية ٦٠) حصراً، وحفظه في صندوق مقيّد مستقل.</p></div>",
        common_rows(r),
        escape(&amount),
        escape(&org.name_en),
        r.date,
        escape(&h_en),
        escape(&org.name_ar),
        escape(&h_ar),
    );
    page(&format!("Zakat certificate {}", r.receipt_no), org, &body, &format!("Audit seal SHA-256: {}", escape(&r.audit_hash)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_small_raster_logos_are_accepted() {
        assert!(valid_logo("data:image/png;base64,iVBORw0KGgo="));
        assert!(!valid_logo("data:image/svg+xml;base64,PHN2Zz4="));
        assert!(!valid_logo("data:image/png;base64,\"><script>"));
        assert!(!valid_logo(&format!("data:image/png;base64,{}", "A".repeat(MAX_LOGO_CHARS))));
    }
    use rust_decimal_macros::dec;

    fn org() -> Organization {
        Organization {
            name_ar: "جمعية البر".into(),
            name_en: "Al-Birr Society".into(),
            license_no: "123".into(),
            vat_number: None,
            address: "Riyadh".into(),
            tax_statement_ar: "لم تُقدَّم سلع أو خدمات مقابل هذا التبرع.".into(),
            tax_statement_en: "No goods or services were provided in exchange.".into(),
            logo_data_url: None,
            stamp_data_url: None,
            org_type: None,
            registration_no: None,
            phone: None,
            email: None,
            website: None,
            fiscal_year_start: 1,
        }
    }

    fn data() -> ReceiptData {
        ReceiptData {
            receipt_no: "R-2025-000001".into(),
            date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            donor_name: "<script>alert(1)</script>".into(),
            donor_national_id: None,
            amount: dec!(500),
            currency: Currency::SAR,
            kind: DonationKind::Zakat,
            fund_name_ar: "صندوق الزكاة".into(),
            fund_name_en: "Zakat Fund".into(),
            payment_method: "mada".into(),
            audit_hash: "ab".repeat(32),
        }
    }

    #[test]
    fn receipt_escapes_and_shows_dual_dates() {
        let html = render_receipt(&org(), &data());
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("١ رمضان ١٤٤٦ هـ"));
        assert!(html.contains("500.00 SAR"));
    }

    #[test]
    fn certificate_names_asnaf_undertaking() {
        let html = render_zakat_certificate(&org(), &data());
        assert!(html.contains("مصارف الزكاة الثمانية"));
        assert!(html.contains("1 Ramadan 1446 AH"));
    }
}
