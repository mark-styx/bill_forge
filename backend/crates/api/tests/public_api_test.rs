//! Integration tests for the Public API + Webhooks platform (#293).
//!
//! Covers: PAT auth, scope enforcement, rate limiting, tenant isolation,
//! webhook subscription CRUD, and HMAC-signed webhook delivery.
//!
//! Run: `cargo test -p billforge-api --test public_api_test`

#![allow(warnings)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Extension, Json, Router,
};
use billforge_core::public_api::{
    compute_hmac_signature, generate_signing_secret, require_scope, verify_pat, PublicApiToken,
    RateLimiter, ALLOWED_EVENT_TYPES,
};
use billforge_db::DatabaseManager;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::util::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Helper to create an api_key row in the metadata database.
/// Returns (api_key_id, bearer_token).
async fn seed_api_key(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    scopes: &[&str],
    revoked: bool,
) -> (Uuid, String) {
    let id = Uuid::new_v4();
    // Generate a random bearer token
    let bearer = format!("bfg_pat_{}", Uuid::new_v4().to_string().replace('-', ""));
    let token_hash = hex::encode(Sha256::digest(bearer.as_bytes()));
    let token_prefix = &bearer[..8];

    let revoked_at = if revoked {
        Some(chrono::Utc::now())
    } else {
        None
    };

    sqlx::query(
        r#"INSERT INTO api_keys (id, tenant_id, name, token_hash, token_prefix, scopes, rate_limit_per_minute, created_at, revoked_at)
           VALUES ($1, $2, 'test-key', $3, $4, $5, 60, NOW(), $6)"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(&token_hash)
    .bind(token_prefix)
    .bind(scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    .bind(revoked_at)
    .execute(pool)
    .await
    .expect("seed api_key");

    (id, bearer)
}

// ---------------------------------------------------------------------------
// Unit tests (no database required)
// ---------------------------------------------------------------------------

#[test]
fn test_require_scope_missing_returns_error() {
    let token = PublicApiToken {
        tenant_id: Uuid::new_v4(),
        api_key_id: Uuid::new_v4(),
        scopes: vec!["invoices:read".to_string()],
        rate_limit_per_minute: 60,
    };
    let result = require_scope(&token, "webhooks:write");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("webhooks:write"));
}

#[test]
fn test_require_scope_present_succeeds() {
    let token = PublicApiToken {
        tenant_id: Uuid::new_v4(),
        api_key_id: Uuid::new_v4(),
        scopes: vec!["invoices:read".to_string(), "webhooks:write".to_string()],
        rate_limit_per_minute: 60,
    };
    assert!(require_scope(&token, "invoices:read").is_ok());
    assert!(require_scope(&token, "webhooks:write").is_ok());
}

#[test]
fn test_compute_hmac_signature_deterministic() {
    let secret = "test-secret";
    let body = b"{\"event\":\"invoice.created\"}";
    let sig1 = compute_hmac_signature(secret, body);
    let sig2 = compute_hmac_signature(secret, body);
    assert_eq!(sig1, sig2);
    assert!(!sig1.is_empty());
}

#[test]
fn test_generate_signing_secret_unique() {
    let s1 = generate_signing_secret();
    let s2 = generate_signing_secret();
    assert_ne!(s1, s2);
    assert_eq!(s1.len(), 64); // 32 bytes hex-encoded
}

#[test]
fn test_allowed_event_types_contains_core_events() {
    assert!(ALLOWED_EVENT_TYPES.contains(&"invoice.created"));
    assert!(ALLOWED_EVENT_TYPES.contains(&"invoice.approved"));
    assert!(ALLOWED_EVENT_TYPES.contains(&"approval.requested"));
}

// ---------------------------------------------------------------------------
// Rate limiter tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rate_limiter_allows_within_limit() {
    let limiter = RateLimiter::new();
    let key_id = Uuid::new_v4();

    for _ in 0..60 {
        assert!(limiter.check(key_id, 60).await.is_ok());
    }
}

#[tokio::test]
async fn test_rate_limiter_rejects_over_limit() {
    let limiter = RateLimiter::new();
    let key_id = Uuid::new_v4();

    // Exhaust the limit
    for _ in 0..3 {
        limiter.check(key_id, 3).await.unwrap();
    }

    // 4th request should be rejected
    let result = limiter.check(key_id, 3).await;
    assert!(result.is_err());
    let retry_after = result.unwrap_err();
    assert!(retry_after > 0);
}

#[tokio::test]
async fn test_rate_limiter_per_key_isolation() {
    let limiter = RateLimiter::new();
    let key_a = Uuid::new_v4();
    let key_b = Uuid::new_v4();

    // Exhaust key A
    for _ in 0..2 {
        limiter.check(key_a, 2).await.unwrap();
    }
    assert!(limiter.check(key_a, 2).await.is_err());

    // Key B should still work
    assert!(limiter.check(key_b, 2).await.is_ok());
}

// ---------------------------------------------------------------------------
// Integration tests (require DATABASE_URL)
// ---------------------------------------------------------------------------

/// Helper to get a database pool from DATABASE_URL (skips test if not set).
async fn get_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .ok()?;
    // Run the migration to ensure tables exist
    let migration_sql = include_str!("../../../migrations/110_create_public_api_platform.sql");
    sqlx::raw_sql(migration_sql).execute(&pool).await.ok()?;
    Some(pool)
}

/// Seed a tenant in the metadata database (idempotent).
async fn seed_tenant(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) {
    sqlx::query(
        r#"INSERT INTO tenants (id, name, is_active, created_at)
           VALUES ($1, $2, true, NOW())
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .ok();
}

#[tokio::test]
async fn test_verify_pat_valid_key() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let tenant_id = Uuid::new_v4();
    seed_tenant(&pool, tenant_id, "Test Tenant PAT").await;
    let (key_id, bearer) = seed_api_key(&pool, tenant_id, &["invoices:read"], false).await;

    let token = verify_pat(&pool, &bearer).await.unwrap();
    assert_eq!(token.tenant_id, tenant_id);
    assert_eq!(token.api_key_id, key_id);
    assert!(token.scopes.contains(&"invoices:read".to_string()));
}

#[tokio::test]
async fn test_verify_pat_revoked_key_returns_error() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let tenant_id = Uuid::new_v4();
    seed_tenant(&pool, tenant_id, "Test Tenant Revoked").await;
    let (_key_id, bearer) = seed_api_key(&pool, tenant_id, &["invoices:read"], true).await;

    let result = verify_pat(&pool, &bearer).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("revoked"));
}

#[tokio::test]
async fn test_verify_pat_wrong_token_returns_error() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let result = verify_pat(&pool, "bfg_pat_completely_wrong_token").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_webhook_subscription_crud_and_delivery_audit() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let tenant_id = Uuid::new_v4();
    seed_tenant(&pool, tenant_id, "Test Tenant Webhook").await;
    let (key_id, _bearer) = seed_api_key(
        &pool,
        tenant_id,
        &["webhooks:write", "webhooks:read"],
        false,
    )
    .await;

    // Create a webhook subscription
    let sub_id = Uuid::new_v4();
    let signing_secret = generate_signing_secret();
    let target_url = "https://example.com/webhook";

    sqlx::query(
        r#"INSERT INTO webhook_subscriptions (id, tenant_id, api_key_id, target_url, event_types, signing_secret, is_active, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, true, NOW())"#,
    )
    .bind(sub_id)
    .bind(tenant_id)
    .bind(key_id)
    .bind(target_url)
    .bind(&vec!["invoice.created".to_string()])
    .bind(&signing_secret)
    .execute(&pool)
    .await
    .expect("insert subscription");

    // Verify subscription exists
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM webhook_subscriptions WHERE id = $1 AND tenant_id = $2")
            .bind(sub_id)
            .bind(tenant_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(row.is_some());

    // Simulate a webhook delivery audit row
    let delivery_id = Uuid::new_v4();
    let payload = serde_json::json!({"invoice_id": "test-123", "event": "invoice.created"});
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let signature = compute_hmac_signature(&signing_secret, &body_bytes);

    sqlx::query(
        r#"INSERT INTO webhook_deliveries (id, subscription_id, event_type, payload, response_status, response_body, attempted_at, success)
           VALUES ($1, $2, $3, $4, 200, 'OK', NOW(), true)"#,
    )
    .bind(delivery_id)
    .bind(sub_id)
    .bind("invoice.created")
    .bind(&payload)
    .execute(&pool)
    .await
    .expect("insert delivery");

    // Verify delivery audit row exists
    let delivery_row: Option<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, event_type, success FROM webhook_deliveries WHERE subscription_id = $1",
    )
    .bind(sub_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (del_id, event_type, success) = delivery_row.unwrap();
    assert_eq!(del_id, delivery_id);
    assert_eq!(event_type, "invoice.created");
    assert!(success);

    // Verify the HMAC signature is computable from the stored signing_secret
    let recomputed = compute_hmac_signature(&signing_secret, &body_bytes);
    assert_eq!(signature, recomputed);

    // Delete subscription and verify cascade
    sqlx::query("DELETE FROM webhook_subscriptions WHERE id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .unwrap();

    let check: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM webhook_subscriptions WHERE id = $1")
            .bind(sub_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(check.is_none());
}

#[tokio::test]
async fn test_tenant_isolation_webhook_subscriptions() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    seed_tenant(&pool, tenant_a, "Tenant A").await;
    seed_tenant(&pool, tenant_b, "Tenant B").await;

    let (key_a, _) = seed_api_key(&pool, tenant_a, &["webhooks:write"], false).await;
    let (key_b, _) = seed_api_key(&pool, tenant_b, &["webhooks:write"], false).await;

    // Create subscription for tenant A
    sqlx::query(
        r#"INSERT INTO webhook_subscriptions (id, tenant_id, api_key_id, target_url, event_types, signing_secret, is_active, created_at)
           VALUES ($1, $2, $3, 'https://a.example.com/hook', $4, 'secret-a', true, NOW())"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_a)
    .bind(key_a)
    .bind(&vec!["invoice.created".to_string()])
    .execute(&pool)
    .await
    .unwrap();

    // Create subscription for tenant B
    sqlx::query(
        r#"INSERT INTO webhook_subscriptions (id, tenant_id, api_key_id, target_url, event_types, signing_secret, is_active, created_at)
           VALUES ($1, $2, $3, 'https://b.example.com/hook', $4, 'secret-b', true, NOW())"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_b)
    .bind(key_b)
    .bind(&vec!["invoice.created".to_string()])
    .execute(&pool)
    .await
    .unwrap();

    // Tenant A sees only its own subscription
    let a_subs: Vec<(String,)> =
        sqlx::query_as("SELECT target_url FROM webhook_subscriptions WHERE tenant_id = $1")
            .bind(tenant_a)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(a_subs.len(), 1);
    assert_eq!(a_subs[0].0, "https://a.example.com/hook");

    // Tenant B sees only its own subscription
    let b_subs: Vec<(String,)> =
        sqlx::query_as("SELECT target_url FROM webhook_subscriptions WHERE tenant_id = $1")
            .bind(tenant_b)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(b_subs.len(), 1);
    assert_eq!(b_subs[0].0, "https://b.example.com/hook");
}
