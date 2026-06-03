//! Public API + Webhooks Platform core logic
//!
//! PAT auth verification, scope enforcement, in-memory rate limiting,
//! and outbound webhook dispatch with HMAC signing.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;

type HmacSha256 = Hmac<sha2::Sha256>;

/// Allowed event types for webhook subscriptions.
pub const ALLOWED_EVENT_TYPES: &[&str] = &[
    "invoice.created",
    "invoice.approved",
    "approval.requested",
];

/// Represents a verified PAT token's identity and permissions.
#[derive(Debug, Clone)]
pub struct PublicApiToken {
    pub tenant_id: Uuid,
    pub api_key_id: Uuid,
    pub scopes: Vec<String>,
    pub rate_limit_per_minute: i32,
}

/// In-memory token-bucket rate limiter keyed by api_key_id.
#[derive(Clone, Default)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<Uuid, (Instant, u32)>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns Ok(()) if the request is within rate limits, Err(retry_after_secs) if exceeded.
    pub async fn check(&self, key_id: Uuid, limit_per_minute: i32) -> Result<(), u64> {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        let limit = limit_per_minute as u32;

        let entry = buckets.entry(key_id).or_insert((now, 0));

        // If the window is older than 60 seconds, reset it
        if now.duration_since(entry.0).as_secs() >= 60 {
            *entry = (now, 0);
        }

        if entry.1 >= limit {
            let elapsed = now.duration_since(entry.0).as_secs();
            let retry_after = 60u64.saturating_sub(elapsed).max(1);
            return Err(retry_after);
        }

        entry.1 += 1;
        Ok(())
    }
}

/// Verify a PAT bearer token against the api_keys table.
/// Returns the token's identity on success.
pub async fn verify_pat(pool: &PgPool, bearer_token: &str) -> Result<PublicApiToken, String> {
    // SHA-256 hash the bearer token
    let token_hash = hex::encode(Sha256::digest(bearer_token.as_bytes()));

    let row = sqlx::query_as::<_, (Uuid, Uuid, Vec<String>, i32)>(
        r#"SELECT id, tenant_id, scopes, rate_limit_per_minute
           FROM api_keys
           WHERE token_hash = $1 AND revoked_at IS NULL"#,
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Database error verifying PAT: {}", e))?;

    let (api_key_id, tenant_id, scopes, rate_limit_per_minute) =
        row.ok_or_else(|| "Invalid or revoked API key".to_string())?;

    // Update last_used_at (best-effort, don't fail the request)
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
        .bind(api_key_id)
        .execute(pool)
        .await;

    Ok(PublicApiToken {
        tenant_id,
        api_key_id,
        scopes,
        rate_limit_per_minute,
    })
}

/// Check that the token has the required scope.
pub fn require_scope(token: &PublicApiToken, required: &str) -> Result<(), String> {
    if token.scopes.iter().any(|s| s == required) {
        Ok(())
    } else {
        Err(format!("Missing required scope: {}", required))
    }
}

/// Compute HMAC-SHA256 signature of a body using a signing secret.
pub fn compute_hmac_signature(signing_secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Dispatch webhooks for a given event to all matching active subscriptions.
/// This is synchronous fan-out: POST to each target, 5s timeout, single attempt.
/// Failures are logged but do NOT fail the caller.
pub async fn dispatch_webhook(
    metadata_pool: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
) {
    // Find active subscriptions matching this event type for this tenant
    let subscriptions = match sqlx::query_as::<
        _,
        (Uuid, Uuid, String, String, bool),
    >(
        r#"SELECT id, tenant_id, target_url, signing_secret, is_active
           FROM webhook_subscriptions
           WHERE tenant_id = $1 AND is_active = true AND $2 = ANY(event_types)"#,
    )
    .bind(tenant_id)
    .bind(event_type)
    .fetch_all(metadata_pool)
    .await
    {
        Ok(subs) => subs,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to fetch webhook subscriptions");
            return;
        }
    };

    let delivery_id = Uuid::new_v4();
    let body_bytes = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to serialize webhook payload");
            return;
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to build HTTP client for webhooks");
            return;
        }
    };

    for (sub_id, _sub_tenant_id, target_url, signing_secret, _is_active) in subscriptions {
        let signature = compute_hmac_signature(&signing_secret, &body_bytes);

        let result = client
            .post(&target_url)
            .header("Content-Type", "application/json")
            .header("X-BillForge-Event", event_type)
            .header("X-BillForge-Signature", signature)
            .header("X-BillForge-Delivery", delivery_id.to_string())
            .body(body_bytes.clone())
            .send()
            .await;

        let (success, response_status, response_body) = match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                ((200..300).contains(&status), Some(status as i32), Some(body))
            }
            Err(e) => {
                tracing::warn!(
                    subscription_id = %sub_id,
                    error = %e,
                    "Webhook delivery failed"
                );
                (false, None, Some(e.to_string()))
            }
        };

        // Record delivery audit row
        let _ = sqlx::query(
            r#"INSERT INTO webhook_deliveries (id, subscription_id, event_type, payload, response_status, response_body, attempted_at, success)
               VALUES ($1, $2, $3, $4, $5, $6, NOW(), $7)"#,
        )
        .bind(Uuid::new_v4())
        .bind(sub_id)
        .bind(event_type)
        .bind(&payload)
        .bind(response_status)
        .bind(response_body.as_deref().unwrap_or("").chars().take(10000).collect::<String>())
        .bind(success)
        .execute(metadata_pool)
        .await;

        // Update subscription last delivery stats
        let _ = sqlx::query(
            r#"UPDATE webhook_subscriptions
               SET last_delivery_at = NOW(),
                   last_delivery_status = $1
               WHERE id = $2"#,
        )
        .bind(if success { "success" } else { "failed" })
        .bind(sub_id)
        .execute(metadata_pool)
        .await;
    }
}

/// Generate a random signing secret for webhook subscriptions.
pub fn generate_signing_secret() -> String {
    use std::fmt::Write;
    let bytes: [u8; 32] = rand_random_bytes();
    let mut s = String::with_capacity(64);
    for b in &bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

fn rand_random_bytes() -> [u8; 32] {
    // Use a simple approach: hash a UUID with the current timestamp
    let mut result = [0u8; 32];
    let input = format!("{}-{}", Uuid::new_v4(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let hash = Sha256::digest(input.as_bytes());
    result.copy_from_slice(&hash);
    result
}
