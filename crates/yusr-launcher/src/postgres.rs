//! A private PostgreSQL cluster managed through `initdb` and `pg_ctl`.

use std::path::PathBuf;

use sqlx::Connection;

use crate::process;

/// How long `pg_ctl` waits for the server to start or stop.
const WAIT_SECONDS: &str = "90";

#[derive(Debug, Clone)]
pub struct Postgres {
    bin: PathBuf,
    data: PathBuf,
    log: PathBuf,
    pub port: u16,
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() }
}

impl Postgres {
    pub fn new(bin: PathBuf, data: PathBuf, log: PathBuf, port: u16) -> Self {
        Self { bin, data, log, port }
    }

    fn tool(&self, name: &str) -> PathBuf {
        self.bin.join(exe(name))
    }

    pub fn url(&self, user: &str, password: &str, db: &str) -> String {
        format!("postgres://{user}:{password}@127.0.0.1:{}/{db}", self.port)
    }

    pub fn initdb_args(&self, user: &str, pwfile: &std::path::Path) -> Vec<String> {
        vec![
            "-D".into(),
            self.data.display().to_string(),
            "-U".into(),
            user.into(),
            format!("--pwfile={}", pwfile.display()),
            "--auth=scram-sha-256".into(),
            "-E".into(),
            "UTF8".into(),
            // The OS locale (often Arabic on these machines) is not needed by Yusr and has
            // broken initdb on Windows; the C locale sorts bytes and always exists.
            "--no-locale".into(),
        ]
    }

    pub fn start_args(&self) -> Vec<String> {
        vec![
            "start".into(),
            "-D".into(),
            self.data.display().to_string(),
            "-l".into(),
            self.log.display().to_string(),
            "-w".into(),
            "-t".into(),
            WAIT_SECONDS.into(),
            "-o".into(),
            format!("-p {} -c listen_addresses=127.0.0.1", self.port),
        ]
    }

    /// Creates the cluster on first run. The password file is removed straight after.
    pub fn init_if_needed(&self, user: &str, password: &str) -> Result<(), String> {
        if self.data.join("PG_VERSION").exists() {
            return Ok(());
        }
        if self.data.exists() {
            // A half-created cluster from an interrupted first run: initdb needs an empty folder.
            std::fs::remove_dir_all(&self.data).map_err(|e| format!("{}: {e}", self.data.display()))?;
        }
        let pwfile = self.data.with_extension("pw");
        std::fs::write(&pwfile, password).map_err(|e| e.to_string())?;
        let result = process::run(&self.tool("initdb"), &self.initdb_args(user, &pwfile));
        let _ = std::fs::remove_file(&pwfile);
        result.map(|_| ())
    }

    fn running(&self) -> bool {
        process::run(&self.tool("pg_ctl"), &["status".into(), "-D".into(), self.data.display().to_string()]).is_ok()
    }

    /// Starts the server, first stopping one a crashed session left running (it would hold
    /// the data folder on an old port).
    pub fn start(&self) -> Result<(), String> {
        if self.running() {
            self.stop()?;
        }
        process::run(&self.tool("pg_ctl"), &self.start_args()).map(|_| ()).map_err(|e| {
            let log = std::fs::read_to_string(&self.log).unwrap_or_default();
            let tail: String = log.lines().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            format!("{e}\n{tail}")
        })
    }

    pub fn stop(&self) -> Result<(), String> {
        let args = ["stop", "-D", &self.data.display().to_string(), "-m", "fast", "-w", "-t", WAIT_SECONDS].map(String::from);
        process::run(&self.tool("pg_ctl"), &args).map(|_| ())
    }
}

/// `CREATE DATABASE` unless it exists (it cannot run inside a transaction or take parameters).
pub async fn create_database(admin_url: &str, name: &str) -> Result<(), String> {
    assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "database names are fixed identifiers");
    let mut conn = sqlx::PgConnection::connect(admin_url).await.map_err(|e| e.to_string())?;
    let exists: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM pg_database WHERE datname = $1").bind(name).fetch_optional(&mut conn).await.map_err(|e| e.to_string())?;
    if exists.is_none() {
        sqlx::query(&format!("CREATE DATABASE {name}")).execute(&mut conn).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// True before migrations ran (no `users` table yet) or while no account exists.
pub async fn has_no_users(url: &str) -> Result<bool, String> {
    let mut conn = sqlx::PgConnection::connect(url).await.map_err(|e| e.to_string())?;
    let (table,): (Option<String>,) = sqlx::query_as("SELECT to_regclass('public.users')::text").fetch_one(&mut conn).await.map_err(|e| e.to_string())?;
    if table.is_none() {
        return Ok(true);
    }
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(&mut conn).await.map_err(|e| e.to_string())?;
    Ok(n == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg() -> Postgres {
        Postgres::new("bin".into(), "data".into(), "pg.log".into(), 5999)
    }

    #[test]
    fn listens_on_loopback_only_with_a_c_locale() {
        let start = pg().start_args().join(" ");
        assert!(start.contains("listen_addresses=127.0.0.1") && start.contains("-p 5999"));
        let init = pg().initdb_args("yusr", std::path::Path::new("p")).join(" ");
        assert!(init.contains("--no-locale") && init.contains("scram-sha-256") && init.contains("UTF8"));
        assert_eq!(pg().url("u", "p", "d"), "postgres://u:p@127.0.0.1:5999/d");
    }

    /// Full lifecycle against real binaries: `YUSR_TEST_PG_BIN=<postgres bin dir> cargo test -- --ignored`.
    /// Run on the Windows CI runner with the binaries that ship in the installer.
    #[tokio::test]
    #[ignore = "needs PostgreSQL binaries in YUSR_TEST_PG_BIN"]
    async fn cluster_lifecycle() {
        let bin = PathBuf::from(std::env::var("YUSR_TEST_PG_BIN").expect("YUSR_TEST_PG_BIN"));
        let root = std::env::temp_dir().join(format!("yusr-pg-{}", crate::random_secret(6)));
        std::fs::create_dir_all(&root).unwrap();
        let pg = Postgres::new(bin, root.join("pgdata"), root.join("pg.log"), crate::free_port().unwrap());
        pg.init_if_needed("yusr", "s3cret-pw").unwrap();
        pg.start().unwrap();
        create_database(&pg.url("yusr", "s3cret-pw", "postgres"), "yusr").await.unwrap();
        create_database(&pg.url("yusr", "s3cret-pw", "postgres"), "yusr").await.unwrap();
        assert!(has_no_users(&pg.url("yusr", "s3cret-pw", "yusr")).await.unwrap());
        // A second start (as after a crash) stops the old server first instead of failing.
        pg.start().unwrap();
        pg.stop().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
