//! RLS coverage tests for tenant_db-created tables
//!
//! Verifies that every tenant-scoped table created by tenant_db.rs has:
//!  1. relrowsecurity = true  (RLS enabled)
//!  2. relforcerowsecurity = true  (FORCE RLS)
//!  3. A rls_tenant_* policy that blocks cross-tenant reads and writes
//!
//! Run:
//!   cargo test -p billforge-db --test rls_tenant_db_tables --features integration
//!   cargo test -p billforge-db --test rls_tenant_db_tables -- --ignored

use billforge_core::TenantId;
use billforge_db::PgManager;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers (mirroring rls_isolation_test.rs)
// ---------------------------------------------------------------------------

const RLS_TEST_ROLE: &str = "billforge_rls_test";
const RLS_TEST_PASSWORD: &str = "billforge_rls_test";

async fn setup(tag: &str) -> (PgManager, TenantId, sqlx::PgPool, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template.clone())
        .await
        .expect("PgManager");

    let tenant_id: TenantId =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("rls-tdb-{tag}").as_bytes())
            .to_string()
            .parse()
            .unwrap();

    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(&tenant_id, &format!("RLS TDB Tenant {tag}"))
        .await
        .expect("create tenant");

    let admin_pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();
    manager
        .run_tenant_migrations(&admin_pool)
        .await
        .expect("migrate tenant");

    grant_rls_test_role(&admin_pool).await;

    let db_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));
    let admin_url = tenant_template.replace("{database}", &db_name);
    let app_url = make_app_url(&admin_url);

    let app_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&app_url)
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
    .expect("create role");

    sqlx::raw_sql(&format!(
        "GRANT USAGE ON SCHEMA public TO {RLS_TEST_ROLE};
         GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {RLS_TEST_ROLE};
         GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {RLS_TEST_ROLE};"
    ))
    .execute(pool)
    .await
    .expect("grant role");
}

fn make_app_url(admin_url: &str) -> String {
    let Some(rest) = admin_url.strip_prefix("postgres://") else {
        return admin_url.to_string();
    };
    let Some((_, host_and_path)) = rest.split_once('@') else {
        return admin_url.to_string();
    };
    format!("postgres://{RLS_TEST_ROLE}:{RLS_TEST_PASSWORD}@{host_and_path}")
}

async fn set_tenant(pool: &sqlx::PgPool, tenant_id: Option<Uuid>) {
    let sql = match tenant_id {
        Some(id) => format!("SET app.current_tenant_id = '{}'", id),
        None => "RESET app.current_tenant_id".to_string(),
    };
    sqlx::query(&sql).execute(pool).await.expect("set tenant");
}

async fn teardown(manager: &PgManager, tenant_id: &TenantId) {
    manager.delete_tenant(tenant_id).await.ok();
}

/// Seed a vendor + user so FK-dependent tables can reference them.
async fn seed_vendor_and_user(pool: &sqlx::PgPool, tenant_uuid: Uuid) -> (Uuid, Uuid) {
    let vendor_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, status, routing_rules)
         VALUES ($1, $2, $3, 'active', '{}'::jsonb)",
    )
    .bind(vendor_id)
    .bind(tenant_uuid)
    .bind(format!("Vendor {}", vendor_id))
    .execute(pool)
    .await
    .expect("seed vendor");

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, 'hash', 'Test User')",
    )
    .bind(user_id)
    .bind(tenant_uuid)
    .bind(format!("rls-tdb-{}@test.com", user_id))
    .execute(pool)
    .await
    .expect("seed user");

    (vendor_id, user_id)
}

/// All tenant_db-created tables that should have RLS.
const RLS_TABLES: &[&str] = &[
    "documents",
    "audit_log",
    "invoice_status_config",
    "approval_limits",
    "edi_connections",
    "edi_documents",
    "edi_trading_partners",
    "edi_webhook_nonces",
    "invoice_line_items",
    "vendor_contacts",
    "vendor_bank_accounts",
    "vendor_statement_settings",
    "vendor_statements",
];

// ===========================================================================
// Test 1: relrowsecurity + relforcerowsecurity on every table
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_enabled_and_forced_on_all_tenant_db_tables() {
    let (manager, tenant_id, admin_pool, _app_pool) = setup("meta").await;

    for &table in RLS_TABLES {
        let row: Option<(bool, bool)> = sqlx::query_as(
            "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE relname = $1",
        )
        .bind(table)
        .fetch_optional(&admin_pool)
        .await
        .unwrap_or_else(|_| panic!("query pg_class for {}", table));

        let (rls, forced) = row.unwrap_or_else(|| panic!("table {} not found in pg_class", table));
        assert!(rls, "RLS should be enabled on {}", table);
        assert!(forced, "FORCE RLS should be enabled on {}", table);
    }

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 2: documents — cross-tenant SELECT blocked
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_documents_cross_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup("docs").await;
    let tenant_uuid = *tenant_id.as_uuid();

    let (vendor_id, user_id) = seed_vendor_and_user(&admin_pool, tenant_uuid).await;

    // Seed an invoice (needed for documents FK) and a document
    let invoice_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by)
         VALUES ($1, $2, $3, 'V', 'INV-1', 100, $4, $5)",
    )
    .bind(invoice_id)
    .bind(tenant_uuid)
    .bind(vendor_id)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(&admin_pool)
    .await
    .expect("seed invoice");

    sqlx::query(
        "INSERT INTO documents (id, tenant_id, filename, mime_type, size_bytes,
                                storage_key, doc_type, uploaded_by)
         VALUES ($1, $2, 'test.pdf', 'application/pdf', 42, 'key', 'invoice_original', $3)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .bind(user_id)
    .execute(&admin_pool)
    .await
    .expect("seed document");

    // Correct tenant sees the row
    set_tenant(&pool, Some(tenant_uuid)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(count.0 >= 1, "Correct tenant should see documents");

    // Wrong tenant sees zero rows
    let wrong_tenant = Uuid::new_v4();
    set_tenant(&pool, Some(wrong_tenant)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count.0, 0, "Wrong tenant should see 0 documents");

    // INSERT with wrong tenant_id blocked by WITH CHECK
    set_tenant(&pool, Some(wrong_tenant)).await;
    let result = sqlx::query(
        "INSERT INTO documents (id, tenant_id, filename, mime_type, size_bytes,
                                storage_key, doc_type, uploaded_by)
         VALUES ($1, $2, 'evil.pdf', 'application/pdf', 1, 'k', 'invoice_original', $3)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid) // wrong tenant_id for current setting
    .bind(user_id)
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "INSERT with mismatched tenant_id should be blocked"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 3: audit_log — cross-tenant SELECT blocked
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_audit_log_cross_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup("audit").await;
    let tenant_uuid = *tenant_id.as_uuid();

    let (_, user_id) = seed_vendor_and_user(&admin_pool, tenant_uuid).await;

    sqlx::query(
        "INSERT INTO audit_log (id, tenant_id, user_id, action, resource_type)
         VALUES ($1, $2, $3, 'test_action', 'test_resource')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .bind(user_id)
    .execute(&admin_pool)
    .await
    .expect("seed audit_log");

    // Correct tenant sees row
    set_tenant(&pool, Some(tenant_uuid)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(count.0 >= 1, "Correct tenant should see audit_log rows");

    // Wrong tenant sees zero
    let wrong_tenant = Uuid::new_v4();
    set_tenant(&pool, Some(wrong_tenant)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count.0, 0, "Wrong tenant should see 0 audit_log rows");

    // INSERT with wrong tenant_id blocked
    let result = sqlx::query(
        "INSERT INTO audit_log (id, tenant_id, user_id, action, resource_type)
         VALUES ($1, $2, $3, 'evil', 'evil')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .bind(user_id)
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "INSERT with mismatched tenant_id should be blocked"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 4: edi_connections — cross-tenant SELECT blocked
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_edi_connections_cross_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup("edi").await;
    let tenant_uuid = *tenant_id.as_uuid();

    sqlx::query(
        "INSERT INTO edi_connections (id, tenant_id, provider, api_key_encrypted, webhook_secret)
         VALUES ($1, $2, 'test_provider', 'enc_key', 'secret')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .execute(&admin_pool)
    .await
    .expect("seed edi_connections");

    // Correct tenant sees row
    set_tenant(&pool, Some(tenant_uuid)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM edi_connections")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(count.0 >= 1, "Correct tenant should see edi_connections");

    // Wrong tenant sees zero
    let wrong_tenant = Uuid::new_v4();
    set_tenant(&pool, Some(wrong_tenant)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM edi_connections")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count.0, 0, "Wrong tenant should see 0 edi_connections");

    // INSERT with wrong tenant_id blocked
    let result = sqlx::query(
        "INSERT INTO edi_connections (id, tenant_id, provider, api_key_encrypted, webhook_secret)
         VALUES ($1, $2, 'evil', 'k', 's')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "INSERT with mismatched tenant_id should be blocked"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 5: vendor_contacts — EXISTS-based policy via vendor_id
// ===========================================================================

#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn rls_vendor_contacts_cross_tenant_blocked() {
    let (manager, tenant_id, admin_pool, pool) = setup("vc").await;
    let tenant_uuid = *tenant_id.as_uuid();

    let (vendor_id, _) = seed_vendor_and_user(&admin_pool, tenant_uuid).await;

    sqlx::query(
        "INSERT INTO vendor_contacts (id, vendor_id, name, is_primary)
         VALUES ($1, $2, 'Test Contact', true)",
    )
    .bind(Uuid::new_v4())
    .bind(vendor_id)
    .execute(&admin_pool)
    .await
    .expect("seed vendor_contacts");

    // Correct tenant sees row
    set_tenant(&pool, Some(tenant_uuid)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM vendor_contacts")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(count.0 >= 1, "Correct tenant should see vendor_contacts");

    // Wrong tenant sees zero
    let wrong_tenant = Uuid::new_v4();
    set_tenant(&pool, Some(wrong_tenant)).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM vendor_contacts")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count.0, 0, "Wrong tenant should see 0 vendor_contacts");

    teardown(&manager, &tenant_id).await;
}
