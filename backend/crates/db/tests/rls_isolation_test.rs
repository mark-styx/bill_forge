//! Row-Level Security integration tests
//!
//! Verifies that PostgreSQL RLS policies on core tenant tables (invoices, users,
//! vendors) enforce tenant isolation at the database level, independent of
//! application-layer WHERE clauses.
//!
//! Scenarios covered:
//! 1. With the correct session variable, rows are visible
//! 2. With no session variable, SELECT * returns 0 rows (defense-in-depth)
//! 3. With a wrong session variable, SELECT * returns 0 rows
//! 4. INSERT with a mismatched tenant_id is blocked by WITH CHECK
//! 5. UPDATE on a row belonging to another tenant is blocked
//! 6. DELETE on a row belonging to another tenant is blocked
//!
//! Run in CI:
//!   cargo test -p billforge-db --test rls_isolation_test --features integration
//!
//! Run locally (requires Postgres):
//!   cargo test -p billforge-db --test rls_isolation_test -- --ignored

use billforge_core::TenantId;
use billforge_db::PgManager;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a single tenant database with all migrations (including 080_enable_rls).
/// Returns the manager, tenant ID, and pool.
async fn setup_rls_tenant(tag: &str) -> (PgManager, TenantId, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("Failed to create PgManager");

    let tenant_id: TenantId =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("rls-tenant-{tag}").as_bytes())
            .to_string()
            .parse()
            .unwrap();

    // Cleanup previous run then create fresh
    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(&tenant_id, &format!("RLS Test Tenant {tag}"))
        .await
        .expect("create tenant");

    let pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();

    // Run migrations so tables + RLS policies exist
    manager
        .run_tenant_migrations(&pool)
        .await
        .expect("migrate tenant");

    (manager, tenant_id, pool)
}

/// Teardown: drop the tenant database.
async fn teardown(manager: &PgManager, tenant_id: &TenantId) {
    manager.delete_tenant(tenant_id).await.ok();
}

/// Seed a minimal vendor row (bypasses RLS since we run as superuser).
async fn seed_vendor(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, status, routing_rules)
         VALUES ($1, $2, $3, 'active', '{}'::jsonb)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(format!("RLS Vendor {}", id))
    .execute(pool)
    .await
    .expect("seed vendor");
    id
}

/// Seed a minimal user row.
async fn seed_user(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, 'hash', 'RLS Test User')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(format!("rls-{}@test.com", id))
    .execute(pool)
    .await
    .expect("seed user");
    id
}

/// Seed a minimal invoice row.
async fn seed_invoice(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    user_id: Uuid,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by)
         VALUES ($1, $2, $3, 'Test Vendor', $4, 5000, $5, $6)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(format!("RLS-INV-{}", id))
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed invoice");
    id
}

// ===========================================================================
// Test 1: Correct session variable sees rows
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_correct_tenant_sees_own_rows() {
    let (manager, tenant_id, pool) = setup_rls_tenant("see-own").await;
    let tenant_uuid = *tenant_id.as_uuid();

    // Set session variable to match our tenant
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        tenant_uuid
    ))
    .execute(&pool)
    .await
    .expect("set session var");

    // Seed data (superuser bypasses RLS for INSERT)
    let vendor_id = seed_vendor(&pool, tenant_uuid).await;
    let user_id = seed_user(&pool, tenant_uuid).await;
    let invoice_id = seed_invoice(&pool, tenant_uuid, vendor_id, user_id).await;

    // Verify vendor is visible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM vendors")
        .fetch_one(&pool)
        .await
        .expect("count vendors");
    assert!(
        count.0 >= 1,
        "Should see at least 1 vendor with correct session var"
    );

    // Verify user is visible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("count users");
    assert!(
        count.0 >= 1,
        "Should see at least 1 user with correct session var"
    );

    // Verify invoice is visible
    let found: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_optional(&pool)
        .await
        .expect("find invoice");
    assert!(
        found.is_some(),
        "Invoice should be visible with correct session var"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 2: No session variable = zero rows visible (RLS blocks all reads)
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_unset_session_variable_blocks_reads() {
    let (manager, tenant_id, pool) = setup_rls_tenant("unset-var").await;
    let tenant_uuid = *tenant_id.as_uuid();

    // Seed data (session var doesn't matter for superuser seed)
    let vendor_id = seed_vendor(&pool, tenant_uuid).await;
    let user_id = seed_user(&pool, tenant_uuid).await;
    seed_invoice(&pool, tenant_uuid, vendor_id, user_id).await;

    // Reset the session variable so it's empty/null
    sqlx::query("SET LOCAL app.current_tenant_id = ''")
        .execute(&pool)
        .await
        .expect("reset session var");

    // Verify invoices invisible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM invoices")
        .fetch_one(&pool)
        .await
        .expect("count invoices");
    assert_eq!(
        count.0, 0,
        "With empty session variable, invoices should be invisible via RLS"
    );

    // Verify vendors invisible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM vendors")
        .fetch_one(&pool)
        .await
        .expect("count vendors");
    assert_eq!(
        count.0, 0,
        "With empty session variable, vendors should be invisible via RLS"
    );

    // Verify users invisible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("count users");
    assert_eq!(
        count.0, 0,
        "With empty session variable, users should be invisible via RLS"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 3: Wrong session variable = zero rows visible
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_wrong_session_variable_blocks_reads() {
    let (manager, tenant_id, pool) = setup_rls_tenant("wrong-var").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Seed data for our tenant
    let vendor_id = seed_vendor(&pool, tenant_uuid).await;
    let user_id = seed_user(&pool, tenant_uuid).await;
    let invoice_id = seed_invoice(&pool, tenant_uuid, vendor_id, user_id).await;

    // Set session variable to WRONG tenant
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        wrong_tenant
    ))
    .execute(&pool)
    .await
    .expect("set wrong session var");

    // Verify invoices invisible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM invoices")
        .fetch_one(&pool)
        .await
        .expect("count invoices");
    assert_eq!(
        count.0, 0,
        "With wrong session variable, invoices should be invisible via RLS"
    );

    // Verify the specific invoice is not findable
    let found: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_optional(&pool)
        .await
        .expect("find invoice");
    assert!(
        found.is_none(),
        "Specific invoice should not be visible with wrong session var"
    );

    // Verify vendors invisible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM vendors")
        .fetch_one(&pool)
        .await
        .expect("count vendors");
    assert_eq!(
        count.0, 0,
        "Vendors should be invisible with wrong session var"
    );

    // Verify users invisible
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("count users");
    assert_eq!(
        count.0, 0,
        "Users should be invisible with wrong session var"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 4: INSERT with wrong tenant_id is blocked by WITH CHECK
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_insert_wrong_tenant_blocked() {
    let (manager, tenant_id, pool) = setup_rls_tenant("insert-block").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Set session to correct tenant
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        tenant_uuid
    ))
    .execute(&pool)
    .await
    .expect("set session var");

    // Seed a vendor and user under correct tenant for FK
    let vendor_id = seed_vendor(&pool, tenant_uuid).await;
    let user_id = seed_user(&pool, tenant_uuid).await;

    // Attempt to INSERT a vendor with wrong tenant_id - should fail
    let result = sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, status, routing_rules)
         VALUES ($1, $2, 'Evil Vendor', 'active', '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(wrong_tenant)
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "INSERT with wrong tenant_id should be blocked by RLS WITH CHECK"
    );
    let err_msg = format!("{:?}", result.unwrap_err()).to_lowercase();
    assert!(
        err_msg.contains("policy") || err_msg.contains("violation") || err_msg.contains("check"),
        "Error should reference RLS policy violation, got: {}",
        err_msg
    );

    // Attempt to INSERT an invoice with wrong tenant_id - should also fail
    let result = sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by)
         VALUES ($1, $2, $3, 'Evil', 'RLS-EVIL-001', 9999, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(wrong_tenant)
    .bind(vendor_id)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "INSERT invoice with wrong tenant_id should be blocked by RLS WITH CHECK"
    );

    // Attempt to INSERT a user with wrong tenant_id - should also fail
    let result = sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'evil@test.com', 'hash', 'Evil User')",
    )
    .bind(Uuid::new_v4())
    .bind(wrong_tenant)
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "INSERT user with wrong tenant_id should be blocked by RLS WITH CHECK"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 5: UPDATE on row belonging to another tenant is blocked by RLS
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_update_wrong_tenant_blocked() {
    let (manager, tenant_id, pool) = setup_rls_tenant("update-block").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Seed a vendor under correct tenant (superuser bypasses RLS)
    let vendor_id = seed_vendor(&pool, tenant_uuid).await;

    // Set session to WRONG tenant
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        wrong_tenant
    ))
    .execute(&pool)
    .await
    .expect("set wrong session var");

    // Attempt UPDATE without WHERE tenant_id (RLS should make the row invisible)
    let result = sqlx::query("UPDATE vendors SET name = 'Hacked' WHERE id = $1")
        .bind(vendor_id)
        .execute(&pool)
        .await
        .expect("query should succeed (but affect 0 rows)");

    assert_eq!(
        result.rows_affected(),
        0,
        "UPDATE on invisible row should affect 0 rows due to RLS"
    );

    // Set session back to correct tenant and verify vendor is untouched
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        tenant_uuid
    ))
    .execute(&pool)
    .await
    .expect("set correct session var");

    let name: (String,) = sqlx::query_as("SELECT name FROM vendors WHERE id = $1")
        .bind(vendor_id)
        .fetch_one(&pool)
        .await
        .expect("fetch vendor");

    assert!(
        !name.0.contains("Hacked"),
        "Vendor name should not be modified after RLS-blocked UPDATE"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 6: DELETE on row belonging to another tenant is blocked by RLS
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_delete_wrong_tenant_blocked() {
    let (manager, tenant_id, pool) = setup_rls_tenant("delete-block").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Seed a user under correct tenant
    let user_id = seed_user(&pool, tenant_uuid).await;

    // Set session to WRONG tenant
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        wrong_tenant
    ))
    .execute(&pool)
    .await
    .expect("set wrong session var");

    // Attempt DELETE (RLS should make the row invisible)
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("query should succeed (but affect 0 rows)");

    assert_eq!(
        result.rows_affected(),
        0,
        "DELETE on invisible row should affect 0 rows due to RLS"
    );

    // Verify user still exists with correct session var
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        tenant_uuid
    ))
    .execute(&pool)
    .await
    .expect("set correct session var");

    let found: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .expect("find user");

    assert!(
        found.is_some(),
        "User should still exist after RLS-blocked DELETE"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 7: RLS policies are actually installed (meta-test)
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn rls_policies_exist_on_core_tables() {
    let (manager, tenant_id, pool) = setup_rls_tenant("meta-check").await;

    // Check RLS is enabled on invoices
    let rls_enabled: (String,) =
        sqlx::query_as("SELECT relrowsecurity::text FROM pg_class WHERE relname = 'invoices'")
            .fetch_one(&pool)
            .await
            .expect("check RLS on invoices");
    assert_eq!(rls_enabled.0, "true", "RLS should be enabled on invoices");

    // Check RLS is enabled on users
    let rls_enabled: (String,) =
        sqlx::query_as("SELECT relrowsecurity::text FROM pg_class WHERE relname = 'users'")
            .fetch_one(&pool)
            .await
            .expect("check RLS on users");
    assert_eq!(rls_enabled.0, "true", "RLS should be enabled on users");

    // Check RLS is enabled on vendors
    let rls_enabled: (String,) =
        sqlx::query_as("SELECT relrowsecurity::text FROM pg_class WHERE relname = 'vendors'")
            .fetch_one(&pool)
            .await
            .expect("check RLS on vendors");
    assert_eq!(rls_enabled.0, "true", "RLS should be enabled on vendors");

    // Verify specific policies exist
    let policies: Vec<(String,)> = sqlx::query_as(
        "SELECT policyname FROM pg_policies WHERE tablename IN ('invoices', 'users', 'vendors') AND policyname LIKE 'rls_tenant_%'"
    )
    .fetch_all(&pool)
    .await
    .expect("list RLS policies");

    assert_eq!(
        policies.len(),
        3,
        "Should have 3 rls_tenant_* policies (invoices, users, vendors), found: {:?}",
        policies
    );

    teardown(&manager, &tenant_id).await;
}
