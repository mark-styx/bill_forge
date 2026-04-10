//! Integration tests for tenant isolation across core tables
//!
//! Verifies that invoices, vendors, users, purchase orders, and receiving
//! records enforce tenant_id scoping so one tenant cannot read or modify
//! another tenant's data.
//!
//! These are integration tests requiring a running Postgres instance.
//! They are gated behind `#[cfg_attr(not(feature = "integration"), ignore)]`
//! so `cargo test` skips them by default but `cargo test --features integration`
//! (or `cargo test -- --ignored`) runs them.

use billforge_core::TenantId;
use billforge_db::PgManager;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Insert a minimal vendor row for the given tenant.
async fn seed_vendor(pool: &sqlx::PgPool, tenant_id: &TenantId, vendor_id: Uuid) {
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name)
         VALUES ($1, $2, $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("Test Vendor {}", vendor_id))
    .execute(pool)
    .await
    .expect("seed vendor");
}

/// Insert a minimal user row for the given tenant.
async fn seed_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("user-{}@test.com", user_id))
    .bind("hash")
    .bind("Test User")
    .execute(pool)
    .await
    .expect("seed user");
}

/// Insert a minimal invoice row for the given tenant.
/// Requires that the vendor and user already exist.
async fn seed_invoice(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
    vendor_id: Uuid,
    user_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by)
         VALUES ($1, $2, $3, $4, $5, 1000, $6, $7)",
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id)
    .bind("Test Vendor")
    .bind(format!("INV-{}", invoice_id))
    .bind(Uuid::new_v4()) // document_id
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed invoice");
}

/// Helper: create a minimal purchase_orders row + po_line_items row under a given tenant.
async fn seed_po_with_line(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    po_id: Uuid,
    vendor_id: Uuid,
    user_id: Uuid,
) {
    seed_vendor(pool, tenant_id, vendor_id).await;
    seed_user(pool, tenant_id, user_id).await;

    // Create PO
    sqlx::query(
        "INSERT INTO purchase_orders (id, tenant_id, po_number, vendor_id, vendor_name, order_date, total_amount_cents, created_by)
         VALUES ($1, $2, $3, $4, $5, CURRENT_DATE, 0, $6)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(po_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("PO-{}", po_id))
    .bind(vendor_id)
    .bind("Test Vendor")
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed PO");

    // Create line item
    sqlx::query(
        "INSERT INTO po_line_items (id, po_id, line_number, description, quantity, unit_of_measure, unit_price_cents, total_cents)
         VALUES ($1, $2, 1, 'Test item', 10, 'EA', 100, 1000)",
    )
    .bind(Uuid::new_v4())
    .bind(po_id)
    .execute(pool)
    .await
    .expect("seed line item");
}

/// Helper: create a receiving_records + receiving_line_items row under a given tenant.
async fn seed_receiving(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    po_id: Uuid,
    recv_id: Uuid,
    line_item_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO receiving_records (id, tenant_id, po_id, received_date)
         VALUES ($1, $2, $3, CURRENT_DATE)",
    )
    .bind(recv_id)
    .bind(*tenant_id.as_uuid())
    .bind(po_id)
    .execute(pool)
    .await
    .expect("seed receiving record");

    sqlx::query(
        "INSERT INTO receiving_line_items (id, receiving_id, po_line_number, quantity_received, quantity_damaged)
         VALUES ($1, $2, 1, 5, 0)",
    )
    .bind(line_item_id)
    .bind(recv_id)
    .execute(pool)
    .await
    .expect("seed receiving line item");
}

// ---------------------------------------------------------------------------
// Shared setup helper
// ---------------------------------------------------------------------------

/// Two-tenant test fixture. Creates (or re-creates) two tenant databases
/// and returns `(manager, tenant_a, tenant_b, pool_a, pool_b)`.
///
/// Callers should use unique `tag` values to avoid collisions when tests
/// run in parallel. The tag is embedded in the tenant UUID namespace.
async fn setup_two_tenants(
    tag: &str,
) -> (
    PgManager,
    TenantId,
    TenantId,
    sqlx::PgPool,
    sqlx::PgPool,
) {
    let metadata_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".to_string());
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("Failed to create PgManager");

    // Derive deterministic but tag-unique tenant IDs
    let tenant_a: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("tag-a-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();
    let tenant_b: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("tag-b-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();

    // Cleanup previous runs then create fresh
    manager.delete_tenant(&tenant_a).await.ok();
    manager.delete_tenant(&tenant_b).await.ok();
    manager.create_tenant(&tenant_a, &format!("Tenant A {}", tag)).await.expect("create tenant A");
    manager.create_tenant(&tenant_b, &format!("Tenant B {}", tag)).await.expect("create tenant B");

    let pool_a = (*manager.tenant(&tenant_a).await.expect("pool A")).clone();
    let pool_b = (*manager.tenant(&tenant_b).await.expect("pool B")).clone();

    // Run migrations so the tables exist
    manager.run_tenant_migrations(&pool_a).await.expect("migrate A");
    manager.run_tenant_migrations(&pool_b).await.expect("migrate B");

    (manager, tenant_a, tenant_b, pool_a, pool_b)
}

/// Teardown helper: drop both tenant databases.
async fn teardown_two_tenants(manager: &PgManager, tenant_a: &TenantId, tenant_b: &TenantId) {
    manager.delete_tenant(tenant_a).await.ok();
    manager.delete_tenant(tenant_b).await.ok();
}

// ===========================================================================
// PO tests (existing, un-ignored)
// ===========================================================================

/// Test: update_received_quantities should NOT update rows when tenant_id does not own the PO.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_update_received_qty_rejects_wrong_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("po-recv").await;

    let po_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Create PO under tenant A
    seed_po_with_line(&pool_a, &tenant_a, po_id, vendor_id, user_id).await;

    // Attempt to update received_quantity using tenant B's ID (cross-tenant)
    let result = sqlx::query(
        "UPDATE po_line_items SET received_quantity = received_quantity + $1
         WHERE po_id = $2 AND line_number = $3
           AND po_id IN (SELECT id FROM purchase_orders WHERE id = $2 AND tenant_id = $4)",
    )
    .bind(5.0_f64)
    .bind(po_id)
    .bind(1_i32)
    .bind(*tenant_b.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 0,
        "Cross-tenant update_received_quantities should affect 0 rows"
    );

    // Verify the correct tenant CAN update
    let result = sqlx::query(
        "UPDATE po_line_items SET received_quantity = received_quantity + $1
         WHERE po_id = $2 AND line_number = $3
           AND po_id IN (SELECT id FROM purchase_orders WHERE id = $2 AND tenant_id = $4)",
    )
    .bind(5.0_f64)
    .bind(po_id)
    .bind(1_i32)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 1,
        "Same-tenant update_received_quantities should affect 1 row"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Test: update_invoiced_quantities should NOT update rows when tenant_id does not own the PO.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_update_invoiced_qty_rejects_wrong_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("po-inv").await;

    let po_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    seed_po_with_line(&pool_a, &tenant_a, po_id, vendor_id, user_id).await;

    // Cross-tenant attempt with tenant B
    let result = sqlx::query(
        "UPDATE po_line_items SET invoiced_quantity = invoiced_quantity + $1
         WHERE po_id = $2 AND line_number = $3
           AND po_id IN (SELECT id FROM purchase_orders WHERE id = $2 AND tenant_id = $4)",
    )
    .bind(3.0_f64)
    .bind(po_id)
    .bind(1_i32)
    .bind(*tenant_b.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 0,
        "Cross-tenant update_invoiced_quantities should affect 0 rows"
    );

    // Same-tenant should succeed
    let result = sqlx::query(
        "UPDATE po_line_items SET invoiced_quantity = invoiced_quantity + $1
         WHERE po_id = $2 AND line_number = $3
           AND po_id IN (SELECT id FROM purchase_orders WHERE id = $2 AND tenant_id = $4)",
    )
    .bind(3.0_f64)
    .bind(po_id)
    .bind(1_i32)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 1,
        "Same-tenant update_invoiced_quantities should affect 1 row"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Test: run_match receiving query should exclude records from other tenants.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_run_match_excludes_cross_tenant_receiving() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("po-recv-match").await;

    let po_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let recv_id = Uuid::new_v4();
    let line_item_id = Uuid::new_v4();

    // Create PO + receiving under tenant A
    seed_po_with_line(&pool_a, &tenant_a, po_id, vendor_id, user_id).await;
    seed_receiving(&pool_a, &tenant_a, po_id, recv_id, line_item_id).await;

    // Query with tenant A should find the receiving line
    let rows_a: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT rl.id, rl.po_line_number, rl.quantity_received, rl.quantity_damaged, rl.product_id
         FROM receiving_line_items rl
         JOIN receiving_records rr ON rl.receiving_id = rr.id
         WHERE rr.po_id = $1 AND rr.tenant_id = $2",
    )
    .bind(po_id)
    .bind(*tenant_a.as_uuid())
    .fetch_all(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(rows_a.len(), 1, "Tenant A should see its receiving records");

    // Query with tenant B should find NOTHING
    let rows_b: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT rl.id, rl.po_line_number, rl.quantity_received, rl.quantity_damaged, rl.product_id
         FROM receiving_line_items rl
         JOIN receiving_records rr ON rl.receiving_id = rr.id
         WHERE rr.po_id = $1 AND rr.tenant_id = $2",
    )
    .bind(po_id)
    .bind(*tenant_b.as_uuid())
    .fetch_all(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(rows_b.len(), 0, "Tenant B should not see tenant A's receiving records");

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Invoice tenant isolation tests
// ===========================================================================

/// Querying an invoice by ID with the wrong tenant_id should return 0 rows.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_invoice_get_by_id_rejects_wrong_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("inv-get").await;

    let vendor_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();

    // Seed vendor + user + invoice under tenant A
    seed_vendor(&pool_a, &tenant_a, vendor_id).await;
    seed_user(&pool_a, &tenant_a, user_id).await;
    seed_invoice(&pool_a, &tenant_a, invoice_id, vendor_id, user_id).await;

    // Query with tenant B should see nothing
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(*tenant_b.as_uuid())
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_none(),
        "Cross-tenant invoice lookup should return None"
    );

    // Query with tenant A should find it
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(*tenant_a.as_uuid())
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_some(),
        "Same-tenant invoice lookup should return the invoice"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Listing invoices filtered by tenant_id should only return that tenant's invoices.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_invoice_list_excludes_other_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) =
        setup_two_tenants("inv-list").await;

    // Seed vendor + user under both tenants
    let vendor_a = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    seed_vendor(&pool_a, &tenant_a, vendor_a).await;
    seed_user(&pool_a, &tenant_a, user_a).await;

    let vendor_b = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    seed_vendor(&pool_b, &tenant_b, vendor_b).await;
    seed_user(&pool_b, &tenant_b, user_b).await;

    // Seed 2 invoices under tenant A, 3 under tenant B
    for _ in 0..2 {
        seed_invoice(&pool_a, &tenant_a, Uuid::new_v4(), vendor_a, user_a).await;
    }
    for _ in 0..3 {
        seed_invoice(&pool_b, &tenant_b, Uuid::new_v4(), vendor_b, user_b).await;
    }

    // Count for tenant A
    let count_a: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM invoices WHERE tenant_id = $1",
    )
    .bind(*tenant_a.as_uuid())
    .fetch_one(&pool_a)
    .await
    .expect("count A");

    assert_eq!(count_a.0, 2, "Tenant A should see exactly 2 invoices");

    // Count for tenant B
    let count_b: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM invoices WHERE tenant_id = $1",
    )
    .bind(*tenant_b.as_uuid())
    .fetch_one(&pool_b)
    .await
    .expect("count B");

    assert_eq!(count_b.0, 3, "Tenant B should see exactly 3 invoices");

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Vendor tenant isolation tests
// ===========================================================================

/// Querying a vendor by ID with the wrong tenant_id should return None.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_vendor_get_by_id_rejects_wrong_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("vnd-get").await;

    let vendor_id = Uuid::new_v4();
    seed_vendor(&pool_a, &tenant_a, vendor_id).await;

    // Cross-tenant query
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id)
    .bind(*tenant_b.as_uuid())
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_none(),
        "Cross-tenant vendor lookup should return None"
    );

    // Same-tenant query
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id)
    .bind(*tenant_a.as_uuid())
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_some(),
        "Same-tenant vendor lookup should return the vendor"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Deleting a vendor with the wrong tenant_id should affect 0 rows.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_vendor_delete_rejects_wrong_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("vnd-del").await;

    let vendor_id = Uuid::new_v4();
    seed_vendor(&pool_a, &tenant_a, vendor_id).await;

    // Cross-tenant delete attempt
    let result = sqlx::query(
        "DELETE FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id)
    .bind(*tenant_b.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 0,
        "Cross-tenant vendor delete should affect 0 rows"
    );

    // Verify vendor still exists for tenant A
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id)
    .bind(*tenant_a.as_uuid())
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_some(),
        "Vendor should still exist for the owning tenant after cross-tenant delete attempt"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// User tenant isolation tests
// ===========================================================================

/// Looking up a user by ID with the wrong tenant_id should return None.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_user_email_lookup_rejects_wrong_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("usr-lookup").await;

    let user_id = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_id).await;

    // Cross-tenant query
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT email FROM users WHERE tenant_id = $1 AND id = $2",
    )
    .bind(*tenant_b.as_uuid())
    .bind(user_id)
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_none(),
        "Cross-tenant user lookup should return None"
    );

    // Same-tenant query
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT email FROM users WHERE tenant_id = $1 AND id = $2",
    )
    .bind(*tenant_a.as_uuid())
    .bind(user_id)
    .fetch_optional(&pool_a)
    .await
    .expect("query executed");

    assert!(
        row.is_some(),
        "Same-tenant user lookup should return the user"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Listing users filtered by tenant_id should only return that tenant's users.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_user_list_excludes_other_tenant() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) =
        setup_two_tenants("usr-list").await;

    // Seed 2 users under tenant A
    seed_user(&pool_a, &tenant_a, Uuid::new_v4()).await;
    seed_user(&pool_a, &tenant_a, Uuid::new_v4()).await;

    // Seed 3 users under tenant B
    seed_user(&pool_b, &tenant_b, Uuid::new_v4()).await;
    seed_user(&pool_b, &tenant_b, Uuid::new_v4()).await;
    seed_user(&pool_b, &tenant_b, Uuid::new_v4()).await;

    let count_a: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM users WHERE tenant_id = $1",
    )
    .bind(*tenant_a.as_uuid())
    .fetch_one(&pool_a)
    .await
    .expect("count A");

    assert_eq!(count_a.0, 2, "Tenant A should see exactly 2 users");

    let count_b: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM users WHERE tenant_id = $1",
    )
    .bind(*tenant_b.as_uuid())
    .fetch_one(&pool_b)
    .await
    .expect("count B");

    assert_eq!(count_b.0, 3, "Tenant B should see exactly 3 users");

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Queue item claim/complete tenant isolation tests
// ===========================================================================

/// Claiming a queue item with the wrong tenant_id should affect 0 rows.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_claim_item_cross_tenant_blocked() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("qi-claim").await;

    let vendor_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let queue_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();

    // Seed prerequisites + a queue item under tenant A
    seed_vendor(&pool_a, &tenant_a, vendor_id).await;
    seed_user(&pool_a, &tenant_a, user_id).await;
    seed_invoice(&pool_a, &tenant_a, invoice_id, vendor_id, user_id).await;

    sqlx::query(
        "INSERT INTO work_queues (id, tenant_id, name, queue_type)
         VALUES ($1, $2, 'Test Queue', 'approval')",
    )
    .bind(queue_id)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("seed work queue");

    sqlx::query(
        "INSERT INTO queue_items (id, tenant_id, queue_id, invoice_id, assigned_to, status, priority, entered_at)
         VALUES ($1, $2, $3, $4, NULL, 'pending', 0, NOW())",
    )
    .bind(item_id)
    .bind(*tenant_a.as_uuid())
    .bind(queue_id)
    .bind(invoice_id)
    .execute(&pool_a)
    .await
    .expect("seed queue item");

    // Cross-tenant claim attempt with tenant B
    let result = sqlx::query(
        "UPDATE queue_items SET assigned_to = $1, claimed_at = $2 WHERE id = $3 AND tenant_id = $4",
    )
    .bind(user_id)
    .bind(chrono::Utc::now())
    .bind(item_id)
    .bind(*tenant_b.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 0,
        "Cross-tenant claim should affect 0 rows"
    );

    // Verify the item is still unclaimed
    let claimed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT claimed_at FROM queue_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_one(&pool_a)
    .await
    .expect("query executed");

    assert!(
        claimed_at.is_none(),
        "Item should remain unclaimed after cross-tenant attempt"
    );

    // Same-tenant claim should succeed
    let result = sqlx::query(
        "UPDATE queue_items SET assigned_to = $1, claimed_at = $2 WHERE id = $3 AND tenant_id = $4",
    )
    .bind(user_id)
    .bind(chrono::Utc::now())
    .bind(item_id)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 1,
        "Same-tenant claim should affect 1 row"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Completing a queue item with the wrong tenant_id should affect 0 rows.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_complete_item_cross_tenant_blocked() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("qi-complete").await;

    let vendor_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let queue_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();

    // Seed prerequisites + a claimed queue item under tenant A
    seed_vendor(&pool_a, &tenant_a, vendor_id).await;
    seed_user(&pool_a, &tenant_a, user_id).await;
    seed_invoice(&pool_a, &tenant_a, invoice_id, vendor_id, user_id).await;

    sqlx::query(
        "INSERT INTO work_queues (id, tenant_id, name, queue_type)
         VALUES ($1, $2, 'Test Queue', 'approval')",
    )
    .bind(queue_id)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("seed work queue");

    sqlx::query(
        "INSERT INTO queue_items (id, tenant_id, queue_id, invoice_id, assigned_to, status, priority, entered_at, claimed_at)
         VALUES ($1, $2, $3, $4, $5, 'claimed', 0, NOW(), NOW())",
    )
    .bind(item_id)
    .bind(*tenant_a.as_uuid())
    .bind(queue_id)
    .bind(invoice_id)
    .bind(user_id)
    .execute(&pool_a)
    .await
    .expect("seed claimed queue item");

    // Cross-tenant complete attempt with tenant B
    let result = sqlx::query(
        "UPDATE queue_items SET completed_at = $1, completion_action = $2 WHERE id = $3 AND tenant_id = $4",
    )
    .bind(chrono::Utc::now())
    .bind("approve")
    .bind(item_id)
    .bind(*tenant_b.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 0,
        "Cross-tenant complete should affect 0 rows"
    );

    // Verify the item is still incomplete
    let completed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT completed_at FROM queue_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_one(&pool_a)
    .await
    .expect("query executed");

    assert!(
        completed_at.is_none(),
        "Item should remain incomplete after cross-tenant attempt"
    );

    // Same-tenant complete should succeed
    let result = sqlx::query(
        "UPDATE queue_items SET completed_at = $1, completion_action = $2 WHERE id = $3 AND tenant_id = $4",
    )
    .bind(chrono::Utc::now())
    .bind("approve")
    .bind(item_id)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 1,
        "Same-tenant complete should affect 1 row"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Reassigning a queue item with the wrong tenant_id should affect 0 rows.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_reassign_item_cross_tenant_blocked() {
    let (manager, tenant_a, tenant_b, pool_a, _pool_b) =
        setup_two_tenants("qi-reassign").await;

    let vendor_id = Uuid::new_v4();
    let user_id_a = Uuid::new_v4();
    let user_id_b = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let queue_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();

    // Seed prerequisites + a queue item under tenant A
    seed_vendor(&pool_a, &tenant_a, vendor_id).await;
    seed_user(&pool_a, &tenant_a, user_id_a).await;
    seed_user(&pool_a, &tenant_a, user_id_b).await;
    seed_invoice(&pool_a, &tenant_a, invoice_id, vendor_id, user_id_a).await;

    sqlx::query(
        "INSERT INTO work_queues (id, tenant_id, name, queue_type)
         VALUES ($1, $2, 'Test Queue', 'approval')",
    )
    .bind(queue_id)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("seed work queue");

    sqlx::query(
        "INSERT INTO queue_items (id, tenant_id, queue_id, invoice_id, assigned_to, status, priority, entered_at)
         VALUES ($1, $2, $3, $4, $5, 'pending', 0, NOW())",
    )
    .bind(item_id)
    .bind(*tenant_a.as_uuid())
    .bind(queue_id)
    .bind(invoice_id)
    .bind(user_id_a)
    .execute(&pool_a)
    .await
    .expect("seed queue item");

    // Cross-tenant reassign attempt with tenant B
    let result = sqlx::query(
        "UPDATE queue_items SET assigned_to = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4",
    )
    .bind(user_id_b)
    .bind(chrono::Utc::now())
    .bind(item_id)
    .bind(*tenant_b.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 0,
        "Cross-tenant reassign should affect 0 rows"
    );

    // Verify the item's assigned_to is unchanged
    let current_assigned: Option<Uuid> = sqlx::query_scalar(
        "SELECT assigned_to FROM queue_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_one(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        current_assigned, Some(user_id_a),
        "Item should still be assigned to user_id_a after cross-tenant attempt"
    );

    // Same-tenant reassign should succeed
    let result = sqlx::query(
        "UPDATE queue_items SET assigned_to = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4",
    )
    .bind(user_id_b)
    .bind(chrono::Utc::now())
    .bind(item_id)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("query executed");

    assert_eq!(
        result.rows_affected(), 1,
        "Same-tenant reassign should affect 1 row"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Payment request items tenant isolation tests
// ===========================================================================

/// Test: `get_payment_request` items query should only return invoices owned by
/// the same tenant, even if a stray cross-tenant `payment_request_items` row exists.
///
/// Defense-in-depth: the JOIN on `invoices` now includes `i.tenant_id = $2`, so a
/// corrupted item row pointing at another tenant's invoice is silently excluded.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn payment_request_items_query_is_tenant_scoped() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) =
        setup_two_tenants("pr-items-scope").await;

    let vendor_id_a = Uuid::new_v4();
    let user_id_a = Uuid::new_v4();
    let invoice_id_a = Uuid::new_v4();

    let vendor_id_b = Uuid::new_v4();
    let user_id_b = Uuid::new_v4();
    let invoice_id_b = Uuid::new_v4();

    // Seed tenant A: vendor, user, invoice (ready_for_payment)
    seed_vendor(&pool_a, &tenant_a, vendor_id_a).await;
    seed_user(&pool_a, &tenant_a, user_id_a).await;
    seed_invoice(&pool_a, &tenant_a, invoice_id_a, vendor_id_a, user_id_a).await;

    // Seed tenant B: vendor, user, invoice
    seed_vendor(&pool_b, &tenant_b, vendor_id_b).await;
    seed_user(&pool_b, &tenant_b, user_id_b).await;
    seed_invoice(&pool_b, &tenant_b, invoice_id_b, vendor_id_b, user_id_b).await;

    // Insert a payment_requests row for tenant A via raw SQL
    let request_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO payment_requests
            (id, tenant_id, request_number, status, total_amount_cents, currency,
             invoice_count, created_by, created_at, updated_at)
           VALUES ($1, $2, 'PR-TEST-001', 'draft', 1000, 'USD', 1, $3, $4, $5)"#,
    )
    .bind(request_id)
    .bind(*tenant_a.as_uuid())
    .bind(user_id_a)
    .bind(now)
    .bind(now)
    .execute(&pool_a)
    .await
    .expect("insert payment_requests");

    // Insert a legitimate payment_request_items row for tenant A's invoice
    let item_id_a = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO payment_request_items
            (id, payment_request_id, invoice_id, amount_cents, currency, created_at)
           VALUES ($1, $2, $3, 1000, 'USD', $4)"#,
    )
    .bind(item_id_a)
    .bind(request_id)
    .bind(invoice_id_a)
    .bind(now)
    .execute(&pool_a)
    .await
    .expect("insert payment_request_items for tenant A invoice");

    // Sanity check: tenant B cannot see the parent request at all
    let repo_b = billforge_db::PaymentRequestRepositoryImpl::new(std::sync::Arc::new(pool_b.clone()));
    let result = repo_b.get_payment_request(&tenant_b, request_id).await;
    assert!(
        matches!(result, Ok(None)),
        "Cross-tenant get_payment_request should return Ok(None)"
    );

    // Defense-in-depth: insert a stray cross-tenant item row pointing at tenant B's invoice
    let stray_item_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO payment_request_items
            (id, payment_request_id, invoice_id, amount_cents, currency, created_at)
           VALUES ($1, $2, $3, 9999, 'USD', $4)"#,
    )
    .bind(stray_item_id)
    .bind(request_id)
    .bind(invoice_id_b)
    .bind(now)
    .execute(&pool_a)
    .await
    .expect("insert stray cross-tenant payment_request_item");

    // Query as tenant A: should only see the tenant A invoice, not the stray B row
    let repo_a = billforge_db::PaymentRequestRepositoryImpl::new(std::sync::Arc::new(pool_a.clone()));
    let result = repo_a.get_payment_request(&tenant_a, request_id).await;
    assert!(result.is_ok(), "get_payment_request should succeed for tenant A");

    let (_request, items) = result.unwrap().expect("should find the request");
    assert_eq!(items.len(), 1, "Should only return 1 item (tenant A's invoice), not the cross-tenant stray");
    assert_eq!(items[0].invoice_id, invoice_id_a, "The returned item should be tenant A's invoice");

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

/// Test: `add_invoices_to_request` aggregate recompute should exclude cross-tenant
/// invoice data. If a stray `payment_request_items` row points at another tenant's
/// invoice, the recomputed `total_amount_cents` and `invoice_count` must not include it.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn add_invoices_to_request_recompute_is_tenant_scoped() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) =
        setup_two_tenants("pr-add-inv-scope").await;

    let vendor_id_a = Uuid::new_v4();
    let user_id_a = Uuid::new_v4();
    let invoice_id_a1 = Uuid::new_v4();
    let invoice_id_a2 = Uuid::new_v4();

    let vendor_id_b = Uuid::new_v4();
    let user_id_b = Uuid::new_v4();
    let invoice_id_b = Uuid::new_v4();

    // Seed tenant A: vendor, user, two invoices
    seed_vendor(&pool_a, &tenant_a, vendor_id_a).await;
    seed_user(&pool_a, &tenant_a, user_id_a).await;

    // Invoice A1: 1000 cents, ready_for_payment
    seed_invoice(&pool_a, &tenant_a, invoice_id_a1, vendor_id_a, user_id_a).await;
    sqlx::query("UPDATE invoices SET processing_status = 'ready_for_payment' WHERE id = $1")
        .bind(invoice_id_a1)
        .execute(&pool_a)
        .await
        .expect("set invoice A1 to ready_for_payment");

    // Invoice A2: 2000 cents, ready_for_payment (will be added later via repo)
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by, processing_status)
         VALUES ($1, $2, $3, 'Test Vendor A', $4, 2000, $5, $6, 'ready_for_payment')",
    )
    .bind(invoice_id_a2)
    .bind(*tenant_a.as_uuid())
    .bind(vendor_id_a)
    .bind(format!("INV-A2-{}", invoice_id_a2))
    .bind(Uuid::new_v4())
    .bind(user_id_a)
    .execute(&pool_a)
    .await
    .expect("seed invoice A2");

    // Seed tenant B: vendor, user, invoice (5000 cents)
    seed_vendor(&pool_b, &tenant_b, vendor_id_b).await;
    seed_user(&pool_b, &tenant_b, user_id_b).await;
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by)
         VALUES ($1, $2, $3, 'Test Vendor B', $4, 5000, $5, $6)",
    )
    .bind(invoice_id_b)
    .bind(*tenant_b.as_uuid())
    .bind(vendor_id_b)
    .bind(format!("INV-B-{}", invoice_id_b))
    .bind(Uuid::new_v4())
    .bind(user_id_b)
    .execute(&pool_b)
    .await
    .expect("seed invoice B");

    // Create a draft payment request for tenant A via repo (with invoice A1)
    let repo_a = billforge_db::PaymentRequestRepositoryImpl::new(std::sync::Arc::new(pool_a.clone()));
    let pr = repo_a
        .create_payment_request(&tenant_a, user_id_a, &[invoice_id_a1], None)
        .await
        .expect("create payment request");
    assert_eq!(pr.total_amount_cents, 1000);
    assert_eq!(pr.invoice_count, 1);

    // Manually insert a stray cross-tenant item row pointing at tenant B's invoice
    let stray_item_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO payment_request_items
            (id, payment_request_id, invoice_id, amount_cents, currency, created_at)
           VALUES ($1, $2, $3, 5000, 'USD', NOW())"#,
    )
    .bind(stray_item_id)
    .bind(pr.id)
    .bind(invoice_id_b)
    .execute(&pool_a)
    .await
    .expect("insert stray cross-tenant item");

    // Now add invoice A2 via the repo. The aggregate recompute should only count
    // invoices belonging to tenant A (1000 + 2000 = 3000), NOT the stray B row (5000).
    let _items = repo_a
        .add_invoices_to_request(&tenant_a, pr.id, &[invoice_id_a2])
        .await
        .expect("add invoices to request");

    // Verify the recomputed totals on the payment_requests row
    let row: (i64, i32) = sqlx::query_as(
        "SELECT total_amount_cents, invoice_count FROM payment_requests WHERE id = $1",
    )
    .bind(pr.id)
    .fetch_one(&pool_a)
    .await
    .expect("fetch updated payment request");

    assert_eq!(
        row.0, 3000,
        "total_amount_cents should be 3000 (A1=1000 + A2=2000), excluding stray tenant B invoice (5000). Got {}",
        row.0
    );
    assert_eq!(
        row.1, 2,
        "invoice_count should be 2 (only tenant A invoices), not 3. Got {}",
        row.1
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}
