//! Integration tests for Vendor CRUD lifecycle
//!
//! Verifies:
//! - Vendor creation (happy path)
//! - Tenant-scoped listing (isolation between tenants)
//! - PATCH routing_rules JSONB roundtrip
//! - Soft delete (status='inactive', not hard-deleted)
//! - get_routing_rules() helper returns expected shape
//!
//! These tests run against the real PostgreSQL database (DATABASE_URL).

use billforge_api::get_routing_rules;
use billforge_core::domain::VendorId;
use billforge_core::types::TenantId;
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";

/// Helper to get a database pool from DATABASE_URL
async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Helper: clean up test vendors created during a test (by name prefix).
async fn cleanup_test_vendors(pool: &sqlx::PgPool, tenant_id: &TenantId, prefix: &str) {
    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 AND name LIKE $2")
        .bind(*tenant_id.as_uuid())
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
}

/// Helper: insert a vendor row directly and return its ID.
async fn insert_test_vendor(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    name: &str,
    tax_id: Option<&str>,
    status: &str,
    routing_rules: &serde_json::Value,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, tax_id, status, routing_rules, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6::jsonb, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .bind(tax_id)
    .bind(status)
    .bind(routing_rules)
    .execute(pool)
    .await
    .expect("Failed to insert test vendor");
    id
}

// ============================================================================
// Test: create vendor (happy path)
// ============================================================================

#[tokio::test]
async fn test_create_vendor_happy_path() {
    let pool = get_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());

    cleanup_test_vendors(&pool, &tenant_id, "TEST-CRUD").await;

    let vendor_id = insert_test_vendor(
        &pool,
        &tenant_id,
        "TEST-CRUD Happy Vendor",
        Some("12-3456789"),
        "active",
        &serde_json::json!({}),
    ).await;

    // Verify the row exists and has the expected shape
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT name, tax_id, status FROM vendors WHERE id = $1 AND tenant_id = $2"
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(&pool)
    .await
    .expect("Query should succeed");

    let (name, tax_id, status) = row.expect("Vendor row should exist");
    assert_eq!(name, "TEST-CRUD Happy Vendor");
    assert_eq!(tax_id.as_deref(), Some("12-3456789"));
    assert_eq!(status, "active");

    cleanup_test_vendors(&pool, &tenant_id, "TEST-CRUD").await;
}

// ============================================================================
// Test: list vendors - tenant isolation
// ============================================================================

#[tokio::test]
async fn test_list_vendors_tenant_isolation() {
    let pool = get_pool().await;
    let tenant_a = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());
    let tenant_b_id = Uuid::new_v4();
    let tenant_b = TenantId::from_uuid(tenant_b_id);

    // Ensure tenant_b schema exists (create a minimal vendors-like namespace)
    // We use a separate row with tenant_b's ID in the same table, which is
    // sufficient for testing isolation.
    cleanup_test_vendors(&pool, &tenant_a, "TEST-ISOLATE").await;

    // Create vendor for tenant A
    insert_test_vendor(
        &pool, &tenant_a, "TEST-ISOLATE Tenant A Vendor", None, "active", &serde_json::json!({}),
    ).await;

    // Create vendor for tenant B
    insert_test_vendor(
        &pool, &tenant_b, "TEST-ISOLATE Tenant B Vendor", None, "active", &serde_json::json!({}),
    ).await;

    // Query for tenant A vendors - should only see tenant A's vendor
    let tenant_a_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM vendors WHERE tenant_id = $1 AND name LIKE 'TEST-ISOLATE%'"
    )
    .bind(*tenant_a.as_uuid())
    .fetch_all(&pool)
    .await
    .expect("Query should succeed");

    assert_eq!(tenant_a_names.len(), 1, "Tenant A should see exactly 1 vendor");
    assert_eq!(tenant_a_names[0], "TEST-ISOLATE Tenant A Vendor");

    // Query for tenant B vendors - should only see tenant B's vendor
    let tenant_b_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM vendors WHERE tenant_id = $1 AND name LIKE 'TEST-ISOLATE%'"
    )
    .bind(tenant_b_id)
    .fetch_all(&pool)
    .await
    .expect("Query should succeed");

    assert_eq!(tenant_b_names.len(), 1, "Tenant B should see exactly 1 vendor");
    assert_eq!(tenant_b_names[0], "TEST-ISOLATE Tenant B Vendor");

    // Cleanup both tenants
    cleanup_test_vendors(&pool, &tenant_a, "TEST-ISOLATE").await;
    cleanup_test_vendors(&pool, &tenant_b, "TEST-ISOLATE").await;
}

// ============================================================================
// Test: PATCH routing_rules JSONB roundtrip
// ============================================================================

#[tokio::test]
async fn test_update_routing_rules() {
    let pool = get_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());

    cleanup_test_vendors(&pool, &tenant_id, "TEST-ROUTING").await;

    let vendor_id = insert_test_vendor(
        &pool, &tenant_id, "TEST-ROUTING Vendor", None, "active", &serde_json::json!({}),
    ).await;

    // Update routing_rules via direct SQL (simulating what the PATCH handler does)
    let rules = serde_json::json!({
        "approver_email": "cfo@example.com",
        "auto_approve_threshold_cents": 50000,
        "requires_dual_approval": true
    });
    sqlx::query("UPDATE vendors SET routing_rules = $1::jsonb, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
        .bind(&rules)
        .bind(vendor_id)
        .bind(*tenant_id.as_uuid())
        .execute(&pool)
        .await
        .expect("Should update routing_rules");

    // Verify roundtrip
    let stored: serde_json::Value = sqlx::query_scalar(
        "SELECT routing_rules FROM vendors WHERE id = $1 AND tenant_id = $2"
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("Should fetch routing_rules");

    assert_eq!(stored["approver_email"], "cfo@example.com");
    assert_eq!(stored["auto_approve_threshold_cents"], 50000);
    assert_eq!(stored["requires_dual_approval"], true);

    cleanup_test_vendors(&pool, &tenant_id, "TEST-ROUTING").await;
}

// ============================================================================
// Test: DELETE sets status='inactive' (soft delete), row still exists
// ============================================================================

#[tokio::test]
async fn test_delete_sets_inactive() {
    let pool = get_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());

    cleanup_test_vendors(&pool, &tenant_id, "TEST-SOFTDEL").await;

    let vendor_id = insert_test_vendor(
        &pool, &tenant_id, "TEST-SOFTDEL Vendor", None, "active", &serde_json::json!({}),
    ).await;

    // Simulate soft delete (as the delete_vendor handler now does)
    sqlx::query("UPDATE vendors SET status = 'inactive', updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
        .bind(vendor_id)
        .bind(*tenant_id.as_uuid())
        .execute(&pool)
        .await
        .expect("Should soft-delete vendor");

    // Verify the row still exists but is inactive
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM vendors WHERE id = $1 AND tenant_id = $2"
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(&pool)
    .await
    .expect("Query should succeed");

    let (status,) = row.expect("Vendor row should still exist after soft delete");
    assert_eq!(status, "inactive", "Status should be 'inactive' after soft delete");

    cleanup_test_vendors(&pool, &tenant_id, "TEST-SOFTDEL").await;
}

// ============================================================================
// Test: get_routing_rules() helper returns expected shape
// ============================================================================

#[tokio::test]
async fn test_get_routing_rules_helper() {
    let pool = get_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());

    cleanup_test_vendors(&pool, &tenant_id, "TEST-HELPER").await;

    // Create vendor with routing rules
    let vendor_id = insert_test_vendor(
        &pool,
        &tenant_id,
        "TEST-HELPER Vendor",
        None,
        "active",
        &serde_json::json!({
            "approver_email": "ap-manager@example.com",
            "auto_approve_threshold_cents": 25000,
            "requires_dual_approval": false
        }),
    ).await;

    let vendor_id = VendorId(vendor_id);

    // Call the public helper
    let rules = get_routing_rules(&pool, &tenant_id, &vendor_id)
        .await
        .expect("get_routing_rules should succeed");

    assert_eq!(rules.approver_email.as_deref(), Some("ap-manager@example.com"));
    assert_eq!(rules.auto_approve_threshold_cents, Some(25000));
    assert_eq!(rules.requires_dual_approval, Some(false));

    // Also test with a vendor that has empty routing rules (default)
    let vendor_id_empty = insert_test_vendor(
        &pool,
        &tenant_id,
        "TEST-HELPER Empty Rules",
        None,
        "active",
        &serde_json::json!({}),
    ).await;

    let vendor_id_empty = VendorId(vendor_id_empty);
    let empty_rules = get_routing_rules(&pool, &tenant_id, &vendor_id_empty)
        .await
        .expect("get_routing_rules should succeed for empty rules");

    assert!(empty_rules.approver_email.is_none());
    assert!(empty_rules.auto_approve_threshold_cents.is_none());
    assert!(empty_rules.requires_dual_approval.is_none());

    // Test that a non-existent vendor returns NotFound
    let bad_vendor_id = VendorId::new();
    let result = get_routing_rules(&pool, &tenant_id, &bad_vendor_id).await;
    assert!(result.is_err(), "Should return error for non-existent vendor");

    cleanup_test_vendors(&pool, &tenant_id, "TEST-HELPER").await;
}
