//! Integration tests against a real PostgreSQL. Each test gets its own database.
//! Set `YUSR_TEST_DATABASE_URL` to a server URL whose user may CREATE DATABASE,
//! e.g. `postgres://postgres@127.0.0.1:5432/postgres`; without it the tests are skipped.

use chrono::{NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::Connection;
use uuid::Uuid;
use yusr_domain::charity::{Asnaf, DonationKind};
use yusr_domain::money::Currency;
use yusr_domain::receipt::Organization;
use yusr_domain::scoring::{CaseProfile, Housing};
use yusr_infra::aid::{AidKind, NewBeneficiary, NewDistribution};
use yusr_infra::budgets::NewBudget;
use yusr_infra::donations::{NewDonation, NewDonor};
use yusr_infra::{InfraError, Store};

fn org() -> Organization {
    Organization {
        name_ar: "جمعية الاختبار".into(),
        name_en: "Test Charity".into(),
        license_no: "999".into(),
        vat_number: Some("300000000000003".into()),
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

/// A store on its own throwaway database, dropped when the test ends (pass or fail).
struct TestDb {
    store: Store,
    admin: String,
    name: String,
}

impl std::ops::Deref for TestDb {
    type Target = Store;
    fn deref(&self) -> &Store {
        &self.store
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Drop cannot await, and the test's runtime may be shutting down: use a private one.
        // FORCE (PostgreSQL 13+) closes the pool connections still open on the database.
        let (admin, name) = (self.admin.clone(), self.name.clone());
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
            rt.block_on(async {
                // Parallel tests can briefly exhaust max_connections; retry rather than leak the database.
                for attempt in 0..10u64 {
                    if let Ok(mut conn) = sqlx::PgConnection::connect(&admin).await {
                        if sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)")).execute(&mut conn).await.is_ok() {
                            return;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt + 1))).await;
                }
                eprintln!("could not drop test database {name}");
            });
        })
        .join();
    }
}

async fn store() -> Option<TestDb> {
    let Ok(admin) = std::env::var("YUSR_TEST_DATABASE_URL") else {
        eprintln!("YUSR_TEST_DATABASE_URL not set; skipping database test");
        return None;
    };
    let name = format!("yusr_t_{}", Uuid::new_v4().simple());
    let mut conn = sqlx::PgConnection::connect(&admin).await.expect("connect to admin database");
    sqlx::query(&format!("CREATE DATABASE {name}")).execute(&mut conn).await.expect("create test database");
    let (prefix, _) = admin.rsplit_once('/').expect("URL has a database path");
    let store = Store::open(&format!("{prefix}/{name}"), &org()).await.expect("open store");
    Some(TestDb { store, admin, name })
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

async fn fund_id(s: &Store, code: &str) -> Uuid {
    s.funds().await.unwrap().into_iter().find(|f| f.code == code).unwrap().id
}

async fn donor(s: &Store) -> Uuid {
    let new = NewDonor {
        is_organization: false,
        full_name: "عبدالله المحسن".into(),
        national_id: Some(Uuid::new_v4().simple().to_string()),
        phone: None,
        email: None,
        country: Some("SA".into()),
        preferred_language: "ar".into(),
        notes: None,
    };
    s.create_donor(&new, "tester").await.unwrap()
}

fn donation(donor_id: Uuid, kind: Option<DonationKind>, amount: Decimal, note: &str) -> NewDonation {
    NewDonation {
        donor_id,
        donation_date: d(2025, 3, 1),
        kind,
        donor_designated: false,
        fund_id: None,
        project_id: None,
        amount,
        currency: Currency::SAR,
        payment_method: "bank".into(),
        payment_ref: None,
        note: Some(note.into()),
    }
}

async fn beneficiary(s: &Store, category: Asnaf) -> Uuid {
    let b = NewBeneficiary {
        full_name: "أسرة محتاجة".into(),
        national_id: None,
        phone: None,
        city: Some("Riyadh".into()),
        category,
        profile: CaseProfile {
            household_size: 5,
            monthly_income: dec!(1000),
            poverty_line_per_person: dec!(1000),
            dependents_under_18: 3,
            members_with_disability: 0,
            members_with_chronic_illness: 1,
            outstanding_debt: dec!(10000),
            housing: Housing::Rented,
            monthly_rent: dec!(800),
            head_of_household_employed: false,
            female_or_orphan_headed: true,
        },
    };
    s.create_beneficiary(&b, "tester").await.unwrap().id
}

fn aid(beneficiary_id: Uuid, fund_id: Uuid, kind: AidKind, amount: Decimal) -> NewDistribution {
    NewDistribution {
        beneficiary_id,
        kind,
        fund_id,
        project_id: None,
        amount,
        currency: Currency::SAR,
        description: "monthly support".into(),
        items: None,
        distributed_on: d(2025, 3, 5),
        paid_from_bank: true,
    }
}

fn code(e: &InfraError) -> &'static str {
    match e {
        InfraError::Domain(d) => d.code(),
        InfraError::Conflict(..) => "conflict",
        InfraError::NotFound(_) => "not_found",
        _ => "other",
    }
}

#[tokio::test]
async fn zakat_donation_is_classified_posted_and_receipted() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    let r = s.record_donation(&donation(donor, None, dec!(2500), "زكاة المال لعام ١٤٤٦"), "cashier").await.unwrap();
    assert_eq!(r.kind, DonationKind::Zakat);
    assert_eq!(r.receipt_no, "R-2025-000001");
    assert_eq!(r.entry.hijri_date, "1446-09-01");

    let tb = s.trial_balance(d(2025, 12, 31)).await.unwrap();
    let (dr, cr): (Decimal, Decimal) = tb.iter().fold((Decimal::ZERO, Decimal::ZERO), |(a, b), r| (a + r.debit, b + r.credit));
    assert_eq!(dr, dec!(2500));
    assert_eq!(dr, cr);

    let zakat = s.fund_balances(d(2025, 12, 31)).await.unwrap().into_iter().find(|f| f.code == "ZAKAT").unwrap();
    assert_eq!(zakat.net_assets, dec!(2500));

    let html = s.receipt_html(r.donation_id, true).await.unwrap();
    assert!(html.contains("R-2025-000001") && html.contains("مصارف الزكاة الثمانية"));
    let second = s.record_donation(&donation(donor, Some(DonationKind::Sadaqah), dec!(10), "x"), "cashier").await.unwrap();
    assert_eq!(second.receipt_no, "R-2025-000002");
    assert!(s.receipt_html(second.donation_id, true).await.is_err());

    let report = yusr_infra::audit::verify(&s.pool).await.unwrap();
    assert!(report.ok, "{report:?}");
    assert_eq!(report.records_checked, 3); // donor + two donations
}

#[tokio::test]
async fn shariah_and_restriction_rules_are_enforced() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    let general = fund_id(&s, "GEN").await;
    let zakat = fund_id(&s, "ZAKAT").await;
    let kaff = fund_id(&s, "KAFF").await;

    // Zakat cannot be dropped into the general fund.
    let mut z = donation(donor, Some(DonationKind::Zakat), dec!(100), "");
    z.fund_id = Some(general);
    assert_eq!(code(&s.record_donation(&z, "c").await.unwrap_err()), "restriction_mismatch");
    // Nor into another restricted fund of a different kind.
    z.fund_id = Some(kaff);
    assert_eq!(code(&s.record_donation(&z, "c").await.unwrap_err()), "restriction_mismatch");

    s.record_donation(&donation(donor, Some(DonationKind::Zakat), dec!(1000), ""), "c").await.unwrap();
    s.record_donation(&donation(donor, Some(DonationKind::Kaffarah), dec!(1000), ""), "c").await.unwrap();

    let debtor = beneficiary(&s, Asnaf::Gharimin).await;
    let poor = beneficiary(&s, Asnaf::Masakin).await;
    // Kaffarah feeds the poor and needy only.
    assert_eq!(code(&s.distribute(&aid(debtor, kaff, AidKind::Cash, dec!(100)), "w").await.unwrap_err()), "ineligible_recipient");
    s.distribute(&aid(poor, kaff, AidKind::Cash, dec!(100)), "w").await.unwrap();
    // Zakat may go to debtors (al-gharimin).
    s.distribute(&aid(debtor, zakat, AidKind::Cash, dec!(400), ), "w").await.unwrap();
    // But a restricted fund is never overdrawn.
    assert_eq!(code(&s.distribute(&aid(debtor, zakat, AidKind::Cash, dec!(600.01)), "w").await.unwrap_err()), "insufficient_fund_balance");
    s.distribute(&aid(debtor, zakat, AidKind::Cash, dec!(600)), "w").await.unwrap();

    // Waqf principal is never spent.
    let waqf = fund_id(&s, "WAQF").await;
    s.record_donation(&donation(donor, Some(DonationKind::Waqf), dec!(5000), ""), "c").await.unwrap();
    assert_eq!(code(&s.distribute(&aid(poor, waqf, AidKind::Cash, dec!(1)), "w").await.unwrap_err()), "endowment_principal_spend");
}

#[tokio::test]
async fn budgets_block_overspending() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    let general = fund_id(&s, "GEN").await;
    s.record_donation(&donation(donor, Some(DonationKind::General), dec!(10000), ""), "c").await.unwrap();
    s.create_budget(
        &NewBudget { fund_id: general, project_id: None, period_start: d(2025, 1, 1), period_end: d(2025, 12, 31), approved: dec!(1000), committed: dec!(0) },
        "cfo",
    )
    .await
    .unwrap();
    let b = beneficiary(&s, Asnaf::Fuqara).await;
    let ok = s.distribute(&aid(b, general, AidKind::Cash, dec!(800)), "w").await.unwrap();
    assert!(ok.entry.budget_warnings.is_empty());
    let warn = s.distribute(&aid(b, general, AidKind::Cash, dec!(150)), "w").await.unwrap();
    assert_eq!(warn.entry.budget_warnings.len(), 1);
    assert_eq!(code(&s.distribute(&aid(b, general, AidKind::Cash, dec!(50.01)), "w").await.unwrap_err()), "budget_exceeded");
    let usage = s.budget_usage(d(2025, 6, 1)).await.unwrap();
    assert_eq!(usage[0].spent, dec!(950));
}

#[tokio::test]
async fn ledger_and_audit_are_write_once_and_tampering_is_detected() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    let r = s.record_donation(&donation(donor, Some(DonationKind::Sadaqah), dec!(50), ""), "c").await.unwrap();

    for sql in [
        "UPDATE journal_lines SET amount = 1",
        "DELETE FROM journal_lines",
        "UPDATE journal_entries SET description = 'x'",
        "DELETE FROM journal_entries",
        "UPDATE audit_log SET actor = 'x'",
        "DELETE FROM audit_log",
        "TRUNCATE audit_log",
    ] {
        let err = sqlx::query(sql).execute(&s.pool).await.expect_err(sql);
        assert!(err.to_string().contains("write-once") || err.to_string().contains("immutable") || err.to_string().contains("deleted"), "{sql}: {err}");
    }

    // A superuser can switch the trigger off; the hash chain still exposes the edit.
    let mut tx = s.pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE audit_log DISABLE TRIGGER audit_log_worm").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE audit_log SET payload = jsonb_set(payload, '{description}', '\"edited\"') WHERE entity = 'journal_entry'")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE audit_log ENABLE TRIGGER audit_log_worm").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let report = yusr_infra::audit::verify(&s.pool).await.unwrap();
    assert!(!report.ok);
    assert!(report.error.unwrap().contains("content altered"));
    let _ = r;
}

#[tokio::test]
async fn reversal_nets_to_zero_and_only_once() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    let r = s.record_donation(&donation(donor, Some(DonationKind::General), dec!(300), ""), "c").await.unwrap();
    let rev = s.reverse_entry(r.entry.id, d(2025, 3, 2), "entered twice", "accountant").await.unwrap();
    assert_eq!(s.entry(r.entry.id).await.unwrap().status, "reversed");
    assert_eq!(s.entry(rev.id).await.unwrap().reverses_id, Some(r.entry.id));
    let general = s.fund_balances(d(2025, 12, 31)).await.unwrap().into_iter().find(|f| f.code == "GEN").unwrap();
    assert_eq!(general.net_assets, Decimal::ZERO);
    assert_eq!(code(&s.reverse_entry(r.entry.id, d(2025, 3, 3), "again", "a").await.unwrap_err()), "conflict");
}

#[tokio::test]
async fn foreign_currency_uses_historical_rate() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    s.set_rate(Currency::USD, d(2025, 1, 1), dec!(3.75), "SAMA", "admin").await.unwrap();
    s.set_rate(Currency::USD, d(2025, 6, 1), dec!(3.7505), "SAMA", "admin").await.unwrap();
    let mut usd = donation(donor, Some(DonationKind::Sadaqah), dec!(100), "");
    usd.currency = Currency::USD;
    let r = s.record_donation(&usd, "c").await.unwrap();
    assert_eq!(r.entry.lines[0].fx_rate, dec!(3.75));
    assert_eq!(r.entry.lines[0].base_amount, dec!(375.00));
    usd.donation_date = d(2025, 7, 1);
    let r = s.record_donation(&usd, "c").await.unwrap();
    assert_eq!(r.entry.lines[0].base_amount, dec!(375.05));
    usd.donation_date = d(2024, 12, 31);
    assert!(s.record_donation(&usd, "c").await.is_err(), "no rate before 2025-01-01");
}

#[tokio::test]
async fn concurrent_spending_never_overdraws_and_keeps_the_chain_intact() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    s.record_donation(&donation(donor, Some(DonationKind::Zakat), dec!(500), ""), "c").await.unwrap();
    let zakat = fund_id(&s, "ZAKAT").await;
    let b = beneficiary(&s, Asnaf::Fuqara).await;

    let tasks: Vec<_> = (0..10)
        .map(|_| {
            let s = Store::clone(&s);
            tokio::spawn(async move { s.distribute(&aid(b, zakat, AidKind::Cash, dec!(100)), "w").await })
        })
        .collect();
    let mut ok = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(e) => assert_eq!(code(&e), "insufficient_fund_balance", "{e}"),
        }
    }
    assert_eq!(ok, 5);
    let fund = s.fund_balances(d(2025, 12, 31)).await.unwrap().into_iter().find(|f| f.code == "ZAKAT").unwrap();
    assert_eq!(fund.net_assets, Decimal::ZERO);
    assert!(yusr_infra::audit::verify(&s.pool).await.unwrap().ok);
}

#[tokio::test]
async fn vouchers_are_issued_redeemed_once() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    s.record_donation(&donation(donor, Some(DonationKind::General), dec!(1000), ""), "c").await.unwrap();
    let general = fund_id(&s, "GEN").await;
    let b = beneficiary(&s, Asnaf::Fuqara).await;
    let dist = s.distribute(&aid(b, general, AidKind::Voucher, dec!(200)), "w").await.unwrap();
    let code_str = dist.voucher_code.unwrap();
    s.redeem_voucher(&code_str.to_lowercase(), d(2025, 3, 10), "Panda Market", "shop").await.unwrap();
    assert_eq!(code(&s.redeem_voucher(&code_str, d(2025, 3, 11), "Panda Market", "shop").await.unwrap_err()), "conflict");
    assert_eq!(code(&s.redeem_voucher("AAAA-BBBB-CCCC-DDDD-EEEE-FFFF", d(2025, 3, 11), "x", "shop").await.unwrap_err()), "invalid_voucher");
    let tb = s.trial_balance(d(2025, 12, 31)).await.unwrap();
    let vouchers = tb.iter().find(|r| r.code == "2200").unwrap();
    assert_eq!(vouchers.debit, vouchers.credit, "voucher liability settled");
}

#[tokio::test]
async fn zatca_invoices_chain_counter_and_previous_hash() {
    use yusr_domain::zatca::invoice::{Address, DocumentKind, InvoiceLine, InvoiceType, Party, VatCategory};
    use yusr_domain::zatca::sign::{Credentials, INITIAL_PIH};
    use yusr_infra::zatca::InvoiceRequest;
    let Some(s) = store().await else { return };
    let creds = Credentials::self_signed_for_development(k256::ecdsa::SigningKey::from_bytes(&[9u8; 32].into()).unwrap(), "Dev");
    let req = InvoiceRequest {
        number: None,
        issued_at: NaiveDateTime::new(d(2025, 3, 1), chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
        invoice_type: InvoiceType::Simplified,
        kind: DocumentKind::Invoice,
        billing_reference: None,
        note_reason: None,
        seller: Party {
            registration_name: "جمعية الاختبار".into(),
            vat_number: Some("300000000000003".into()),
            other_id: None,
            address: Address {
                street: "s".into(),
                building_number: "1234".into(),
                district: "d".into(),
                city: "Riyadh".into(),
                postal_zone: "12345".into(),
                country: "SA".into(),
            },
        },
        buyer: None,
        currency: "SAR".into(),
        payment_means: "10".into(),
        lines: vec![InvoiceLine {
            name: "Bazaar item".into(),
            quantity: dec!(1),
            unit_price: dec!(100),
            vat_category: VatCategory::S,
            vat_percent: dec!(15),
            exemption: None,
        }],
    };
    let (a, _) = s.issue_invoice(&req, &creds, "cashier").await.unwrap();
    let (b, xml) = s.issue_invoice(&req, &creds, "cashier").await.unwrap();
    assert_eq!((a.icv, b.icv), (1, 2));
    assert_eq!(a.previous_hash, INITIAL_PIH);
    assert_eq!(b.previous_hash, a.invoice_hash);
    assert_eq!(b.number, "INV-00000002");
    assert!(xml.contains(&a.invoice_hash));
}

fn conflict_code(e: &InfraError) -> &'static str {
    match e {
        InfraError::Conflict(c, _) => c,
        _ => "other",
    }
}

#[tokio::test]
async fn archive_numbers_documents_keeps_files_write_once_and_voids() {
    use yusr_infra::archive::{DocumentCategory, DocumentQuery, NewDocument};
    let Some(s) = store().await else { return };
    let new = |title: &str| NewDocument {
        title: title.into(),
        category: DocumentCategory::Contract,
        reference_no: Some(" 77/ع ".into()),
        document_date: d(2026, 10, 1),
        party: Some("شركة المقاولات".into()),
        description: None,
        tags: vec!["عقد".into(), " ".into()],
    };
    let doc = s.create_document(&new("عقد صيانة المقر"), "archivist").await.unwrap();
    assert_eq!(doc.doc_no, "ARC-2026-000001");
    assert_eq!(doc.reference_no.as_deref(), Some("77/ع"));
    assert_eq!(doc.tags, vec!["عقد".to_string()]);
    assert_eq!(s.create_document(&new("ملحق العقد"), "archivist").await.unwrap().doc_no, "ARC-2026-000002");

    let bytes = b"%PDF-1.4 signed contract".to_vec();
    let file = s.add_document_file(doc.id, "../scans/عقد.pdf", "application/pdf", &bytes, "archivist").await.unwrap();
    assert_eq!(file.file_name, "عقد.pdf");
    assert_eq!(file.size_bytes, bytes.len() as i64);
    let (meta, data) = s.document_file(file.id).await.unwrap();
    assert_eq!(data, bytes);
    assert_eq!(meta.sha256.len(), 64);
    assert!(s.add_document_file(doc.id, "empty.txt", "text/plain", &[], "archivist").await.is_err());

    // Files are evidence: the database itself refuses to rewrite them.
    assert!(sqlx::query("UPDATE document_files SET data = 'x' WHERE id = $1").bind(file.id).execute(&s.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM document_files WHERE id = $1").bind(file.id).execute(&s.pool).await.is_err());

    // Search reaches file names and party; voided documents drop out unless asked for.
    let found = s.documents(&DocumentQuery { q: Some("عقد.pdf".into()), ..Default::default() }, 50).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file_count, 1);
    s.void_document(doc.id, "نسخة مكررة", "archivist").await.unwrap();
    assert_eq!(conflict_code(&s.void_document(doc.id, "again", "archivist").await.unwrap_err()), "document_voided");
    assert_eq!(conflict_code(&s.add_document_file(doc.id, "b.pdf", "application/pdf", b"x", "archivist").await.unwrap_err()), "document_voided");
    assert_eq!(s.documents(&DocumentQuery::default(), 50).await.unwrap().len(), 1);
    assert_eq!(s.documents(&DocumentQuery { include_voided: true, ..Default::default() }, 50).await.unwrap().len(), 2);

    let report = yusr_infra::audit::verify(&s.pool).await.unwrap();
    assert!(report.ok, "{report:?}");
}

#[tokio::test]
async fn letters_are_numbered_per_hijri_year_and_frozen_once_issued() {
    use yusr_infra::archive::{DocumentCategory, NewDocument};
    use yusr_infra::letters::{LetterDraft, LetterKind};
    let Some(s) = store().await else { return };
    let draft = LetterDraft {
        kind: LetterKind::Congratulation,
        recipient_name: "مدير عام الجمعية الشقيقة".into(),
        recipient_title: Some("سعادة".into()),
        recipient_org: None,
        subject: "تهنئة بحلول شهر رمضان".into(),
        body: "يسرنا أن نرفع لكم أطيب التهاني.".into(),
        closing: None,
        signatory_name: "رئيس مجلس الإدارة".into(),
        signatory_title: None,
        attachments: None,
        cc: None,
        letter_date: d(2026, 10, 25),
    };
    let mut l = s.create_letter(&draft, "secretary").await.unwrap();
    assert_eq!(l.status, "draft");
    assert_eq!(l.hijri_date, "1448-05-14");
    assert!(l.ref_no.is_none());

    let mut edited = draft.clone();
    edited.body = "يسرنا أن نرفع لكم أطيب التهاني والتبريكات.".into();
    l = s.update_letter(l.id, &edited, "secretary").await.unwrap();
    assert!(l.body.ends_with("والتبريكات."));

    l = s.issue_letter(l.id, "secretary").await.unwrap();
    assert_eq!(l.ref_no.as_deref(), Some("1448/0001"));
    assert_eq!(conflict_code(&s.issue_letter(l.id, "secretary").await.unwrap_err()), "letter_issued");
    assert_eq!(conflict_code(&s.update_letter(l.id, &draft, "secretary").await.unwrap_err()), "letter_issued");
    // The trigger guards issued letters even against direct SQL.
    assert!(sqlx::query("UPDATE letters SET body = 'x' WHERE id = $1").bind(l.id).execute(&s.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM letters WHERE id = $1").bind(l.id).execute(&s.pool).await.is_err());

    let second = s.create_letter(&draft, "secretary").await.unwrap();
    assert_eq!(s.issue_letter(second.id, "secretary").await.unwrap().ref_no.as_deref(), Some("1448/0002"));

    let copy = NewDocument {
        title: l.subject.clone(),
        category: DocumentCategory::Outgoing,
        reference_no: l.ref_no.clone(),
        document_date: l.letter_date,
        party: Some(l.recipient_name.clone()),
        description: None,
        tags: vec![],
    };
    let doc = s.create_document(&copy, "secretary").await.unwrap();
    assert_eq!(s.link_letter_archive(l.id, doc.id, "secretary").await.unwrap().archive_id, Some(doc.id));
    assert_eq!(conflict_code(&s.link_letter_archive(l.id, doc.id, "secretary").await.unwrap_err()), "letter_not_archivable");
    assert_eq!(s.letters(Some("issued"), Some("رمضان"), 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn period_reports_reconcile_with_the_ledger() {
    let Some(s) = store().await else { return };
    let donor = donor(&s).await;
    s.record_donation(&donation(donor, Some(DonationKind::Zakat), dec!(3000), ""), "cashier").await.unwrap();
    s.record_donation(&donation(donor, Some(DonationKind::Sadaqah), dec!(500), ""), "cashier").await.unwrap();
    let zakat = fund_id(&s, "ZAKAT").await;
    let case = beneficiary(&s, Asnaf::Fuqara).await;
    s.distribute(&aid(case, zakat, AidKind::Cash, dec!(1200)), "worker").await.unwrap();

    let (from, to) = (d(2025, 1, 1), d(2025, 12, 31));
    let rows = s.activities(from, to).await.unwrap();
    let total = |t: &str| rows.iter().filter(|r| r.account_type == t).map(|r| r.amount).sum::<Decimal>();
    assert_eq!(total("revenue"), dec!(3500));
    assert_eq!(total("expense"), dec!(1200));

    let ds = s.donation_summary(from, to).await.unwrap();
    assert_eq!((ds.total, ds.count, ds.donors), (dec!(3500), 2, 1));
    assert_eq!(ds.by_kind[0].key, "zakat");
    assert_eq!(ds.top_donors[0].total, dec!(3500));
    assert_eq!(ds.by_month[0].key, "2025-03");

    let a = s.aid_summary(from, to).await.unwrap();
    assert_eq!((a.total, a.count, a.beneficiaries), (dec!(1200), 1, 1));
    assert_eq!(a.by_category[0].key, "fuqara");
    assert_eq!(a.by_fund[0].key, "ZAKAT");
}

#[tokio::test]
async fn idempotency_keys_replay_once_and_free_on_failure() {
    use yusr_infra::security::Claim;
    let Some(s) = store().await else { return };
    let (scope, key) = ("donations:user-1", "3f2b9c1e-7d4a-4e8b-9a6f-2c1d0e5b7a90");
    assert_eq!(s.idempotency_claim(scope, key).await.unwrap(), Claim::Fresh);
    assert_eq!(s.idempotency_claim(scope, key).await.unwrap(), Claim::InProgress);
    s.idempotency_complete(scope, key, 201, &serde_json::json!({ "receipt_no": "R-1" })).await.unwrap();
    assert_eq!(s.idempotency_claim(scope, key).await.unwrap(), Claim::Replay { status: 201, body: serde_json::json!({ "receipt_no": "R-1" }) });
    // Same key from another caller or endpoint is independent.
    assert_eq!(s.idempotency_claim("donations:user-2", key).await.unwrap(), Claim::Fresh);
    // A failed attempt releases its key so the client can retry it.
    s.idempotency_release("donations:user-2", key).await.unwrap();
    assert_eq!(s.idempotency_claim("donations:user-2", key).await.unwrap(), Claim::Fresh);
}

#[tokio::test]
async fn parallel_failed_logins_all_count_toward_the_lockout() {
    let Some(s) = store().await else { return };
    let (id,): (Uuid,) = sqlx::query_as("INSERT INTO users (username, full_name, password_hash, role) VALUES ('target', 'T', 'x', 'viewer') RETURNING id")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    let attempts = (0..4).map(|_| {
        let s = Store::clone(&s);
        tokio::spawn(async move { s.record_failed_login(id, 5, 15).await.unwrap() })
    });
    for a in attempts {
        assert!(!a.await.unwrap());
    }
    let (failed,): (i32,) = sqlx::query_as("SELECT failed_logins FROM users WHERE id = $1").bind(id).fetch_one(&s.pool).await.unwrap();
    assert_eq!(failed, 4, "no failure may be lost to a race");
    assert!(s.record_failed_login(id, 5, 15).await.unwrap(), "the fifth failure locks the account");
    s.clear_failed_logins(id).await.unwrap();
    let (locked,): (bool,) = sqlx::query_as("SELECT locked_until IS NOT NULL FROM users WHERE id = $1").bind(id).fetch_one(&s.pool).await.unwrap();
    assert!(!locked);
}

#[tokio::test]
async fn sponsorship_payments_are_receipted_donations_and_drive_arrears() {
    use yusr_infra::sponsorships::{NewSponsorship, SponsorshipPayment, SponsorshipProgram, SponsorshipStatus};
    let Some(s) = store().await else { return };
    let sponsor = donor(&s).await;
    let orphan = beneficiary(&s, Asnaf::Fuqara).await;
    let today = chrono::Utc::now().date_naive();
    let start = today.checked_sub_months(chrono::Months::new(2)).unwrap();
    let new = NewSponsorship {
        donor_id: sponsor,
        beneficiary_id: Some(orphan),
        program: SponsorshipProgram::Orphan,
        kind: DonationKind::Sadaqah,
        fund_id: None,
        monthly_amount: dec!(300),
        currency: Currency::SAR,
        start_date: start,
        end_date: None,
        notes: None,
    };
    let sp = s.create_sponsorship(&new, "officer").await.unwrap();
    assert_eq!((sp.months_due, sp.months_paid, sp.arrears_months), (3, 0, 3));
    let bad = NewSponsorship { monthly_amount: dec!(-1), ..new.clone() };
    assert!(s.create_sponsorship(&bad, "officer").await.is_err());

    let pay = |months| SponsorshipPayment { months, donation_date: today, payment_method: "transfer".into(), payment_ref: None, note: None };
    let r = s.pay_sponsorship(sp.id, &pay(2), "officer").await.unwrap();
    assert!(r.receipt_no.starts_with("R-"));
    // Earmarked sadaqah lands in the designated-sadaqah fund.
    assert_eq!(r.fund_id, fund_id(&s, "SADQ").await);
    assert!(s.pay_sponsorship(sp.id, &pay(0), "officer").await.is_err());

    let detail = s.sponsorship(sp.id).await.unwrap();
    assert_eq!((detail.sponsorship.months_paid, detail.sponsorship.arrears_months), (2, 1));
    assert_eq!(detail.sponsorship.amount_paid, dec!(600));
    assert_eq!(detail.payments.len(), 1);

    let ended = s.set_sponsorship_status(sp.id, SponsorshipStatus::Ended, "officer").await.unwrap();
    assert_eq!(ended.end_date, Some(today));
    assert_eq!(ended.arrears_months, 0);
    assert_eq!(conflict_code(&s.pay_sponsorship(sp.id, &pay(1), "officer").await.unwrap_err()), "sponsorship_ended");
    assert!(sqlx::query("DELETE FROM sponsorship_payments").execute(&s.pool).await.is_err());

    let report = yusr_infra::audit::verify(&s.pool).await.unwrap();
    assert!(report.ok, "{report:?}");
}

#[tokio::test]
async fn volunteer_hours_are_validated_and_summarised() {
    use yusr_infra::volunteers::{NewHours, NewVolunteer, VolunteerStatus};
    let Some(s) = store().await else { return };
    let today = chrono::Utc::now().date_naive();
    let v = s
        .create_volunteer(
            &NewVolunteer {
                full_name: "سلمى المتطوعة".into(),
                national_id: None,
                phone: Some("0500000001".into()),
                email: None,
                city: Some("الرياض".into()),
                skills: vec!["تصوير".into(), " ".into()],
                joined_on: today,
                notes: None,
            },
            "coordinator",
        )
        .await
        .unwrap();
    assert_eq!(v.skills, vec!["تصوير".to_string()]);
    let entry = |hours, worked_on| NewHours { volunteer_id: v.id, activity: "توزيع السلال".into(), project_id: None, worked_on, hours, notes: None };
    s.log_hours(&entry(dec!(4.5), today), "coordinator").await.unwrap();
    s.log_hours(&entry(dec!(3), today), "coordinator").await.unwrap();
    assert!(s.log_hours(&entry(dec!(0), today), "coordinator").await.is_err());
    assert!(s.log_hours(&entry(dec!(25), today), "coordinator").await.is_err());
    assert!(s.log_hours(&entry(dec!(2), today.succ_opt().unwrap()), "coordinator").await.is_err());
    assert_eq!(s.volunteer(v.id).await.unwrap().total_hours, dec!(7.5));

    let sum = s.volunteer_summary(today, today).await.unwrap();
    assert_eq!((sum.total_hours, sum.entries, sum.volunteers), (dec!(7.5), 2, 1));
    assert_eq!(sum.by_activity[0].key, "توزيع السلال");

    s.set_volunteer_status(v.id, VolunteerStatus::Inactive, "coordinator").await.unwrap();
    assert_eq!(conflict_code(&s.log_hours(&entry(dec!(1), today), "coordinator").await.unwrap_err()), "volunteer_inactive");
    assert!(sqlx::query("UPDATE volunteer_hours SET hours = 1").execute(&s.pool).await.is_err());
}

#[tokio::test]
async fn backup_restores_exactly_into_another_database() {
    let Some(a) = store().await else { return };
    let Some(b) = store().await else { return };
    let donor_id = donor(&a).await;
    let first = a.record_donation(&donation(donor_id, Some(DonationKind::Sadaqah), dec!(150), "قبل النسخة"), "tester").await.unwrap();
    let backup = a.backup("user:admin").await.unwrap();
    assert!(backup.manifest.tables.iter().any(|t| t.name == "journal_lines" && t.rows >= 2));

    // A tampered byte is caught before anything is touched.
    let mut tampered = backup.bytes.clone();
    let mid = tampered.len() / 2;
    tampered[mid] ^= 0xff;
    assert!(yusr_infra::backup::parse(&tampered).is_err());

    let other_donor = donor(&b).await;
    b.record_donation(&donation(other_donor, Some(DonationKind::Sadaqah), dec!(9), "بيانات ستُستبدل"), "tester").await.unwrap();
    let parsed = yusr_infra::backup::parse(&backup.bytes).unwrap();
    let report = b.restore(&parsed, "user:admin").await.unwrap();
    assert!(report.verified);

    let rows = b.donations(None, d(2000, 1, 1), d(2100, 1, 1), 50).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].receipt_no, first.receipt_no);
    assert!(yusr_infra::audit::verify(&b.pool).await.unwrap().ok);
    // Write-once triggers are back on after the load.
    assert!(sqlx::query("DELETE FROM journal_lines").execute(&b.pool).await.is_err());
    // Sequences continue after the restored rows instead of colliding with them.
    let second = b.record_donation(&donation(donor_id, Some(DonationKind::Sadaqah), dec!(10), "بعد الاستعادة"), "tester").await.unwrap();
    assert_ne!(second.receipt_no, first.receipt_no);

    // Restoring into the source rolls it back to the snapshot.
    a.record_donation(&donation(donor_id, Some(DonationKind::Sadaqah), dec!(5), "بعد النسخة"), "tester").await.unwrap();
    a.restore(&parsed, "user:admin").await.unwrap();
    assert_eq!(a.donations(None, d(2000, 1, 1), d(2100, 1, 1), 50).await.unwrap().len(), 1);
}

#[tokio::test]
async fn registry_edits_are_audited_and_money_stays_fixed() {
    let Some(s) = store().await else { return };
    let donor_id = donor(&s).await;
    let current = s.donor(donor_id).await.unwrap();
    let new = NewDonor {
        is_organization: current.is_organization,
        full_name: "اسم مصحح".into(),
        national_id: current.national_id.clone(),
        phone: Some("0500000000".into()),
        email: None,
        country: current.country.clone(),
        preferred_language: "ar".into(),
        notes: None,
    };
    assert_eq!(s.update_donor(donor_id, &new, "tester").await.unwrap().full_name, "اسم مصحح");
    let gift = s.record_donation(&donation(donor_id, Some(DonationKind::Sadaqah), dec!(20), "x"), "tester").await.unwrap();
    s.update_donation(gift.donation_id, &yusr_infra::edits::DonationEdit { payment_ref: Some("TRX-1".into()), note: None }, "tester").await.unwrap();
    let missing = s.update_donor(Uuid::new_v4(), &new, "tester").await;
    assert!(matches!(missing, Err(InfraError::NotFound(_))));
    let log = yusr_infra::audit::list(&s.pool, Some("donor"), 10).await.unwrap();
    assert!(log.iter().any(|r| r.action == "update"));
}
