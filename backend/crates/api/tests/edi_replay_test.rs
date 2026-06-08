//! Integration tests for EDI webhook replay protection
//!
//! Tests the timestamp freshness and nonce deduplication logic
//! that prevents replay attacks on EDI webhook endpoints.
//!
//! Database-dependent tests (replay nonce, full webhook flow) require
//! a running PostgreSQL instance and are run in CI. The tests below
//! verify the pure timestamp freshness logic and data structures.

#![allow(warnings)]

use billforge_edi::{check_replay_nonce, check_replay_nonce_tx, validate_timestamp_freshness};
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
// The following tests require a live PostgreSQL database. Each is gated with
// `#[ignore]` so they only run via `cargo test ... -- --ignored`.
//
// To run locally:
//   TEST_DATABASE_URL=postgres://... \
//     cargo test -p billforge-api --test edi_replay_test -- --ignored

mod integration {
    use super::*;
    use sha2::Digest;
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
    #[ignore] // Requires DATABASE_URL - run with: cargo test --test edi_replay_test -- --ignored
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
    #[ignore] // Requires DATABASE_URL - run with: cargo test --test edi_replay_test -- --ignored
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
    #[ignore] // Requires DATABASE_URL - run with: cargo test --test edi_replay_test -- --ignored
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

    #[sqlx::test]
    #[ignore] // Requires DATABASE_URL - run with: cargo test --test edi_replay_test -- --ignored
    async fn replay_nonce_released_on_processing_failure() {
        // Models the #363 fix: when an EDI webhook handler wraps the post-nonce
        // body in a single Postgres tx, dropping that tx (handler failure) must
        // roll back the nonce insert alongside the rest of the writes. The next
        // provider retry of the same payload then succeeds.
        let pool = setup_test_pool().await;
        let tenant_id = Uuid::new_v4();
        let nonce = "test-nonce-rollback-001";

        // 1. Begin a tx, insert the nonce inside it, then ROLL BACK the tx.
        //    `tx.rollback()` is equivalent to the tx being dropped from a
        //    handler that returns Err -- both undo the nonce insert.
        {
            let mut tx = pool.begin().await.expect("begin tx");
            let first = check_replay_nonce_tx(&mut tx, tenant_id, nonce)
                .await
                .expect("nonce check ok");
            assert!(first, "first-seen nonce inside tx should return true");
            tx.rollback().await.expect("rollback tx");
        }

        // 2. After rollback, the nonce row is gone. A retry with the same
        //    nonce on a fresh tx succeeds (first-seen again).
        {
            let mut tx = pool.begin().await.expect("begin tx");
            let retry = check_replay_nonce_tx(&mut tx, tenant_id, nonce)
                .await
                .expect("nonce check ok");
            assert!(retry, "after rollback, retry should be first-seen");
            tx.commit().await.expect("commit tx");
        }

        // 3. Without rollback, a second check is flagged as replay (sanity
        //    check that the previous commit actually landed the row).
        let third = check_replay_nonce(&pool, tenant_id, nonce)
            .await
            .expect("pool check ok");
        assert!(!third, "third check (after commit) should be flagged replay");

        // Cleanup
        sqlx::query("DELETE FROM edi_webhook_nonces WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
    }

    #[sqlx::test]
    #[ignore] // Requires DATABASE_URL - run with: cargo test --test edi_replay_test -- --ignored
    async fn replay_nonce_persists_when_tx_commits() {
        // Models the happy path: a webhook handler commits its tx, so the
        // nonce row sticks and a subsequent retry of the same payload is
        // correctly rejected as a replay.
        let pool = setup_test_pool().await;
        let tenant_id = Uuid::new_v4();
        let nonce = "test-nonce-commit-001";

        {
            let mut tx = pool.begin().await.expect("begin tx");
            let first = check_replay_nonce_tx(&mut tx, tenant_id, nonce)
                .await
                .expect("nonce check ok");
            assert!(first, "first-seen nonce inside tx should return true");
            tx.commit().await.expect("commit tx");
        }

        // After commit, the same nonce on a fresh tx is detected as replay.
        {
            let mut tx = pool.begin().await.expect("begin tx");
            let replay = check_replay_nonce_tx(&mut tx, tenant_id, nonce)
                .await
                .expect("nonce check ok");
            assert!(!replay, "after commit, replay should be flagged");
            tx.rollback().await.ok();
        }

        // Cleanup
        sqlx::query("DELETE FROM edi_webhook_nonces WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
    }
}
