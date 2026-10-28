//! The donor transparency report against a real PostgreSQL, through the HTTP API.
//! Set `YUSR_TEST_DATABASE_URL` (a URL whose user may CREATE DATABASE); without it the test is skipped.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::Connection;
use tower::ServiceExt;
use uuid::Uuid;

fn org() -> yusr_domain::receipt::Organization {
    yusr_domain::receipt::Organization {
        name_ar: "جمعية الاختبار".into(),
        name_en: "Test Charity".into(),
        license_no: "999".into(),
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

async fn send(app: &axum::Router, method: &str, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path).header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    if method == "POST" {
        req = req.header("idempotency-key", Uuid::new_v4().simple().to_string());
    }
    let res = app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn transparency_report_totals_the_period_in_base_currency() {
    let Ok(admin) = std::env::var("YUSR_TEST_DATABASE_URL") else {
        eprintln!("YUSR_TEST_DATABASE_URL not set; skipping database test");
        return;
    };
    let name = format!("yusr_t_{}", Uuid::new_v4().simple());
    let mut conn = sqlx::PgConnection::connect(&admin).await.expect("connect to admin database");
    sqlx::query(&format!("CREATE DATABASE {name}")).execute(&mut conn).await.expect("create test database");
    let (prefix, _) = admin.rsplit_once('/').expect("URL has a database path");
    let cfg = yusr_server::Config {
        database_url: format!("{prefix}/{name}"),
        bind: "127.0.0.1:0".parse().unwrap(),
        jwt_secret: vec![7; yusr_server::MIN_SECRET_BYTES],
        public_url: "http://127.0.0.1".into(),
        ui_dir: None,
        zatca_key_file: None,
        zatca_cert_file: None,
        bootstrap_admin: Some(("admin".into(), "admin-password-1".into())),
        organization: org(),
        backup_dir: None,
    };
    let state = yusr_server::build_state(&cfg).await.expect("state");
    let pool = state.store.pool.clone();
    let app = yusr_server::router(state, None);

    let (_, login) = send(&app, "POST", "/api/auth/login", None, json!({ "username": "admin", "password": "admin-password-1" })).await;
    let token = login["token"].as_str().expect("token").to_owned();
    let t = Some(token.as_str());
    let (_, donor) = send(&app, "POST", "/api/donors", t, json!({ "full_name": "متبرع", "preferred_language": "ar" })).await;
    let donor = donor["id"].as_str().expect("donor id").to_owned();
    send(&app, "PUT", "/api/fx-rates", t, json!({ "currency": "USD", "date": "2026-07-01", "rate": "3.75" })).await;
    for (date, amount, currency) in [("2026-07-15", "1000", "SAR"), ("2026-08-01", "100", "USD"), ("2026-01-10", "5000", "SAR")] {
        let (status, body) = send(&app, "POST", "/api/donations", t, json!({
            "donor_id": donor, "donation_date": date, "kind": "general", "amount": amount, "currency": currency, "payment_method": "cash"
        })).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let (status, report) = send(&app, "POST", "/api/reports/transparency", t, json!({
        "title": "Q3", "period_start": "2026-07-01", "period_end": "2026-09-30"
    })).await;
    drop(app);
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)")).execute(&mut conn).await.expect("drop test database");

    assert_eq!(status, StatusCode::OK, "{report}");
    // 1000 SAR + 100 USD at 3.75; the January donation is outside the period.
    let total: f64 = report["total_donations"].as_str().expect("total").parse().unwrap();
    assert_eq!(total, 1375.0);
}
