use thiserror::Error;
use yusr_domain::DomainError;

/// Unique keys that serialise an append-only chain; a violation means another
/// transaction appended first and ours must re-read the head and retry.
const CHAIN_KEYS: [&str; 2] = ["audit_log_pkey", "zatca_invoices_icv_key"];

#[derive(Debug, Error)]
pub enum InfraError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("{0} not found")]
    NotFound(&'static str),
    /// A state clash; the code tells clients which one without parsing the text.
    #[error("conflict: {1}")]
    Conflict(&'static str, String),
    #[error("configuration: {0}")]
    Config(String),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

impl InfraError {
    fn sqlstate(&self) -> Option<String> {
        match self {
            Self::Db(sqlx::Error::Database(db)) => db.code().map(|c| c.into_owned()),
            _ => None,
        }
    }

    /// Serialization failure (40001), deadlock (40P01), or two transactions racing to append
    /// to the same chain (audit log, ZATCA invoice counter): safe to run the transaction again.
    pub fn is_retryable(&self) -> bool {
        if let Self::Db(sqlx::Error::Database(db)) = self {
            if db.code().as_deref() == Some("23505") && db.constraint().is_some_and(|c| CHAIN_KEYS.contains(&c)) {
                return true;
            }
        }
        matches!(self.sqlstate().as_deref(), Some("40001" | "40P01"))
    }

    /// A database rule (CHECK, unique, foreign key or one of our triggers) rejected the
    /// write — a client error, not a server fault.
    pub fn is_constraint_violation(&self) -> bool {
        matches!(self.sqlstate().as_deref(), Some(c) if c.starts_with("23") || c == "42501")
    }

    /// Client-facing code and the violated constraint's name for a rejected write. The full
    /// database message is not exposed: it names tables and columns.
    pub fn violation(&self) -> Option<(&'static str, Option<String>)> {
        let Self::Db(sqlx::Error::Database(db)) = self else { return None };
        let code = match db.code().as_deref() {
            Some("23505") => "duplicate_value",
            Some("23503") => "invalid_reference",
            Some("42501") => "write_once",
            Some(c) if c.starts_with("23") => "constraint_violation",
            _ => return None,
        };
        Some((code, db.constraint().map(str::to_owned)))
    }
}
