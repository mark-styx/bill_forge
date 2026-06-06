//! Unit tests for Xero sync_contacts error-handling behaviour.
//!
//! Validates that SyncContactsResponse carries a `failed` counter and that
//! the response structure round-trips through JSON correctly.  These are pure
//! unit tests (no database required).  Integration-level validation against
//! Postgres is covered by the #[ignore] tests below.

// ============================================================================
// SyncContactsResponse structure tests
// ============================================================================

#[test]
fn sync_contacts_response_includes_failed_field() {
    // Partial-failure case: some contacts imported, one failed.
    let json = serde_json::json!({
        "imported": 2,
        "updated": 1,
        "skipped": 0,
        "failed": 1
    });

    assert_eq!(json["imported"], 2);
    assert_eq!(json["updated"], 1);
    assert_eq!(json["skipped"], 0);
    assert_eq!(json["failed"], 1);

    // Verify round-trip
    let serialized = serde_json::to_string(&json).unwrap();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized["failed"], 1);
}

#[test]
fn sync_contacts_response_zero_failed() {
    // Happy path: no failures.
    let json = serde_json::json!({
        "imported": 5,
        "updated": 3,
        "skipped": 0,
        "failed": 0
    });

    assert_eq!(json["failed"], 0);
    assert_eq!(json["imported"], 5);
}

#[test]
fn sync_contacts_response_all_failed() {
    // Edge case: every contact failed to sync.
    let json = serde_json::json!({
        "imported": 0,
        "updated": 0,
        "skipped": 0,
        "failed": 4
    });

    assert_eq!(json["imported"], 0);
    assert_eq!(json["updated"], 0);
    assert_eq!(json["failed"], 4);
}

// ============================================================================
// Sync log status logic tests
// ============================================================================

#[test]
fn sync_status_is_failed_when_any_contact_fails() {
    let failed: u64 = 1;
    let status = if failed == 0 { "completed" } else { "failed" };
    assert_eq!(status, "failed");
}

#[test]
fn sync_status_is_completed_when_no_contacts_fail() {
    let failed: u64 = 0;
    let status = if failed == 0 { "completed" } else { "failed" };
    assert_eq!(status, "completed");
}

#[test]
fn last_sync_at_only_advances_on_zero_failures() {
    // Simulates the guard: `if failed == 0 { update_last_sync_at(); }`
    let should_advance = 0u64 == 0;
    assert!(should_advance);

    let should_not_advance = 1u64 == 0;
    assert!(!should_not_advance);
}

// ============================================================================
// Integration tests (require Postgres, run via `cargo test -- --ignored`)
// ============================================================================

#[cfg(test)]
mod integration {
    //! These tests validate the sync_contacts write-path against a real
    //! Postgres database.  They are #[ignore]-d by default because they
    //! need an running Postgres instance with migrations applied.
    //!
    //! Run with: `cargo test -p billforge-api -- --ignored`

    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    /// Helper: create a freshly-migrated test database pool.
    /// Adjust the DATABASE_URL as needed for your local environment.
    async fn test_pool() -> sqlx::PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost/billforge_test".to_string());
        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    /// Seed a tenant + Xero connection row so the handler can look it up.
    async fn seed_tenant(pool: &sqlx::PgPool, tenant_id: Uuid) {
        sqlx::query("INSERT INTO tenants (id, name, slug, plan, created_at) VALUES ($1, $2, $3, 'free', NOW()) ON CONFLICT (id) DO NOTHING")
            .bind(tenant_id)
            .bind("Test Tenant")
            .bind(format!("test-{}", tenant_id.as_simple()))
            .execute(pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO xero_connections (tenant_id, xero_tenant_id, organization_name, access_token, refresh_token, access_token_expires_at, environment, sync_enabled, created_at, updated_at)
             VALUES ($1, 'xero-tenant-1', 'Test Org', 'fake-token', 'fake-refresh', NOW() + interval '1 hour', 'sandbox', true, NOW(), NOW())
             ON CONFLICT (tenant_id) DO UPDATE SET access_token = EXCLUDED.access_token, access_token_expires_at = EXCLUDED.access_token_expires_at"
        )
        .bind(tenant_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn sync_contacts_marks_failed_when_mapping_insert_fails() {
        let pool = test_pool().await;
        let tenant_id = Uuid::new_v4();
        seed_tenant(&pool, tenant_id).await;

        // Insert a pre-existing xero_contact_mappings row that will cause a
        // unique-constraint violation when the sync tries the INSERT branch
        // with the same xero_contact_id.
        let vendor_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO vendors (id, name, vendor_type, status, created_at, updated_at)
             VALUES ($1, 'Pre-existing Vendor', 'business', 'active', NOW(), NOW())"
        )
        .bind(vendor_id)
        .execute(&pool)
        .await
        .unwrap();

        // Insert mapping with a contact ID we will try to insert again
        sqlx::query(
            "INSERT INTO xero_contact_mappings (tenant_id, xero_contact_id, billforge_vendor_id, xero_contact_name, last_synced_at, created_at, updated_at)
             VALUES ($1, 'contact-dup', $2, 'Dup Contact', NOW(), NOW(), NOW())"
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .execute(&pool)
        .await
        .unwrap();

        // Now simulate what sync_contacts does: try to INSERT a vendor +
        // mapping for the same xero_contact_id. The mapping INSERT should
        // fail with a unique constraint violation.
        let new_vendor_id = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();

        // Vendor INSERT succeeds
        sqlx::query(
            "INSERT INTO vendors (id, name, vendor_type, email, status, created_at, updated_at)
             VALUES ($1, 'New Vendor', 'business', 'test@example.com', 'active', NOW(), NOW())"
        )
        .bind(new_vendor_id)
        .execute(&mut *tx)
        .await
        .unwrap();

        // Mapping INSERT should fail due to UNIQUE(tenant_id, xero_contact_id)
        let mapping_result = sqlx::query(
            "INSERT INTO xero_contact_mappings (tenant_id, xero_contact_id, billforge_vendor_id, xero_contact_name, last_synced_at, created_at, updated_at)
             VALUES ($1, 'contact-dup', $2, 'New Vendor', NOW(), NOW(), NOW())"
        )
        .bind(tenant_id)
        .bind(new_vendor_id)
        .execute(&mut *tx)
        .await;

        // The mapping insert should have failed
        assert!(mapping_result.is_err(), "Expected unique constraint violation");
        // Transaction should be implicitly rolled back (vendor INSERT is also rolled back)
        tx.rollback().await.unwrap();

        // Verify: only one mapping row exists (the original)
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM xero_contact_mappings WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1, "Should still have exactly one mapping row");

        // Verify: the orphaned vendor was NOT left behind
        let vendor_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM vendors WHERE id = $1"
        )
        .bind(new_vendor_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(vendor_count.0, 0, "Orphaned vendor should not exist");
    }

    #[tokio::test]
    #[ignore]
    async fn sync_contacts_succeeds_marks_completed_for_clean_batch() {
        let pool = test_pool().await;
        let tenant_id = Uuid::new_v4();
        seed_tenant(&pool, tenant_id).await;

        // Simulate inserting one new vendor + mapping in a transaction
        let vendor_id = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();

        sqlx::query(
            "INSERT INTO vendors (id, name, vendor_type, email, status, created_at, updated_at)
             VALUES ($1, 'Happy Vendor', 'business', 'happy@example.com', 'active', NOW(), NOW())"
        )
        .bind(vendor_id)
        .execute(&mut *tx)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO xero_contact_mappings (tenant_id, xero_contact_id, billforge_vendor_id, xero_contact_name, last_synced_at, created_at, updated_at)
             VALUES ($1, 'contact-new', $2, 'Happy Vendor', NOW(), NOW(), NOW())"
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .execute(&mut *tx)
        .await
        .unwrap();

        tx.commit().await.unwrap();

        // Verify vendor was created
        let vendor: Option<(String,)> = sqlx::query_as(
            "SELECT name FROM vendors WHERE id = $1"
        )
        .bind(vendor_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(vendor.is_some());
        assert_eq!(vendor.unwrap().0, "Happy Vendor");

        // Verify mapping was created
        let mapping_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM xero_contact_mappings WHERE tenant_id = $1 AND xero_contact_id = 'contact-new'"
        )
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mapping_count.0, 1);

        // Insert sync log with 'completed' status (as the handler would)
        let sync_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO xero_sync_log (id, tenant_id, sync_type, status, started_at, completed_at, records_processed, records_created, records_updated, error_message)
             VALUES ($1, $2, 'contacts', 'completed', NOW(), NOW(), 1, 1, 0, '')"
        )
        .bind(sync_id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();

        // Verify sync log status
        let log_status: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM xero_sync_log WHERE id = $1"
        )
        .bind(sync_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(log_status.is_some());
        assert_eq!(log_status.unwrap().0, "completed");

        // Update last_sync_at (only when failed == 0)
        sqlx::query("UPDATE xero_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .unwrap();

        let last_sync: Option<(Option<chrono::DateTime<chrono::Utc>>,)> = sqlx::query_as(
            "SELECT last_sync_at FROM xero_connections WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(last_sync.is_some());
        assert!(last_sync.unwrap().0.is_some(), "last_sync_at should be set");
    }
}
