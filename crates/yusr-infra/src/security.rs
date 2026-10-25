//! Request replay protection (idempotency keys) and login throttling state.

use uuid::Uuid;

use crate::{Result, Store};

/// A pending claim older than this belongs to a request that died mid-flight; it may be retaken.
const STALE_PENDING_MINUTES: i32 = 5;
/// Completed responses are replayable for a week, long enough for any client retry.
const RETENTION_DAYS: i32 = 7;

#[derive(Debug, Clone, PartialEq)]
pub enum Claim {
    /// First time this key is seen: run the operation, then `idempotency_complete`.
    Fresh,
    /// Already completed: return the stored response unchanged.
    Replay { status: u16, body: serde_json::Value },
    /// Another request with the same key is still running.
    InProgress,
}

/// Client keys are opaque tokens, typically UUIDs.
pub fn valid_key(key: &str) -> bool {
    (8..=100).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Store {
    pub async fn idempotency_claim(&self, scope: &str, key: &str) -> Result<Claim> {
        sqlx::query(
            "DELETE FROM idempotency_keys
              WHERE created_at < now() - make_interval(days => $1)
                 OR (scope = $2 AND key = $3 AND status_code IS NULL AND created_at < now() - make_interval(mins => $4))",
        )
        .bind(RETENTION_DAYS)
        .bind(scope)
        .bind(key)
        .bind(STALE_PENDING_MINUTES)
        .execute(&self.pool)
        .await?;
        let inserted: Option<(i32,)> =
            sqlx::query_as("INSERT INTO idempotency_keys (scope, key) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING 1")
                .bind(scope)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        if inserted.is_some() {
            return Ok(Claim::Fresh);
        }
        let row: Option<(Option<i16>, Option<serde_json::Value>)> =
            sqlx::query_as("SELECT status_code, response FROM idempotency_keys WHERE scope = $1 AND key = $2")
                .bind(scope)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(match row {
            Some((Some(status), Some(body))) => Claim::Replay { status: status as u16, body },
            // Vanished between the insert and the select (pruned): treat as a new request.
            None => return Box::pin(self.idempotency_claim(scope, key)).await,
            _ => Claim::InProgress,
        })
    }

    pub async fn idempotency_complete(&self, scope: &str, key: &str, status: u16, body: &serde_json::Value) -> Result<()> {
        sqlx::query("UPDATE idempotency_keys SET status_code = $3, response = $4 WHERE scope = $1 AND key = $2")
            .bind(scope)
            .bind(key)
            .bind(status as i16)
            .bind(body)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Frees the key after a failed attempt so the client can retry with the same key.
    pub async fn idempotency_release(&self, scope: &str, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM idempotency_keys WHERE scope = $1 AND key = $2 AND status_code IS NULL")
            .bind(scope)
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Counts a failed login in one atomic statement, so parallel guesses cannot all read the
    /// same counter and slip past the lockout. Returns true when this failure locked the account.
    pub async fn record_failed_login(&self, user_id: Uuid, max_failures: i32, lock_minutes: i32) -> Result<bool> {
        let (locked,): (bool,) = sqlx::query_as(
            "UPDATE users
                SET failed_logins = CASE WHEN failed_logins + 1 >= $2 THEN 0 ELSE failed_logins + 1 END,
                    locked_until  = CASE WHEN failed_logins + 1 >= $2 THEN now() + make_interval(mins => $3) ELSE locked_until END
              WHERE id = $1
          RETURNING locked_until IS NOT NULL AND locked_until > now()",
        )
        .bind(user_id)
        .bind(max_failures)
        .bind(lock_minutes)
        .fetch_one(&self.pool)
        .await?;
        Ok(locked)
    }

    pub async fn clear_failed_logins(&self, user_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = $1").bind(user_id).execute(&self.pool).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::valid_key;

    #[test]
    fn idempotency_keys_are_opaque_tokens() {
        assert!(valid_key("3f2b9c1e-7d4a-4e8b-9a6f-2c1d0e5b7a90"));
        assert!(!valid_key("short"));
        assert!(!valid_key("has space in it"));
        assert!(!valid_key(&"x".repeat(101)));
        assert!(!valid_key("'; DROP TABLE x; --"));
    }
}
