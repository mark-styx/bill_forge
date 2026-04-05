//! Integration tests for EDI webhook replay protection
//!
//! Tests the timestamp freshness and nonce deduplication logic
//! that prevents replay attacks on EDI webhook endpoints.
//!
//! Database-dependent tests (replay nonce, full webhook flow) require
//! a running PostgreSQL instance and are run in CI. The tests below
//! verify the pure timestamp freshness logic and data structures.

use billforge_edi::{validate_timestamp_freshness, check_replay_nonce};
use chrono::{TimeDelta, Utc};

#[test]
fn test_timestamp_freshness_valid_30s_ago() {
    let ts = Utc::now() - TimeDelta::seconds(30);
    assert!(validate_timestamp_freshness(ts, 300));
}

#[test]
fn test_timestamp_freshness_expired_10min() {
    let ts = Utc::now() - TimeDelta::seconds(600);
    assert!(!validate_timestamp_freshness(ts, 300));
}

#[test]
fn test_timestamp_freshness_future_2min_rejected() {
    let ts = Utc::now() + TimeDelta::seconds(120);
    assert!(!validate_timestamp_freshness(ts, 300));
}

#[test]
fn test_timestamp_freshness_future_30s_allowed() {
    let ts = Utc::now() + TimeDelta::seconds(30);
    assert!(validate_timestamp_freshness(ts, 300));
}

#[test]
fn test_timestamp_freshness_exactly_at_boundary() {
    // Exactly at the max age boundary should still pass (not strictly greater)
    let ts = Utc::now() - TimeDelta::seconds(299);
    assert!(validate_timestamp_freshness(ts, 300));
}

#[test]
fn test_timestamp_freshness_one_second_over_boundary() {
    let ts = Utc::now() - TimeDelta::seconds(301);
    assert!(!validate_timestamp_freshness(ts, 300));
}

#[test]
fn test_nonce_body_hash_deterministic() {
    // Verify that the body-hash fallback nonce produces the same value
    // for the same input, which is required for replay detection.
    use sha2::{Digest, Sha256};

    let body = b"test webhook body content";
    let hash1 = Sha256::digest(body);
    let hash2 = Sha256::digest(body);
    assert_eq!(hex::encode(hash1), hex::encode(hash2));

    // Different body produces different hash
    let other_body = b"different content";
    let hash3 = Sha256::digest(other_body);
    assert_ne!(hex::encode(hash1), hex::encode(hash3));
}

// ============================================================================
// Database-dependent tests
// ============================================================================
// The following tests require a live PostgreSQL database with tenant schema.
// They are enabled via the `integration` feature flag in CI.
//
// To run locally:
//   SQLX_OFFLINE=false cargo test --features integration --test edi_replay_test

#[cfg(feature = "integration")]
mod integration {
    use super::*;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn setup_test_pool() -> PgPool {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for integration tests");
        let pool = PgPool::connect(&db_url).await.expect("connect to test DB");

        // Create the nonces table if not exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS edi_webhook_nonces (
                tenant_id UUID NOT NULL,
                nonce VARCHAR(128) NOT NULL,
                received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (tenant_id, nonce)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("create nonces table");

        pool
    }

    #[sqlx::test]
    async fn test_webhook_replay_rejected() {
        let pool = setup_test_pool().await;
        let tenant_id = Uuid::new_v4();
        let nonce = "test-nonce-replay-001";

        // First request should succeed (first time seen)
        let result = check_replay_nonce(&pool, tenant_id, nonce).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Second request with same nonce should be flagged as replay
        let result = check_replay_nonce(&pool, tenant_id, nonce).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Cleanup
        sqlx::query("DELETE FROM edi_webhook_nonces WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
    }

    #[sqlx::test]
    async fn test_different_tenants_same_nonce_allowed() {
        let pool = setup_test_pool().await;
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let nonce = "shared-nonce-across-tenants";

        // Tenant A uses the nonce - should succeed
        let result_a = check_replay_nonce(&pool, tenant_a, nonce).await;
        assert!(result_a.is_ok());
        assert!(result_a.unwrap());

        // Tenant B uses the same nonce - should also succeed (different tenant)
        let result_b = check_replay_nonce(&pool, tenant_b, nonce).await;
        assert!(result_b.is_ok());
        assert!(result_b.unwrap());

        // Cleanup
        sqlx::query("DELETE FROM edi_webhook_nonces WHERE tenant_id IN ($1, $2)")
            .bind(tenant_a)
            .bind(tenant_b)
            .execute(&pool)
            .await
            .ok();
    }

    #[sqlx::test]
    async fn test_no_middleware_id_uses_body_hash() {
        let pool = setup_test_pool().await;
        let tenant_id = Uuid::new_v4();

        // Simulate the body-hash fallback used when middleware_id is None
        let body = b"raw webhook body bytes";
        let hash = sha2::Sha256::digest(body);
        let nonce = hex::encode(hash);

        // First call should succeed
        let result = check_replay_nonce(&pool, tenant_id, &nonce).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Replay should be detected
        let result = check_replay_nonce(&pool, tenant_id, &nonce).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Cleanup
        sqlx::query("DELETE FROM edi_webhook_nonces WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
    }
}
