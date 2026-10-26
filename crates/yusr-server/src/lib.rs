//! Yusr server mode: the same core as the desktop app behind an Axum REST API.

pub mod advisor;
pub mod api;
pub mod auth;
pub mod backups;
pub mod error;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use yusr_domain::receipt::Organization;
use yusr_domain::zatca::sign::Credentials;
use yusr_infra::Store;
use yusr_payments::Registry;

/// Shared state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub payments: Registry,
    pub jwt_secret: Arc<Vec<u8>>,
    pub zatca: Option<Arc<Credentials>>,
    /// Public base URL, used to build gateway webhook and return URLs.
    pub public_url: String,
    /// Defaults for the advisor's model endpoint; settings stored in the database win.
    pub advisor: advisor::AdvisorSettings,
    /// Set by the desktop package that downloads its model; `None` everywhere else.
    pub local_model: Option<Arc<dyn advisor::LocalModel>>,
    /// Where automatic and pre-restore backups go, when set.
    pub backup_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub jwt_secret: Vec<u8>,
    pub public_url: String,
    pub ui_dir: Option<String>,
    pub zatca_key_file: Option<String>,
    pub zatca_cert_file: Option<String>,
    pub bootstrap_admin: Option<(String, String)>,
    pub organization: Organization,
    pub backup_dir: Option<String>,
}

/// Minimum JWT secret length: 256 bits, the HS256 key size.
pub const MIN_SECRET_BYTES: usize = 32;

impl Config {
    /// Reads `DATABASE_URL`, `YUSR_JWT_SECRET` (≥ 32 bytes), `YUSR_BIND`, `YUSR_PUBLIC_URL`,
    /// `YUSR_UI_DIR`, `YUSR_ZATCA_KEY_FILE`, `YUSR_ZATCA_CERT_FILE`,
    /// `YUSR_ADMIN_USERNAME` + `YUSR_ADMIN_PASSWORD` (first run only), `YUSR_BACKUP_DIR`,
    /// `YUSR_ADVISOR_URL` / `YUSR_ADVISOR_MODEL` and `YUSR_ORG_*`.
    pub fn from_env() -> Result<Self, String> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let jwt_secret = var("YUSR_JWT_SECRET").ok_or("YUSR_JWT_SECRET is required")?.into_bytes();
        if jwt_secret.len() < MIN_SECRET_BYTES {
            return Err(format!("YUSR_JWT_SECRET must be at least {MIN_SECRET_BYTES} bytes"));
        }
        Ok(Self {
            database_url: var("DATABASE_URL").ok_or("DATABASE_URL is required")?,
            bind: var("YUSR_BIND").unwrap_or_else(|| "127.0.0.1:8080".into()).parse().map_err(|e| format!("YUSR_BIND: {e}"))?,
            jwt_secret,
            public_url: var("YUSR_PUBLIC_URL").unwrap_or_else(|| "http://127.0.0.1:8080".into()).trim_end_matches('/').to_owned(),
            ui_dir: var("YUSR_UI_DIR"),
            zatca_key_file: var("YUSR_ZATCA_KEY_FILE"),
            zatca_cert_file: var("YUSR_ZATCA_CERT_FILE"),
            bootstrap_admin: var("YUSR_ADMIN_USERNAME").zip(var("YUSR_ADMIN_PASSWORD")),
            backup_dir: var("YUSR_BACKUP_DIR"),
            organization: Organization {
                name_ar: var("YUSR_ORG_NAME_AR").unwrap_or_else(|| "جمعيتي الخيرية".into()),
                name_en: var("YUSR_ORG_NAME_EN").unwrap_or_else(|| "My Charity".into()),
                license_no: var("YUSR_ORG_LICENSE").unwrap_or_default(),
                vat_number: var("YUSR_ORG_VAT"),
                address: var("YUSR_ORG_ADDRESS").unwrap_or_default(),
                tax_statement_ar: "لم تُقدَّم أي سلع أو خدمات مقابل هذا التبرع.".into(),
                tax_statement_en: "No goods or services were provided in exchange for this donation.".into(),
                logo_data_url: None,
                stamp_data_url: None,
                org_type: None,
                registration_no: None,
                phone: None,
                email: None,
                website: None,
                fiscal_year_start: 1,
            },
        })
    }
}

/// Opens the store, creates the first admin when there are no users, and loads the
/// optional ZATCA credentials.
pub async fn build_state(cfg: &Config) -> Result<AppState, String> {
    let store = Store::open(&cfg.database_url, &cfg.organization).await.map_err(|e| e.to_string())?;
    let (users,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(&store.pool).await.map_err(|e| e.to_string())?;
    if users == 0 {
        let (name, password) = cfg
            .bootstrap_admin
            .as_ref()
            .ok_or("no users yet: set YUSR_ADMIN_USERNAME and YUSR_ADMIN_PASSWORD to create the first admin")?;
        auth::create_user(&store, name, "Administrator", password, auth::Role::Admin, "system:bootstrap").await.map_err(|e| e.message)?;
        tracing::info!(username = %name, "created first admin user");
    }
    let zatca = match (&cfg.zatca_key_file, &cfg.zatca_cert_file) {
        (Some(k), Some(c)) => {
            let key = std::fs::read_to_string(k).map_err(|e| format!("{k}: {e}"))?;
            let cert = std::fs::read_to_string(c).map_err(|e| format!("{c}: {e}"))?;
            Some(Arc::new(Credentials::from_pem(&key, &cert).map_err(|e| e.to_string())?))
        }
        _ => None,
    };
    Ok(AppState {
        store,
        payments: Registry::from_env(),
        jwt_secret: Arc::new(cfg.jwt_secret.clone()),
        zatca,
        public_url: cfg.public_url.clone(),
        advisor: advisor::AdvisorSettings::from_env(),
        local_model: None,
        backup_dir: cfg.backup_dir.as_ref().map(std::path::PathBuf::from),
    })
}

/// Scripts, styles, fonts and frames come only from this origin; the token in localStorage is
/// therefore out of reach of injected third-party script. `unsafe-inline` styles are needed by
/// React style props and the printable documents' embedded <style>.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
font-src 'self'; connect-src 'self'; frame-src 'self' blob:; worker-src 'self'; manifest-src 'self'; object-src 'none'; \
base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

/// A slow or stuck request is cut off instead of holding a worker and a pool connection.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

pub fn router(state: AppState, ui_dir: Option<&str>) -> Router {
    use axum::http::{HeaderName, HeaderValue, header};
    use tower_http::set_header::SetResponseHeaderLayer;
    let https = state.public_url.starts_with("https://");
    // Body limits are set per route group in `api::routes`.
    let mut app = api::routes().with_state(state);
    if let Some(dir) = ui_dir {
        // Single-page app: unknown paths fall back to index.html.
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html"))));
    }
    // `if_not_present` lets a handler set a stricter policy (archive downloads use `sandbox`).
    let header = |name: HeaderName, value: &'static str| SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value));
    let mut app = app
        .layer(header(header::CONTENT_SECURITY_POLICY, CSP))
        .layer(header(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(header(header::X_FRAME_OPTIONS, "DENY"))
        .layer(header(header::REFERRER_POLICY, "no-referrer"))
        .layer(header(HeaderName::from_static("permissions-policy"), "camera=(), microphone=(), geolocation=(), payment=()"))
        .layer(header(HeaderName::from_static("cross-origin-opener-policy"), "same-origin"));
    if https {
        app = app.layer(header(header::STRICT_TRANSPORT_SECURITY, "max-age=31536000; includeSubDomains"));
    }
    app.layer(tower_http::timeout::TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
        // A panicking handler becomes a 500 response instead of a dropped connection.
        .layer(tower_http::catch_panic::CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
}

/// Runs the headless server until Ctrl-C / SIGTERM.
pub async fn run(cfg: Config) -> Result<(), String> {
    let state = build_state(&cfg).await?;
    if state.payments.ids().is_empty() {
        tracing::info!("no payment gateways configured");
    } else {
        tracing::info!(gateways = ?state.payments.ids(), "payment gateways enabled");
    }
    if let Some(dir) = &state.backup_dir {
        backups::spawn_daily(state.store.clone(), dir.clone());
    }
    let app = router(state, cfg.ui_dir.as_deref());
    let listener = tokio::net::TcpListener::bind(cfg.bind).await.map_err(|e| format!("bind {}: {e}", cfg.bind))?;
    tracing::info!(addr = %cfg.bind, "Yusr server listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            {
                let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal handler");
                tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
            }
            #[cfg(not(unix))]
            let _ = ctrl_c.await;
        })
        .await
        .map_err(|e| e.to_string())
}
