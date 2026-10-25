//! Hash-chained audit trail (write-once, read-many).
//!
//! Each record's hash covers the previous record's hash, so altering, deleting or
//! reordering any row breaks every hash after it. PostgreSQL triggers forbid UPDATE,
//! DELETE and TRUNCATE on the table; this chain detects tampering that bypasses them
//! (e.g. a superuser editing the data files or restoring a doctored backup).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::DomainError;

/// Hash of the virtual record before the first one.
pub const GENESIS: [u8; 32] = [0; 32];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub seq: i64,
    pub occurred_at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub payload: serde_json::Value,
    #[serde(with = "hex_bytes")]
    pub prev_hash: [u8; 32],
    #[serde(with = "hex_bytes")]
    pub hash: [u8; 32],
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let v = hex::decode(&s).map_err(serde::de::Error::custom)?;
        v.try_into().map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

/// Canonical JSON: object keys sorted, no insignificant whitespace. Sorted explicitly
/// because another crate in the build may turn on serde_json's `preserve_order`.
fn canonical_json(v: &serde_json::Value) -> Vec<u8> {
    fn write(v: &serde_json::Value, out: &mut Vec<u8>) {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                out.push(b'{');
                for (i, k) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    serde_json::to_writer(&mut *out, k).expect("strings serialize");
                    out.push(b':');
                    write(&map[k], out);
                }
                out.push(b'}');
            }
            serde_json::Value::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    write(item, out);
                }
                out.push(b']');
            }
            scalar => serde_json::to_writer(&mut *out, scalar).expect("scalars serialize"),
        }
    }
    let mut out = Vec::new();
    write(v, &mut out);
    out
}

/// SHA-256 over length-prefixed fields, which makes the encoding unambiguous
/// ("ab"+"c" and "a"+"bc" hash differently).
pub fn compute_hash(
    prev_hash: &[u8; 32],
    seq: i64,
    occurred_at: DateTime<Utc>,
    actor: &str,
    action: &str,
    entity: &str,
    entity_id: &str,
    payload: &serde_json::Value,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"yusr-audit-v1");
    h.update(prev_hash);
    h.update(seq.to_be_bytes());
    // Microsecond precision matches PostgreSQL timestamptz, so stored values re-hash exactly.
    h.update(occurred_at.timestamp_micros().to_be_bytes());
    let payload = canonical_json(payload);
    for field in [actor.as_bytes(), action.as_bytes(), entity.as_bytes(), entity_id.as_bytes(), &payload] {
        h.update((field.len() as u64).to_be_bytes());
        h.update(field);
    }
    h.finalize().into()
}

impl AuditRecord {
    /// Seals a new record onto the chain after `prev` (or the genesis when `None`).
    pub fn seal(
        prev: Option<(&[u8; 32], i64)>,
        occurred_at: DateTime<Utc>,
        actor: impl Into<String>,
        action: impl Into<String>,
        entity: impl Into<String>,
        entity_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        let (prev_hash, seq) = prev.map_or((GENESIS, 1), |(h, s)| (*h, s + 1));
        // Truncate to microseconds so the hash survives a database round-trip.
        let occurred_at = DateTime::from_timestamp_micros(occurred_at.timestamp_micros()).expect("in range");
        let (actor, action, entity, entity_id) = (actor.into(), action.into(), entity.into(), entity_id.into());
        let hash = compute_hash(&prev_hash, seq, occurred_at, &actor, &action, &entity, &entity_id, &payload);
        Self { seq, occurred_at, actor, action, entity, entity_id, payload, prev_hash, hash }
    }

    pub fn recompute(&self) -> [u8; 32] {
        compute_hash(
            &self.prev_hash,
            self.seq,
            self.occurred_at,
            &self.actor,
            &self.action,
            &self.entity,
            &self.entity_id,
            &self.payload,
        )
    }
}

/// Verifies a contiguous slice of the chain. `anchor` is the (hash, seq) of the record
/// just before the slice, or `None` when the slice starts at the genesis.
pub fn verify_chain(anchor: Option<([u8; 32], i64)>, records: &[AuditRecord]) -> Result<(), DomainError> {
    let (mut prev, mut seq) = anchor.unwrap_or((GENESIS, 0));
    for r in records {
        if r.seq != seq + 1 {
            return Err(DomainError::AuditChainBroken { seq: r.seq, reason: "sequence gap" });
        }
        if r.prev_hash != prev {
            return Err(DomainError::AuditChainBroken { seq: r.seq, reason: "previous hash mismatch" });
        }
        if r.recompute() != r.hash {
            return Err(DomainError::AuditChainBroken { seq: r.seq, reason: "content altered" });
        }
        prev = r.hash;
        seq = r.seq;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chain(n: usize) -> Vec<AuditRecord> {
        let mut out: Vec<AuditRecord> = Vec::new();
        for i in 0..n {
            let prev = out.last().map(|r| (&r.hash, r.seq));
            let r = AuditRecord::seal(prev, Utc::now(), "admin", "post", "journal_entry", i.to_string(), json!({"amount": "10.00", "i": i}));
            out.push(r);
        }
        out
    }

    #[test]
    fn valid_chain_verifies() {
        let c = chain(5);
        assert_eq!(c[0].prev_hash, GENESIS);
        verify_chain(None, &c).unwrap();
        verify_chain(Some((c[1].hash, c[1].seq)), &c[2..]).unwrap();
    }

    #[test]
    fn detects_edit_delete_and_reorder() {
        let mut c = chain(4);
        c[2].payload = json!({"amount": "99.00", "i": 2});
        assert_eq!(verify_chain(None, &c).unwrap_err(), DomainError::AuditChainBroken { seq: 3, reason: "content altered" });

        let mut c = chain(4);
        c.remove(1);
        assert!(matches!(verify_chain(None, &c), Err(DomainError::AuditChainBroken { seq: 3, .. })));

        let mut c = chain(4);
        c.swap(1, 2);
        assert!(verify_chain(None, &c).is_err());
    }

    #[test]
    fn hash_is_independent_of_key_order() {
        let t = Utc::now();
        let a = AuditRecord::seal(None, t, "u", "a", "e", "1", serde_json::from_str(r#"{"b":1,"a":2}"#).unwrap());
        let b = AuditRecord::seal(None, t, "u", "a", "e", "1", serde_json::from_str(r#"{"a":2,"b":1}"#).unwrap());
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn serde_roundtrip_keeps_hash_valid() {
        let c = chain(2);
        let back: Vec<AuditRecord> = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        verify_chain(None, &back).unwrap();
    }
}
