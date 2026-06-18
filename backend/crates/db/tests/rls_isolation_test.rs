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
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const RLS_TEST_ROLE: &str = "billforge_rls_test";
const RLS_TEST_PASSWORD: &str = "billforge_rls_test";

/// Set up a single tenant database with all migrations (including 080_enable_rls).
/// Returns the manager, tenant ID, admin pool, and app-role pool.
async fn setup_rls_tenant(tag: &str) -> (PgManager, TenantId, sqlx::PgPool, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template.clone())
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

    let admin_pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();

    // Run migrations so tables + RLS policies exist
    manager
        .run_tenant_migrations(&admin_pool)
        .await
        .expect("migrate tenant");
    sqlx::query(
        "INSERT INTO tenants (id, name, slug)
         VALUES ($1, $2, $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(*tenant_id.as_uuid())
    .bind(format!("RLS Test Tenant {tag}"))
    .bind(format!("rls-test-tenant-{tag}"))
    .execute(&admin_pool)
    .await
    .expect("seed tenant");

    grant_rls_test_role(&admin_pool).await;

    let app_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&rls_app_url(&tenant_template, &tenant_id))
        .await
        .expect("connect as RLS test role");

    (manager, tenant_id, admin_pool, app_pool)
}

async fn grant_rls_test_role(pool: &sqlx::PgPool) {
    sqlx::query(&format!(
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{RLS_TEST_ROLE}') THEN
                CREATE ROLE {RLS_TEST_ROLE} LOGIN PASSWORD '{RLS_TEST_PASSWORD}';
            END IF;
        END
        $$;
        "#
    ))
    .execute(pool)
    .await
    .expect("create RLS test role");

    sqlx::raw_sql(&format!(
        "GRANT USAGE ON SCHEMA public TO {RLS_TEST_ROLE};
         GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {RLS_TEST_ROLE};
         GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {RLS_TEST_ROLE};"
    ))
    .execute(pool)
    .await
    .expect("grant RLS test role");
}

fn rls_app_url(template: &str, tenant_id: &TenantId) -> String {
    let db_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));
    let admin_url = template.replace("{database}", &db_name);

    let Some(rest) = admin_url.strip_prefix("postgres://") else {
        return admin_url;
    };
    let Some((_, host_and_path)) = rest.split_once('@') else {
        return admin_url;
    };

    format!("postgres://{RLS_TEST_ROLE}:{RLS_TEST_PASSWORD}@{host_and_path}")
}

async fn set_rls_tenant(pool: &sqlx::PgPool, tenant_id: Option<Uuid>) {
    let sql = match tenant_id {
        Some(tenant_id) => format!("SET app.current_tenant_id = '{}'", tenant_id),
        None => "RESET app.current_tenant_id".to_string(),
    };

    sqlx::query(&sql)
        .execute(pool)
        .await
        .expect("set RLS tenant");
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
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_correct_tenant_sees_own_rows() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("see-own").await;
    let tenant_uuid = *tenant_id.as_uuid();

    // Set session variable to match our tenant
    set_rls_tenant(&pool, Some(tenant_uuid)).await;

    // Seed data (superuser bypasses RLS for INSERT)
    let vendor_id = seed_vendor(&admin_pool, tenant_uuid).await;
    let user_id = seed_user(&admin_pool, tenant_uuid).await;
    let invoice_id = seed_invoice(&admin_pool, tenant_uuid, vendor_id, user_id).await;

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
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_unset_session_variable_blocks_reads() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("unset-var").await;
    let tenant_uuid = *tenant_id.as_uuid();

    // Seed data (session var doesn't matter for superuser seed)
    let vendor_id = seed_vendor(&admin_pool, tenant_uuid).await;
    let user_id = seed_user(&admin_pool, tenant_uuid).await;
    seed_invoice(&admin_pool, tenant_uuid, vendor_id, user_id).await;

    // Reset the session variable so it's empty/null
    set_rls_tenant(&pool, None).await;

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
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_wrong_session_variable_blocks_reads() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("wrong-var").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Seed data for our tenant
    let vendor_id = seed_vendor(&admin_pool, tenant_uuid).await;
    let user_id = seed_user(&admin_pool, tenant_uuid).await;
    let invoice_id = seed_invoice(&admin_pool, tenant_uuid, vendor_id, user_id).await;

    // Set session variable to WRONG tenant
    set_rls_tenant(&pool, Some(wrong_tenant)).await;

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
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_insert_wrong_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("insert-block").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Set session to correct tenant
    set_rls_tenant(&pool, Some(tenant_uuid)).await;

    // Seed a vendor and user under correct tenant for FK
    let vendor_id = seed_vendor(&admin_pool, tenant_uuid).await;
    let user_id = seed_user(&admin_pool, tenant_uuid).await;

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
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_update_wrong_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("update-block").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Seed a vendor under correct tenant (superuser bypasses RLS)
    let vendor_id = seed_vendor(&admin_pool, tenant_uuid).await;

    // Set session to WRONG tenant
    set_rls_tenant(&pool, Some(wrong_tenant)).await;

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
    set_rls_tenant(&pool, Some(tenant_uuid)).await;

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
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_delete_wrong_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("delete-block").await;
    let tenant_uuid = *tenant_id.as_uuid();
    let wrong_tenant = Uuid::new_v4();

    // Seed a user under correct tenant
    let user_id = seed_user(&admin_pool, tenant_uuid).await;

    // Set session to WRONG tenant
    set_rls_tenant(&pool, Some(wrong_tenant)).await;

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
    set_rls_tenant(&pool, Some(tenant_uuid)).await;

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

// ===========================================================================
// Test 8: PgManager tenant pool sets app.current_tenant_id automatically
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn tenant_pool_sets_app_current_tenant_id() {
    // Case 1: Verify the after_connect hook sets app.current_tenant_id
    let (manager, tenant_id, _admin_pool, _app_pool) = setup_rls_tenant("guc-check").await;

    // Acquire a connection from the tenant pool (NOT using set_rls_tenant)
    let pool = manager.tenant(&tenant_id).await.expect("tenant pool");
    let setting: (String,) =
        sqlx::query_as("SELECT current_setting('app.current_tenant_id', true)")
            .fetch_one(&*pool)
            .await
            .expect("get current_setting");

    assert_eq!(
        setting.0,
        tenant_id.as_uuid().to_string(),
        "Pool connection should have app.current_tenant_id set to tenant UUID"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 9: after_connect hook drives RLS isolation across two tenants
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_after_connect_hook_isolates_tenants() {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template.clone())
        .await
        .expect("PgManager");

    // Create two tenants in the same database to test cross-tenant isolation
    let tenant_a: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"rls-hook-tenant-a")
        .to_string()
        .parse()
        .unwrap();
    let tenant_b: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"rls-hook-tenant-b")
        .to_string()
        .parse()
        .unwrap();

    // Cleanup previous runs
    manager.delete_tenant(&tenant_a).await.ok();
    manager.delete_tenant(&tenant_b).await.ok();

    manager
        .create_tenant(&tenant_a, "RLS Hook Tenant A")
        .await
        .expect("create tenant A");
    manager
        .create_tenant(&tenant_b, "RLS Hook Tenant B")
        .await
        .expect("create tenant B");

    let admin_a = (*manager.tenant(&tenant_a).await.expect("pool a")).clone();
    let admin_b = (*manager.tenant(&tenant_b).await.expect("pool b")).clone();

    // Run migrations so RLS policies exist in both databases
    manager
        .run_tenant_migrations(&admin_a)
        .await
        .expect("migrate A");
    manager
        .run_tenant_migrations(&admin_b)
        .await
        .expect("migrate B");

    // Seed tenant rows inside each tenant database (superuser bypasses RLS)
    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(*tenant_a.as_uuid())
    .bind("RLS Hook Tenant A")
    .bind("rls-hook-tenant-a")
    .execute(&admin_a)
    .await
    .expect("seed tenant A row");

    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(*tenant_b.as_uuid())
    .bind("RLS Hook Tenant B")
    .bind("rls-hook-tenant-b")
    .execute(&admin_b)
    .await
    .expect("seed tenant B row");

    grant_rls_test_role(&admin_a).await;
    grant_rls_test_role(&admin_b).await;

    // Seed a vendor in each tenant's database
    seed_vendor(&admin_a, *tenant_a.as_uuid()).await;
    seed_vendor(&admin_b, *tenant_b.as_uuid()).await;

    // Create RLS test role pools with after_connect hooks (mimicking PgManager::tenant)
    let tenant_a_uuid = *tenant_a.as_uuid();
    let pool_a = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _meta| {
            let uuid = tenant_a_uuid;
            Box::pin(async move {
                sqlx::query(&format!("SET app.current_tenant_id = '{}'", uuid))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&rls_app_url(&tenant_template, &tenant_a))
        .await
        .expect("RLS pool A");

    let tenant_b_uuid = *tenant_b.as_uuid();
    let pool_b = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _meta| {
            let uuid = tenant_b_uuid;
            Box::pin(async move {
                sqlx::query(&format!("SET app.current_tenant_id = '{}'", uuid))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&rls_app_url(&tenant_template, &tenant_b))
        .await
        .expect("RLS pool B");

    // Pool A should see only tenant A's vendors
    let count_a: (i64,) = sqlx::query_as("SELECT count(*) FROM vendors")
        .fetch_one(&pool_a)
        .await
        .expect("count vendors A");
    assert_eq!(
        count_a.0, 1,
        "Tenant A pool should see exactly 1 vendor (its own)"
    );

    // Pool B should see only tenant B's vendors
    let count_b: (i64,) = sqlx::query_as("SELECT count(*) FROM vendors")
        .fetch_one(&pool_b)
        .await
        .expect("count vendors B");
    assert_eq!(
        count_b.0, 1,
        "Tenant B pool should see exactly 1 vendor (its own)"
    );

    // Cleanup
    manager.delete_tenant(&tenant_a).await.ok();
    manager.delete_tenant(&tenant_b).await.ok();
}

// ===========================================================================
// Test 10: FORCE RLS blocks table owner without tenant setting
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn force_rls_blocks_owner_without_tenant_setting() {
    let (manager, tenant_id, admin_pool, pool) = setup_rls_tenant("force-rls").await;
    let tenant_uuid = *tenant_id.as_uuid();

    // Seed data as superuser (admin_pool bypasses RLS for INSERT)
    let vendor_id = seed_vendor(&admin_pool, tenant_uuid).await;
    let user_id = seed_user(&admin_pool, tenant_uuid).await;
    seed_invoice(&admin_pool, tenant_uuid, vendor_id, user_id).await;

    // Without setting app.current_tenant_id, SELECT should return 0 rows.
    // Under FORCE RLS + the NULLIF policy from migration 092, the predicate
    // fails closed: NULLIF('', '') yields NULL which never equals a UUID.
    set_rls_tenant(&pool, None).await;

    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM invoices")
        .fetch_one(&pool)
        .await
        .expect("count invoices");
    assert_eq!(
        count.0, 0,
        "FORCE RLS should block reads when tenant setting is absent"
    );

    // Setting the tenant should restore access
    set_rls_tenant(&pool, Some(tenant_uuid)).await;

    let vendor_count: (i64,) = sqlx::query_as("SELECT count(*) FROM vendors")
        .fetch_one(&pool)
        .await
        .expect("count vendors");
    assert!(
        vendor_count.0 >= 1,
        "Should see vendors with correct tenant setting"
    );

    // Verify pg_class.relforcerowsecurity = true for all RLS-protected tables
    let force_rls_tables = vec!["invoices", "users", "vendors", "ai_conversations"];
    for table in &force_rls_tables {
        let forced: (bool,) =
            sqlx::query_as("SELECT relforcerowsecurity FROM pg_class WHERE relname = $1")
                .bind(table)
                .fetch_one(&admin_pool)
                .await
                .unwrap_or_else(|_| panic!("table {} not found in pg_class", table));
        assert!(
            forced.0,
            "FORCE ROW LEVEL SECURITY should be enabled on {}",
            table
        );
    }

    teardown(&manager, &tenant_id).await;
}

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_policies_exist_on_core_tables() {
    let (manager, tenant_id, pool, _app_pool) = setup_rls_tenant("meta-check").await;

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
