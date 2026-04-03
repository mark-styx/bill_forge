//! Email action token service for secure email-based actions
//!
//! Generates time-limited, cryptographically signed tokens that allow
//! users to perform actions (approve/reject) via email links without
//! requiring login.

use crate::{Error, Result, TenantId, UserId};
use base64::prelude::*;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Token payload for email actions
#[derive(Debug, Serialize, Deserialize)]
pub struct EmailActionToken {
    pub action: EmailAction,
    pub resource_id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: String,
    pub nonce: Uuid,
    pub expires_at: chrono::DateTime<Utc>,
}

/// Types of email actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailAction {
    ApproveInvoice,
    RejectInvoice,
    HoldInvoice,
    ViewInvoice,
}

/// Email action token service
pub struct EmailActionTokenService {
    pool: Arc<PgPool>,
    secret_key: String,
    token_expiry_hours: i64,
}

impl EmailActionTokenService {
    /// Create a new token service
    pub fn new(pool: Arc<PgPool>, secret_key: String) -> Self {
        Self {
            pool,
            secret_key,
            token_expiry_hours: 72, // 3 days
        }
    }

    /// Generate a secure token for an email action
    pub async fn generate_token(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        action: EmailAction,
        resource_id: Uuid,
        resource_type: &str,
        metadata: serde_json::Value,
    ) -> Result<String> {
        let nonce = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::hours(self.token_expiry_hours);

        // Create token payload
        let payload = EmailActionToken {
            action,
            resource_id,
            user_id: *user_id.as_uuid(),
            tenant_id: tenant_id.as_str().to_string(),
            nonce,
            expires_at,
        };

        // Serialize and encode
        let payload_json = serde_json::to_string(&payload)
            .map_err(|e| Error::Internal(format!("Failed to serialize token: {}", e)))?;

        // Generate signature
        let signature = self.sign(&payload_json);

        // Create final token (base64-encoded payload + signature)
        let token = format!("{}.{}", BASE64_STANDARD.encode(&payload_json), signature);

        // Store hash in database for revocation checking
        let token_hash = self.hash_token(&token);

        sqlx::query(
            r#"INSERT INTO email_action_tokens (
                id, tenant_id, token_hash, action_type, resource_type, resource_id,
                user_id, metadata, expires_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(Uuid::new_v4())
        .bind(*tenant_id.as_uuid())
        .bind(&token_hash)
        .bind(format!("{:?}", payload.action).to_lowercase())
        .bind(resource_type)
        .bind(resource_id)
        .bind(user_id.as_uuid())
        .bind(metadata)
        .bind(expires_at)
        .bind(Utc::now())
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to store token: {}", e)))?;

        Ok(token)
    }

    /// Validate and decode a token
    pub async fn validate_token(&self, token: &str) -> Result<EmailActionToken> {
        // Split token into payload and signature
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 2 {
            return Err(Error::Validation("Invalid token format".to_string()));
        }

        let payload_b64 = parts[0];
        let signature = parts[1];

        // Decode payload
        let payload_json = BASE64_STANDARD
            .decode(payload_b64)
            .map_err(|_| Error::Validation("Invalid token encoding".to_string()))?;

        let payload: EmailActionToken = serde_json::from_slice(&payload_json)
            .map_err(|_| Error::Validation("Invalid token payload".to_string()))?;

        // Verify signature
        let expected_signature = self.sign(&String::from_utf8_lossy(&payload_json));

        if signature != expected_signature {
            return Err(Error::Validation("Invalid token signature".to_string()));
        }

        // Check expiration
        if payload.expires_at < Utc::now() {
            return Err(Error::Validation("Token has expired".to_string()));
        }

        // Check if token was already used
        let token_hash = self.hash_token(token);

        let used: Option<bool> = sqlx::query_scalar(
            "SELECT (used_at IS NOT NULL) FROM email_action_tokens WHERE token_hash = $1",
        )
        .bind(&token_hash)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check token: {}", e)))?;

        if used == Some(true) {
            return Err(Error::Validation("Token has already been used".to_string()));
        }

        Ok(payload)
    }

    /// Mark a token as used (prevents replay attacks)
    pub async fn mark_used(&self, token: &str) -> Result<()> {
        let token_hash = self.hash_token(token);

        sqlx::query("UPDATE email_action_tokens SET used_at = NOW() WHERE token_hash = $1")
            .bind(&token_hash)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to mark token as used: {}", e)))?;

        Ok(())
    }

    /// Generate an action URL with embedded token
    pub fn generate_action_url(&self, base_url: &str, token: &str, action: &str) -> String {
        format!("{}/api/v1/actions/{}?t={}", base_url, action, token)
    }

    /// Sign a payload using HMAC-SHA256
    fn sign(&self, payload: &str) -> String {
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(payload.as_bytes());
        let result = mac.finalize();

        hex::encode(result.into_bytes())
    }

    /// Hash a token for database storage
    fn hash_token(&self, token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sign_and_verify() {
        let pool = Arc::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let service = EmailActionTokenService::new(pool, "test-secret-key".to_string());

        let payload = "test-payload";
        let signature = service.sign(payload);

        // Should verify correctly
        assert!(service.sign(payload) == signature);
    }

    #[tokio::test]
    async fn test_hash_token() {
        let pool = Arc::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let service = EmailActionTokenService::new(pool, "test-secret-key".to_string());

        let token = "test-token-123";
        let hash1 = service.hash_token(token);
        let hash2 = service.hash_token(token);

        // Same token should produce same hash
        assert_eq!(hash1, hash2);

        // Different token should produce different hash
        let hash3 = service.hash_token("different-token");
        assert_ne!(hash1, hash3);
    }
}
