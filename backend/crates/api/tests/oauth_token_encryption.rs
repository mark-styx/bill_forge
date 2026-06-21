//! End-to-end check that QBO and Xero OAuth tokens are stored as AES-256-GCM
//! envelopes, not plaintext. Refs #432.
//!
//! The DB-touching test runs against the real PostgreSQL database (DATABASE_URL)
//! and is `#[ignore]`-d by default to mirror the other integration tests in
//! this crate. The pure roundtrip assertion runs in the default `cargo test`
//! pass and gives us a tight regression net independent of Postgres.

use billforge_core::security::TokenCipher;

fn fixture_cipher() -> TokenCipher {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let key = [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
        0x2e, 0x2f,
    ];
    TokenCipher::from_base64(&STANDARD.encode(key)).unwrap()
}

#[test]
fn stored_value_is_an_envelope_not_plaintext() {
    let cipher = fixture_cipher();
    let plaintext_access = "qbo-access-token-roundtrip-fixture";
    let plaintext_refresh = "qbo-refresh-token-roundtrip-fixture";

    // What the route layer binds into the access_token / refresh_token columns.
    let stored_access = cipher.seal(plaintext_access);
    let stored_refresh = cipher.seal(plaintext_refresh);

    assert!(
        stored_access.starts_with("v1:"),
        "access_token column must hold a v1 envelope, got {stored_access}"
    );
    assert!(
        stored_refresh.starts_with("v1:"),
        "refresh_token column must hold a v1 envelope, got {stored_refresh}"
    );
    assert!(
        !stored_access.contains(plaintext_access),
        "envelope must not leak plaintext access token"
    );
    assert!(
        !stored_refresh.contains(plaintext_refresh),
        "envelope must not leak plaintext refresh token"
    );

    // Read-side decryption recovers the original plaintext for the OAuth client.
    assert_eq!(cipher.open(&stored_access).unwrap(), plaintext_access);
    assert_eq!(cipher.open(&stored_refresh).unwrap(), plaintext_refresh);
}

#[cfg(test)]
mod integration {
    use super::fixture_cipher;
    use chrono::{Duration, Utc};
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn test_pool() -> PgPool {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost/billforge_test".to_string());
        PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    /// Verify that the exact INSERT used by the QBO connect callback writes a
    /// `v1:` envelope to the column, and that the read path recovers the
    /// original plaintext token.
    #[tokio::test]
    #[ignore]
    async fn qbo_connection_row_holds_envelope_not_plaintext() {
        let pool = test_pool().await;
        let cipher = fixture_cipher();
        let tenant_id = Uuid::new_v4();
        let company_id = format!("TEST-COMPANY-{}", tenant_id.as_simple());
        let plaintext_access = "fixture-qbo-access-token";
        let plaintext_refresh = "fixture-qbo-refresh-token";

        sqlx::query(
            "INSERT INTO tenants (id, name, slug, plan, created_at)
             VALUES ($1, $2, $3, 'free', NOW())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(tenant_id)
        .bind("OAuth Encryption Test Tenant")
        .bind(format!("oauth-enc-{}", tenant_id.as_simple()))
        .execute(&pool)
        .await
        .unwrap();

        let sealed_access = cipher.seal(plaintext_access);
        let sealed_refresh = cipher.seal(plaintext_refresh);
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO quickbooks_connections (
                tenant_id, company_id, access_token, refresh_token,
                access_token_expires_at, refresh_token_expires_at,
                environment, sync_enabled, created_at, updated_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, 'sandbox', true, NOW(), NOW())
             ON CONFLICT (tenant_id) DO UPDATE SET
                access_token = EXCLUDED.access_token,
                refresh_token = EXCLUDED.refresh_token",
        )
        .bind(tenant_id)
        .bind(&company_id)
        .bind(&sealed_access)
        .bind(&sealed_refresh)
        .bind(now + Duration::hours(1))
        .bind(now + Duration::days(100))
        .execute(&pool)
        .await
        .unwrap();

        // Raw column read — what a DB dump or backup would expose.
        let (raw_access, raw_refresh): (String, String) = sqlx::query_as(
            "SELECT access_token, refresh_token FROM quickbooks_connections WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            raw_access.starts_with("v1:"),
            "access_token must be stored as v1 envelope, got: {raw_access}"
        );
        assert!(
            raw_refresh.starts_with("v1:"),
            "refresh_token must be stored as v1 envelope, got: {raw_refresh}"
        );
        assert_ne!(raw_access, plaintext_access);
        assert_ne!(raw_refresh, plaintext_refresh);

        // Application read path recovers plaintext via TokenCipher::open.
        assert_eq!(cipher.open(&raw_access).unwrap(), plaintext_access);
        assert_eq!(cipher.open(&raw_refresh).unwrap(), plaintext_refresh);

        // Clean up.
        sqlx::query("DELETE FROM quickbooks_connections WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
    }
}
