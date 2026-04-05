//! Webhook signature verification for EDI middleware callbacks

use chrono::{DateTime, TimeDelta, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Verify an inbound webhook signature (HMAC-SHA256)
///
/// Most EDI middleware providers sign webhook payloads with a shared secret
/// using HMAC-SHA256. The signature is sent in a header (typically
/// `X-Signature` or `X-Webhook-Signature`).
pub fn verify_webhook_signature(payload: &[u8], signature: &str, secret: &str) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);

    // Signature may be hex-encoded or prefixed with "sha256="
    let sig_hex = signature.strip_prefix("sha256=").unwrap_or(signature);
    let Ok(sig_bytes) = hex::decode(sig_hex) else {
        return false;
    };

    mac.verify_slice(&sig_bytes).is_ok()
}

/// Validate that a webhook timestamp is within acceptable freshness bounds.
///
/// Returns `false` if the timestamp is older than `max_age_secs` or more than
/// 60 seconds in the future (clock skew tolerance). Returns `true` otherwise.
pub fn validate_timestamp_freshness(timestamp: DateTime<Utc>, max_age_secs: i64) -> bool {
    let now = Utc::now();
    let age = now - timestamp;
    let max_age = TimeDelta::seconds(max_age_secs);
    let clock_skew_tolerance = TimeDelta::seconds(60);

    // Reject if too old
    if age > max_age {
        return false;
    }
    // Reject if too far in the future (clock skew)
    if timestamp - now > clock_skew_tolerance {
        return false;
    }
    true
}

/// Check whether a webhook nonce has already been processed (replay protection).
///
/// Attempts to insert the nonce into `edi_webhook_nonces`. If the insert succeeds
/// (first time seen), returns `Ok(true)`. If a unique violation occurs (replay),
/// returns `Ok(false)`. Other errors are propagated.
pub async fn check_replay_nonce(
    pool: &PgPool,
    tenant_id: Uuid,
    nonce: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO edi_webhook_nonces (tenant_id, nonce, received_at) VALUES ($1, $2, NOW())",
    )
    .bind(tenant_id)
    .bind(nonce)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(true),
        Err(e) => {
            // sqlx error code 23505 = unique_violation in PostgreSQL
            if let Some(db_error) = e.as_database_error() {
                if db_error.code().as_deref() == Some("23505") {
                    return Ok(false);
                }
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn test_verify_valid_signature() {
        let secret = "test-webhook-secret";
        let payload = b"hello world";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verify_webhook_signature(payload, &signature, secret));
    }

    #[test]
    fn test_reject_invalid_signature() {
        assert!(!verify_webhook_signature(b"payload", "bad-sig", "secret"));
    }

    #[test]
    fn test_verify_prefixed_signature() {
        let secret = "my-secret";
        let payload = b"test payload";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_webhook_signature(payload, &sig, secret));
    }

    #[test]
    fn test_timestamp_freshness_valid() {
        let timestamp = Utc::now() - TimeDelta::seconds(30);
        assert!(validate_timestamp_freshness(timestamp, 300));
    }

    #[test]
    fn test_timestamp_freshness_expired() {
        let timestamp = Utc::now() - TimeDelta::seconds(600);
        assert!(!validate_timestamp_freshness(timestamp, 300));
    }

    #[test]
    fn test_timestamp_freshness_future() {
        // 2 minutes in the future exceeds 60s clock skew tolerance
        let timestamp = Utc::now() + TimeDelta::seconds(120);
        assert!(!validate_timestamp_freshness(timestamp, 300));
    }

    #[test]
    fn test_timestamp_freshness_slight_future() {
        // 30 seconds in the future is within 60s clock skew tolerance
        let timestamp = Utc::now() + TimeDelta::seconds(30);
        assert!(validate_timestamp_freshness(timestamp, 300));
    }
}
