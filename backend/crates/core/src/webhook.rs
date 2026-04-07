//! Shared webhook signature verification for integration providers
//!
//! Provides HMAC-SHA256 signature verification, timestamp freshness checks,
//! and replay nonce detection. Used by QuickBooks, Xero, Salesforce, Bill.com,
//! Workday, and Sage Intacct webhook handlers.

use chrono::{DateTime, TimeDelta, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Verify an inbound webhook signature (HMAC-SHA256).
///
/// Supports three signature formats:
/// - Hex-encoded: `abcdef0123456789...`
/// - Hex with prefix: `sha256=abcdef0123456789...`
/// - Base64-encoded (used by QuickBooks/Xero): `q1w2e3r4...`
pub fn verify_webhook_signature(payload: &[u8], signature: &str, secret: &str) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);

    // Try hex first (with optional sha256= prefix)
    let sig_hex = signature.strip_prefix("sha256=").unwrap_or(signature);
    if let Ok(sig_bytes) = hex::decode(sig_hex) {
        return mac.verify_slice(&sig_bytes).is_ok();
    }

    // Fall back to base64 (QuickBooks, Xero)
    if let Ok(sig_bytes) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature) {
        return mac.verify_slice(&sig_bytes).is_ok();
    }

    false
}

/// Validate that a webhook timestamp is within acceptable freshness bounds.
///
/// Returns `false` if the timestamp is older than `max_age_secs` or more than
/// 60 seconds in the future (clock skew tolerance).
pub fn validate_timestamp_freshness(timestamp: DateTime<Utc>, max_age_secs: i64) -> bool {
    let now = Utc::now();
    let age = now - timestamp;
    let max_age = TimeDelta::seconds(max_age_secs);
    let clock_skew_tolerance = TimeDelta::seconds(60);

    if age > max_age {
        return false;
    }
    if timestamp - now > clock_skew_tolerance {
        return false;
    }
    true
}

/// Check whether a webhook nonce has already been processed (replay protection).
///
/// Attempts to insert the nonce into `integration_webhook_nonces`. If the insert
/// succeeds (first time seen), returns `Ok(true)`. If a unique violation occurs
/// (replay), returns `Ok(false)`. Other errors are propagated.
pub async fn check_replay_nonce(
    pool: &PgPool,
    provider: &str,
    tenant_id: Uuid,
    nonce: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO integration_webhook_nonces (provider, tenant_id, nonce, received_at) VALUES ($1, $2, $3, NOW())",
    )
    .bind(provider)
    .bind(tenant_id)
    .bind(nonce)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(true),
        Err(e) => {
            if let Some(db_error) = e.as_database_error() {
                if db_error.code().as_deref() == Some("23505") {
                    return Ok(false);
                }
            }
            Err(e)
        }
    }
}

/// Compute a SHA-256 hash of the payload to use as a fallback nonce
/// when the provider doesn't include a deduplication ID.
pub fn compute_payload_nonce(payload: &[u8]) -> String {
    use sha2::Digest;
    let hash = Sha256::digest(payload);
    hex::encode(hash)
}

/// Standard webhook payload envelope shared across integration providers.
#[derive(Debug, serde::Deserialize)]
pub struct WebhookEnvelope {
    /// Event type (e.g., "vendor.updated", "invoice.created")
    pub event_type: String,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Provider-specific event payload
    pub payload: serde_json::Value,
    /// Optional deduplication nonce from the provider
    pub nonce: Option<String>,
    /// Optional tenant identifier included by some providers
    pub tenant_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn test_verify_valid_hex_signature() {
        let secret = "test-webhook-secret";
        let payload = b"hello world";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verify_webhook_signature(payload, &signature, secret));
    }

    #[test]
    fn test_verify_prefixed_hex_signature() {
        let secret = "my-secret";
        let payload = b"test payload";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_webhook_signature(payload, &sig, secret));
    }

    #[test]
    fn test_verify_valid_base64_signature() {
        let secret = "base64-secret";
        let payload = b"base64 payload test";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            mac.finalize().into_bytes(),
        );

        assert!(verify_webhook_signature(payload, &signature, secret));
    }

    #[test]
    fn test_reject_invalid_signature() {
        assert!(!verify_webhook_signature(b"payload", "bad-sig", "secret"));
    }

    #[test]
    fn test_reject_wrong_secret() {
        let payload = b"hello";
        let mut mac = HmacSha256::new_from_slice(b"correct-secret").unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(!verify_webhook_signature(payload, &signature, "wrong-secret"));
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
    fn test_timestamp_freshness_future_rejected() {
        let timestamp = Utc::now() + TimeDelta::seconds(120);
        assert!(!validate_timestamp_freshness(timestamp, 300));
    }

    #[test]
    fn test_timestamp_freshness_slight_future_ok() {
        let timestamp = Utc::now() + TimeDelta::seconds(30);
        assert!(validate_timestamp_freshness(timestamp, 300));
    }
}
