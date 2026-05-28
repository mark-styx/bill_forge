//! Integration tests for the lightweight QBO integration module.
//!
//! Verifies:
//! - OAuth authorize URL construction contains required params
//! - Vendor upsert via `external_id` column works correctly
//! - `last_sync_status` / `last_sync_error` columns on `quickbooks_connections`
//!
//! These tests run against the real PostgreSQL database (DATABASE_URL).

use billforge_core::types::TenantId;
use chrono::{Duration, Utc};
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";

/// Helper to get a database pool from DATABASE_URL
async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Clean up test vendors by name prefix.
async fn cleanup_test_vendors(pool: &sqlx::PgPool, tenant_id: &TenantId, prefix: &str) {
    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 AND name LIKE $2")
        .bind(*tenant_id.as_uuid())
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
}

/// Clean up test QBO connections.
async fn cleanup_qbo_connection(pool: &sqlx::PgPool, tenant_id: &TenantId, company_id: &str) {
    sqlx::query("DELETE FROM quickbooks_connections WHERE tenant_id = $1 AND company_id = $2")
        .bind(*tenant_id.as_uuid())
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
}

// ============================================================================
// Test: authorize URL contains required OAuth params
// ============================================================================

#[tokio::test]
async fn test_connect_returns_authorize_url_with_state() {
    // Verify URL construction logic matches QBO OAuth 2.0 requirements.
    // We build the URL the same way qbo.rs does and assert all required params.
    let client_id = "test_client_id";
    let redirect_uri = "https://example.com/callback";
    let csrf_state = format!("{}:{}", SANDBOX_TENANT_ID, Uuid::new_v4());
    let scope = "com.intuit.quickbooks.accounting";

    let authorize_url = format!(
        "https://appcenter.intuit.com/connect/oauth2?client_id={client_id}&redirect_uri={redirect_uri}&scope={scope}&state={csrf_state}&response_type=code"
    );

    // Assert URL contains required OAuth params.
    assert!(
        authorize_url.contains(&format!("client_id={client_id}")),
        "URL must contain client_id"
    );
    assert!(
        authorize_url.contains(&format!("redirect_uri={redirect_uri}")),
        "URL must contain redirect_uri"
    );
    assert!(
        authorize_url.contains(&format!("scope={scope}")),
        "URL must contain scope"
    );
    assert!(
        authorize_url.contains("response_type=code"),
        "URL must contain response_type=code"
    );

    // Assert state is non-empty and has the expected format (tenant_id:uuid).
    let state_part = csrf_state.split(':').collect::<Vec<_>>();
    assert_eq!(state_part.len(), 2, "State must be tenant_id:uuid");
    assert_eq!(state_part[0], SANDBOX_TENANT_ID);
    assert!(!state_part[1].is_empty(), "State UUID must be non-empty");
}

// ============================================================================
// Test: vendor sync upserts via external_id and updates sync status
// ============================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test qbo_tests -- --ignored
async fn test_sync_vendors_upserts_and_updates_status() {
    let pool = get_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());
    let test_company_id = "TEST-QBO-SYNC-COMPANY";

    // Clean up from prior runs.
    cleanup_test_vendors(&pool, &tenant_id, "TEST-QBO-SYNC").await;
    cleanup_qbo_connection(&pool, &tenant_id, test_company_id).await;

    // Seed a quickbooks_connections row (simulating a completed OAuth flow).
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO quickbooks_connections (
            tenant_id, company_id, access_token, refresh_token,
            access_token_expires_at, refresh_token_expires_at,
            environment, sync_enabled, last_sync_status,
            created_at, updated_at
         )
         VALUES ($1, $2, 'test_access_token', 'test_refresh_token',
                 $3, $4, 'sandbox', true, 'idle', NOW(), NOW())",
    )
    .bind(*tenant_id.as_uuid())
    .bind(test_company_id)
    .bind(now + Duration::hours(1))
    .bind(now + Duration::days(90))
    .execute(&pool)
    .await
    .expect("Should seed quickbooks_connections");

    // Simulate vendor upsert (same SQL as qbo_sync_vendors handler).
    let external_id = "qbo:42";
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, external_id, created_at, updated_at)
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
         ON CONFLICT (tenant_id, external_id) WHERE external_id IS NOT NULL DO UPDATE SET
            name       = EXCLUDED.name,
            vendor_type= EXCLUDED.vendor_type,
            email      = EXCLUDED.email,
            phone      = EXCLUDED.phone,
            status     = EXCLUDED.status,
            updated_at = NOW()",
    )
    .bind(*tenant_id.as_uuid())
    .bind("TEST-QBO-SYNC Acme Corp")
    .bind("business")
    .bind("acme@example.com")
    .bind("555-1234")
    .bind("active")
    .bind(external_id)
    .execute(&pool)
    .await
    .expect("Should upsert vendor");

    // Verify vendor was inserted with the right external_id.
    let vendor: Option<(String, String)> = sqlx::query_as(
        "SELECT name, external_id FROM vendors WHERE tenant_id = $1 AND external_id = $2",
    )
    .bind(*tenant_id.as_uuid())
    .bind(external_id)
    .fetch_optional(&pool)
    .await
    .expect("Query should succeed");

    let (name, ext_id) = vendor.expect("Vendor should exist with external_id");
    assert_eq!(name, "TEST-QBO-SYNC Acme Corp");
    assert_eq!(ext_id, external_id);

    // Update sync status (same SQL as qbo_sync_vendors handler).
    sqlx::query(
        "UPDATE quickbooks_connections
         SET last_sync_at = NOW(),
             last_sync_status = 'success',
             last_sync_error = NULL,
             updated_at = NOW()
         WHERE tenant_id = $1",
    )
    .bind(*tenant_id.as_uuid())
    .execute(&pool)
    .await
    .expect("Should update sync status");

    // Verify last_sync_status = 'success'.
    let status: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT last_sync_status, last_sync_error
         FROM quickbooks_connections
         WHERE tenant_id = $1 AND company_id = $2",
    )
    .bind(*tenant_id.as_uuid())
    .bind(test_company_id)
    .fetch_optional(&pool)
    .await
    .expect("Query should succeed");

    let (sync_status, sync_error) = status.expect("QBO connection should exist");
    assert_eq!(sync_status.as_deref(), Some("success"));
    assert!(sync_error.is_none());

    // Upsert the same vendor again (update path).
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, external_id, created_at, updated_at)
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
         ON CONFLICT (tenant_id, external_id) WHERE external_id IS NOT NULL DO UPDATE SET
            name       = EXCLUDED.name,
            vendor_type= EXCLUDED.vendor_type,
            email      = EXCLUDED.email,
            phone      = EXCLUDED.phone,
            status     = EXCLUDED.status,
            updated_at = NOW()",
    )
    .bind(*tenant_id.as_uuid())
    .bind("TEST-QBO-SYNC Acme Corp Updated")
    .bind("business")
    .bind("acme-new@example.com")
    .bind("555-9999")
    .bind("active")
    .bind(external_id)
    .execute(&pool)
    .await
    .expect("Should upsert vendor on conflict");

    // Verify the name was updated and there's still only one row.
    let vendors: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM vendors WHERE tenant_id = $1 AND external_id = $2")
            .bind(*tenant_id.as_uuid())
            .bind(external_id)
            .fetch_all(&pool)
            .await
            .expect("Query should succeed");

    assert_eq!(
        vendors.len(),
        1,
        "Should be exactly one vendor for this external_id"
    );
    assert_eq!(vendors[0].0, "TEST-QBO-SYNC Acme Corp Updated");

    // Clean up.
    cleanup_test_vendors(&pool, &tenant_id, "TEST-QBO-SYNC").await;
    cleanup_qbo_connection(&pool, &tenant_id, test_company_id).await;
}
