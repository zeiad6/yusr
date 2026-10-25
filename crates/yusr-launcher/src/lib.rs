//! Self-contained Yusr for the desktop: everything it needs runs on this machine.
//!
//! - A private PostgreSQL cluster in the user's data folder, started with the bundled
//!   binaries through `pg_ctl` (which drops administrator rights on Windows, as PostgreSQL
//!   requires) on a free loopback port, and stopped cleanly on exit.
//! - The bundled llama.cpp `llama-server` with the bundled model, for the advisor.
//! - The Yusr server itself, in-process, serving the web interface on loopback only.
//!
//! Nothing listens on a network interface, so no firewall prompt appears and nothing is
//! reachable from other machines.

pub mod paths;
pub mod postgres;
pub mod process;

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Child;

use base64::Engine;
use rand::RngCore;
use yusr_server::advisor::AdvisorSettings;

pub use postgres::Postgres;

/// Where the bundled pieces are, and where data goes.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Writable per-user folder (database, secrets, backups, logs).
    pub data: PathBuf,
    /// PostgreSQL `bin` folder holding `initdb` and `pg_ctl`.
    pub pg_bin: PathBuf,
    /// The built web interface (index.html and assets).
    pub ui_dir: PathBuf,
    /// `llama-server` and the GGUF model; the advisor is off when either is missing.
    pub llama_server: Option<PathBuf>,
    pub model: Option<PathBuf>,
}

/// A running installation. Dropping it without [`Running::shutdown`] leaves PostgreSQL
/// running until the next start, which stops a stale server before starting again.
pub struct Running {
    pub url: String,
    /// Set on the very first start: the generated administrator password, shown once.
    pub first_admin_password: Option<String>,
    pg: Postgres,
    llama: Option<Child>,
    server: tokio::task::JoinHandle<()>,
    data: PathBuf,
}

impl Running {
    pub fn shutdown(mut self) {
        self.server.abort();
        if let Some(mut child) = self.llama.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(llama_pid_file(&self.data));
        if let Err(e) = self.pg.stop() {
            tracing::warn!(error = %e, "PostgreSQL did not stop cleanly");
        }
    }
}

/// The model's name on the local endpoint (`--alias`), so settings never depend on file names.
pub const MODEL_ALIAS: &str = "yusr-advisor";
const DB_NAME: &str = "yusr";
const DB_USER: &str = "yusr";
const ADMIN_USERNAME: &str = "admin";

/// A port the OS reports free on loopback. Another process could take it before we bind,
/// so callers bind immediately and the next start simply picks another.
pub fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?.local_addr()?.port())
}

/// Random URL-safe text of `bytes` random bytes.
pub fn random_secret(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Reads a secret kept in the data folder, creating it on first use.
pub fn stored_secret(path: &Path, bytes: usize) -> std::io::Result<String> {
    if let Ok(s) = std::fs::read_to_string(path) {
        if !s.trim().is_empty() {
            return Ok(s.trim().to_owned());
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let s = random_secret(bytes);
    std::fs::write(path, &s)?;
    Ok(s)
}

fn llama_pid_file(data: &Path) -> PathBuf {
    data.join("run").join("llama.pid")
}

/// Threads for the model: half the cores (the UI and database need the rest), at most 8.
fn model_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 8))
}

pub fn llama_args(model: &Path, port: u16) -> Vec<String> {
    vec![
        "-m".into(),
        model.display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--alias".into(),
        MODEL_ALIAS.into(),
        "--ctx-size".into(),
        "8192".into(),
        "--threads".into(),
        model_threads().to_string(),
        // Chat templates (and the enable_thinking switch the server sends) need Jinja.
        "--jinja".into(),
    ]
}

/// Starts everything and returns once the interface is being served.
pub async fn start(layout: Layout) -> Result<Running, String> {
    let data = &layout.data;
    for dir in ["secrets", "logs", "backups", "run"] {
        std::fs::create_dir_all(data.join(dir)).map_err(|e| format!("{}: {e}", data.join(dir).display()))?;
    }
    let db_password = stored_secret(&data.join("secrets").join("db.key"), 24).map_err(|e| e.to_string())?;
    let jwt_secret = stored_secret(&data.join("secrets").join("jwt.key"), 48).map_err(|e| e.to_string())?;

    let pg = Postgres::new(layout.pg_bin.clone(), data.join("pgdata"), data.join("logs").join("postgres.log"), free_port().map_err(|e| e.to_string())?);
    let pg = tokio::task::spawn_blocking(move || {
        pg.init_if_needed(DB_USER, &db_password)?;
        pg.start()?;
        Ok::<_, String>((pg, db_password))
    })
    .await
    .map_err(|e| e.to_string())??;
    let (pg, db_password) = pg;
    let database_url = pg.url(DB_USER, &db_password, DB_NAME);
    postgres::create_database(&pg.url(DB_USER, &db_password, "postgres"), DB_NAME).await?;

    let llama = match (&layout.llama_server, &layout.model) {
        (Some(bin), Some(model)) if bin.exists() && model.exists() => {
            process::kill_stale(&llama_pid_file(data));
            let port = free_port().map_err(|e| e.to_string())?;
            let log = std::fs::File::create(data.join("logs").join("llama-server.log")).map_err(|e| e.to_string())?;
            match process::background(bin, &llama_args(model, port), log) {
                Ok(child) => {
                    let _ = std::fs::write(llama_pid_file(data), child.id().to_string());
                    Some((child, port))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "advisor model server did not start");
                    None
                }
            }
        }
        _ => None,
    };

    let first_admin_password = postgres::has_no_users(&database_url).await?.then(|| random_secret(12));
    if let Some(pw) = &first_admin_password {
        // Shown once in the app; this copy lets the owner find it again on this machine.
        let note = format!("Yusr first sign-in\nusername: {ADMIN_USERNAME}\npassword: {pw}\nChange it from Settings after signing in.\n");
        let _ = std::fs::write(data.join("first-sign-in.txt"), note);
    }

    let port = free_port().map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:{port}");
    let cfg = yusr_server::Config {
        database_url,
        bind: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        jwt_secret: jwt_secret.into_bytes(),
        public_url: url.clone(),
        ui_dir: Some(layout.ui_dir.display().to_string()),
        zatca_key_file: None,
        zatca_cert_file: None,
        bootstrap_admin: first_admin_password.clone().map(|pw| (ADMIN_USERNAME.to_owned(), pw)),
        organization: default_organization(),
        backup_dir: Some(data.join("backups").display().to_string()),
    };
    let mut state = yusr_server::build_state(&cfg).await?;
    state.advisor = AdvisorSettings {
        enabled: llama.is_some(),
        url: llama.as_ref().map_or_else(|| "http://127.0.0.1:11434/v1".into(), |(_, p)| format!("http://127.0.0.1:{p}/v1")),
        model: MODEL_ALIAS.into(),
    };
    yusr_server::backups::spawn_daily(state.store.clone(), data.join("backups"));
    let listener = tokio::net::TcpListener::bind(cfg.bind).await.map_err(|e| format!("bind {}: {e}", cfg.bind))?;
    let app = yusr_server::router(state, cfg.ui_dir.as_deref());
    let server = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await {
            tracing::error!(error = %e, "server stopped");
        }
    });
    Ok(Running { url, first_admin_password, pg, llama: llama.map(|(c, _)| c), server, data: data.clone() })
}

fn default_organization() -> yusr_domain::receipt::Organization {
    yusr_domain::receipt::Organization {
        name_ar: "جمعيتي الخيرية".into(),
        name_en: "My Charity".into(),
        license_no: String::new(),
        vat_number: None,
        address: String::new(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_random_and_persisted() {
        assert_ne!(random_secret(24), random_secret(24));
        assert_eq!(random_secret(24).len(), 32);
        let dir = std::env::temp_dir().join(format!("yusr-launcher-{}", random_secret(6)));
        let p = dir.join("secrets").join("k");
        let a = stored_secret(&p, 16).unwrap();
        assert_eq!(stored_secret(&p, 16).unwrap(), a);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ports_are_loopback_and_free() {
        let p = free_port().unwrap();
        assert!(p > 0);
        TcpListener::bind((Ipv4Addr::LOCALHOST, p)).unwrap();
    }

    #[test]
    fn model_server_stays_on_loopback() {
        let args = llama_args(Path::new("m.gguf"), 9000);
        let host = args.iter().position(|a| a == "--host").unwrap();
        assert_eq!(args[host + 1], "127.0.0.1");
        assert!(args.contains(&MODEL_ALIAS.to_owned()) && args.contains(&"--jinja".to_owned()));
    }
}
