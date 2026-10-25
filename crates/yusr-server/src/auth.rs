//! Authentication and role-based access control.
//!
//! - Passwords: Argon2id (19 MiB, t=2, p=1 — the OWASP baseline), PHC string format.
//! - Sessions: HS256 JWTs carrying the user's `token_version`. Every request re-checks the
//!   version and the account's active flag in the database, so logout, password change and
//!   deactivation revoke tokens at once (stateful tokens).
//! - Lockout: five consecutive failures lock the account for fifteen minutes.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::AppState;
use crate::error::{ApiError, ApiResult};

pub const TOKEN_TTL_HOURS: i64 = 8;
const MAX_FAILED_LOGINS: i32 = 5;
const LOCKOUT_MINUTES: i32 = 15;
pub const MIN_PASSWORD_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Accountant,
    CaseWorker,
    Auditor,
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Accountant => "accountant",
            Self::CaseWorker => "case_worker",
            Self::Auditor => "auditor",
            Self::Viewer => "viewer",
        }
    }
}

/// Every protected operation names one permission. Roles grant default sets; an admin can
/// replace a user's set with an explicit list (`users.permissions`).
macro_rules! permissions {
    ($($variant:ident => $name:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum Permission { $($variant,)* }

        impl Permission {
            pub const ALL: &'static [Permission] = &[$(Permission::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $name,)* }
            }

            pub fn parse(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)* _ => None }
            }
        }
    };
}

permissions! {
    ReportsRead => "reports.read",
    LedgerRead => "ledger.read",
    LedgerPost => "ledger.post",
    LedgerReverse => "ledger.reverse",
    AccountsManage => "accounts.manage",
    FundsManage => "funds.manage",
    BudgetsManage => "budgets.manage",
    FxManage => "fx.manage",
    SettingsManage => "settings.manage",
    DonorsRead => "donors.read",
    DonorsManage => "donors.manage",
    DonationsRead => "donations.read",
    DonationsRecord => "donations.record",
    DonationsEdit => "donations.edit",
    PaymentsCheckout => "payments.checkout",
    SponsorshipsRead => "sponsorships.read",
    SponsorshipsManage => "sponsorships.manage",
    SponsorshipsPay => "sponsorships.pay",
    CasesRead => "cases.read",
    CasesManage => "cases.manage",
    AidDistribute => "aid.distribute",
    VouchersRedeem => "vouchers.redeem",
    VolunteersRead => "volunteers.read",
    VolunteersManage => "volunteers.manage",
    VolunteersHours => "volunteers.hours",
    ZakatCalculate => "zakat.calculate",
    InvoicesRead => "invoices.read",
    InvoicesIssue => "invoices.issue",
    ArchiveRead => "archive.read",
    ArchiveWrite => "archive.write",
    ArchiveVoid => "archive.void",
    // HR, financial, legal and contract documents in the archive.
    ArchiveConfidential => "archive.confidential",
    LettersRead => "letters.read",
    LettersWrite => "letters.write",
    LettersIssue => "letters.issue",
    AuditRead => "audit.read",
    UsersManage => "users.manage",
    BackupCreate => "backup.create",
    BackupRestore => "backup.restore",
    AdvisorUse => "advisor.use",
    AdvisorManage => "advisor.manage",
}

/// A set of permissions as a bit mask (one bit per [`Permission`] discriminant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PermissionSet(u64);

const _: () = assert!(Permission::ALL.len() <= 64, "PermissionSet holds at most 64 permissions");

impl PermissionSet {
    pub fn all() -> Self {
        Self::of(Permission::ALL)
    }

    pub fn of(ps: &[Permission]) -> Self {
        Self(ps.iter().fold(0, |m, p| m | 1 << *p as u8))
    }

    /// Unknown names are dropped: a permission removed in a later version simply stops granting.
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Self {
        Self::of(&names.iter().filter_map(|n| Permission::parse(n.as_ref())).collect::<Vec<_>>())
    }

    pub fn contains(self, p: Permission) -> bool {
        self.0 & (1 << p as u8) != 0
    }

    pub fn names(self) -> Vec<&'static str> {
        Permission::ALL.iter().filter(|p| self.contains(**p)).map(|p| p.as_str()).collect()
    }
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Admin, Role::Accountant, Role::CaseWorker, Role::Auditor, Role::Viewer];

    pub fn defaults(self) -> PermissionSet {
        use Permission::*;
        match self {
            Role::Admin => PermissionSet::all(),
            Role::Accountant => PermissionSet::of(
                &Permission::ALL
                    .iter()
                    .copied()
                    .filter(|p| !matches!(p, UsersManage | CasesManage | VolunteersManage | BackupRestore | AdvisorManage))
                    .collect::<Vec<_>>(),
            ),
            // Case workers see beneficiaries and hand out aid, never the books.
            Role::CaseWorker => PermissionSet::of(&[
                ReportsRead, CasesRead, CasesManage, AidDistribute, VouchersRedeem, DonorsRead, DonationsRead, SponsorshipsRead,
                VolunteersRead, VolunteersManage, VolunteersHours, ZakatCalculate, ArchiveRead, ArchiveWrite, LettersRead,
                LettersWrite, AdvisorUse,
            ]),
            // Auditors read everything and change nothing.
            Role::Auditor => PermissionSet::of(&[
                ReportsRead, LedgerRead, DonorsRead, DonationsRead, SponsorshipsRead, CasesRead, VolunteersRead, ZakatCalculate,
                InvoicesRead, ArchiveRead, ArchiveConfidential, LettersRead, AuditRead, AdvisorUse,
            ]),
            Role::Viewer => PermissionSet::of(&[ReportsRead, ZakatCalculate, AdvisorUse]),
        }
    }

    /// The set a user actually has: admins always hold everything so nobody can lock the
    /// organisation out of user management.
    pub fn effective(self, custom: Option<&[String]>) -> PermissionSet {
        match (self, custom) {
            (Role::Admin, _) | (_, None) => self.defaults(),
            (_, Some(names)) => PermissionSet::from_names(names),
        }
    }
}

pub fn allows(role: Role, p: Permission) -> bool {
    role.defaults().contains(p)
}

fn argon2() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(19 * 1024, 2, 1, None).expect("valid params"))
}

pub fn hash_password(password: &str) -> ApiResult<String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(ApiError::new(axum::http::StatusCode::BAD_REQUEST, "password_too_short", format!("password must be at least {MIN_PASSWORD_LEN} characters")));
    }
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::bad_request(format!("cannot hash password: {e}")))
}

pub fn verify_password(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| argon2().verify_password(password.as_bytes(), &h).is_ok())
}

/// Each Argon2 run takes 19 MiB and tens of milliseconds of CPU. Running it on the async
/// workers would stall every request, and unbounded parallel logins could exhaust memory, so
/// hashing happens on the blocking pool behind this many slots.
const HASH_SLOTS: usize = 2;
static HASHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(HASH_SLOTS);

async fn off_runtime<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    let _slot = HASHING.acquire().await.map_err(|_| ApiError::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, "busy", "shutting down"))?;
    tokio::task::spawn_blocking(f).await.map_err(|_| ApiError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error"))
}

pub async fn hash_password_async(password: &str) -> ApiResult<String> {
    let password = password.to_owned();
    off_runtime(move || hash_password(&password)).await?
}

pub async fn verify_password_async(password: &str, phc: &str) -> bool {
    let (password, phc) = (password.to_owned(), phc.to_owned());
    off_runtime(move || verify_password(&password, &phc)).await.unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    pub sub: Uuid,
    pub username: String,
    pub role: Role,
    /// Must equal `users.token_version` for the token to be accepted.
    pub ver: i32,
    pub iat: i64,
    pub exp: i64,
}

fn sign(secret: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("any key length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

pub fn issue_token(secret: &[u8], claims: &Claims) -> String {
    let header = B64.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = B64.encode(serde_json::to_vec(claims).expect("claims serialize"));
    let data = format!("{header}.{payload}");
    let sig = B64.encode(sign(secret, &data));
    format!("{data}.{sig}")
}

/// Verifies signature, algorithm and expiry. Database checks happen in [`Auth`].
pub fn decode_token(secret: &[u8], token: &str) -> Option<Claims> {
    let mut parts = token.split('.');
    let (h, p, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let header: serde_json::Value = serde_json::from_slice(&B64.decode(h).ok()?).ok()?;
    // Pin the algorithm: never trust `alg` from the token (rejects `none` and RS/HS confusion).
    if header["alg"] != "HS256" {
        return None;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("any key length");
    mac.update(format!("{h}.{p}").as_bytes());
    mac.verify_slice(&B64.decode(s).ok()?).ok()?;
    let claims: Claims = serde_json::from_slice(&B64.decode(p).ok()?).ok()?;
    (claims.exp > Utc::now().timestamp()).then_some(claims)
}

/// The authenticated caller, extracted from `Authorization: Bearer <jwt>`.
#[derive(Debug, Clone)]
pub struct Auth {
    pub user_id: Uuid,
    pub username: String,
    pub role: Role,
    pub permissions: PermissionSet,
}

impl Auth {
    pub fn can(&self, p: Permission) -> bool {
        self.permissions.contains(p)
    }

    pub fn require(&self, p: Permission) -> ApiResult<()> {
        if self.can(p) { Ok(()) } else { Err(ApiError::forbidden(p.as_str())) }
    }

    /// Actor string recorded in the cryptographic audit trail, capturing both username and user_id.
    pub fn actor(&self) -> String {
        format!("user:{}:{}", self.username, self.user_id)
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(ApiError::unauthorized)?;
        let claims = decode_token(&state.jwt_secret, token).ok_or_else(ApiError::unauthorized)?;
        let row: Option<(String, bool, i32, Option<Vec<String>>)> =
            sqlx::query_as("SELECT role::text, is_active, token_version, permissions FROM users WHERE id = $1")
            .bind(claims.sub)
            .fetch_optional(&state.store.pool)
            .await
            .map_err(yusr_infra::InfraError::from)?;
        match row {
            Some((role, true, ver, custom)) if ver == claims.ver && role == claims.role.as_str() => {
                let permissions = claims.role.effective(custom.as_deref());
                Ok(Auth { user_id: claims.sub, username: claims.username, role: claims.role, permissions })
            }
            _ => Err(ApiError::unauthorized()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: i64,
    pub role: Role,
    pub full_name: String,
    pub permissions: Vec<&'static str>,
}

pub async fn login(state: &AppState, req: &LoginRequest) -> ApiResult<LoginResponse> {
    type Row = (Uuid, String, String, String, bool, i32, Option<chrono::DateTime<Utc>>, i32, Option<Vec<String>>);
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, full_name, password_hash, role::text, is_active, failed_logins, locked_until, token_version, permissions
           FROM users WHERE username = $1",
    )
    .bind(req.username.trim().to_lowercase())
    .fetch_optional(&state.store.pool)
    .await
    .map_err(yusr_infra::InfraError::from)?;
    let invalid = || ApiError::new(axum::http::StatusCode::UNAUTHORIZED, "invalid_credentials", "invalid username or password");
    let Some((id, full_name, phc, role, active, failed, locked_until, ver, custom)) = row else {
        // Hash anyway so response time does not reveal which usernames exist.
        static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let dummy = DUMMY.get_or_init(|| hash_password("timing-equaliser-not-a-password").expect("valid password"));
        let _ = verify_password_async(&req.password, dummy).await;
        return Err(invalid());
    };
    if locked_until.is_some_and(|t| t > Utc::now()) {
        return Err(ApiError::new(axum::http::StatusCode::TOO_MANY_REQUESTS, "account_locked", "too many failed attempts; try again later"));
    }
    if !verify_password_async(&req.password, &phc).await || !active {
        // One atomic increment per failure: parallel guesses cannot share a stale counter.
        state.store.record_failed_login(id, MAX_FAILED_LOGINS, LOCKOUT_MINUTES).await?;
        return Err(invalid());
    }
    if failed > 0 {
        state.store.clear_failed_logins(id).await?;
    }
    let role: Role = serde_json::from_value(serde_json::Value::String(role)).map_err(|_| ApiError::unauthorized())?;
    let now = Utc::now().timestamp();
    let claims = Claims { sub: id, username: req.username.trim().to_lowercase(), role, ver, iat: now, exp: now + TOKEN_TTL_HOURS * 3600 };
    let permissions = role.effective(custom.as_deref()).names();
    Ok(LoginResponse { token: issue_token(&state.jwt_secret, &claims), expires_at: claims.exp, role, full_name, permissions })
}

/// Creates a user and records it in the audit trail; used by the admin API and the
/// first-run bootstrap.
pub async fn create_user(store: &yusr_infra::Store, username: &str, full_name: &str, password: &str, role: Role, actor: &str) -> ApiResult<Uuid> {
    let hash = hash_password_async(password).await?;
    let username = username.trim().to_lowercase();
    let id = yusr_infra::serializable!(store, |conn| {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO users (username, full_name, password_hash, role) VALUES ($1, $2, $3, $4::user_role) RETURNING id",
        )
        .bind(&username)
        .bind(full_name)
        .bind(&hash)
        .bind(role.as_str())
        .fetch_one(&mut *conn)
        .await?;
        yusr_infra::audit::append(conn, actor, "create", "user", &id.to_string(), serde_json::json!({ "username": username, "role": role })).await?;
        Ok(id)
    })?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip_and_policy() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password("correct horse battery", &h));
        assert!(!verify_password("wrong password!!", &h));
        assert!(hash_password("short").is_err());
    }

    #[test]
    fn token_roundtrip_and_tampering() {
        let now = Utc::now().timestamp();
        let c = Claims { sub: Uuid::nil(), username: "amal".into(), role: Role::Accountant, ver: 0, iat: now, exp: now + 60 };
        let t = issue_token(b"k".repeat(32).as_slice(), &c);
        assert_eq!(decode_token(b"k".repeat(32).as_slice(), &t), Some(c.clone()));
        assert!(decode_token(b"x".repeat(32).as_slice(), &t).is_none());

        // Privilege escalation by editing the payload breaks the signature.
        let mut parts: Vec<String> = t.split('.').map(str::to_owned).collect();
        let forged = Claims { role: Role::Admin, ..c.clone() };
        parts[1] = B64.encode(serde_json::to_vec(&forged).unwrap());
        assert!(decode_token(b"k".repeat(32).as_slice(), &parts.join(".")).is_none());

        // `alg: none` is refused.
        let none = format!("{}.{}.", B64.encode(br#"{"alg":"none"}"#), parts[1]);
        assert!(decode_token(b"k".repeat(32).as_slice(), &none).is_none());

        let expired = Claims { exp: now - 1, ..c };
        assert!(decode_token(b"k".repeat(32).as_slice(), &issue_token(b"k".repeat(32).as_slice(), &expired)).is_none());
    }

    #[test]
    fn rbac_matrix() {
        assert!(allows(Role::Admin, Permission::UsersManage));
        assert!(allows(Role::Accountant, Permission::LedgerPost));
        assert!(!allows(Role::Accountant, Permission::UsersManage));
        assert!(allows(Role::CaseWorker, Permission::AidDistribute));
        assert!(!allows(Role::CaseWorker, Permission::LedgerRead));
        assert!(allows(Role::Auditor, Permission::AuditRead));
        assert!(!allows(Role::Auditor, Permission::LedgerPost));
        assert!(!allows(Role::Viewer, Permission::DonationsRead));
        assert!(!allows(Role::Accountant, Permission::BackupRestore));
    }

    #[test]
    fn permission_names_roundtrip_and_custom_sets() {
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.as_str()), Some(*p));
        }
        let custom = vec!["ledger.read".to_owned(), "no.such".to_owned()];
        let set = Role::Viewer.effective(Some(&custom));
        assert!(set.contains(Permission::LedgerRead));
        assert!(!set.contains(Permission::ReportsRead));
        assert_eq!(set.names(), vec!["ledger.read"]);
        // Admins cannot be narrowed.
        assert!(Role::Admin.effective(Some(&custom)).contains(Permission::UsersManage));
        assert_eq!(Role::Accountant.effective(None), Role::Accountant.defaults());
    }
}
