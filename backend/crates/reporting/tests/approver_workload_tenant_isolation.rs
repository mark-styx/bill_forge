//! Integration test: approver-workload LEFT JOIN users must be tenant-qualified.
//!
//! Simulates import drift where an approval_request belonging to tenant A has
//! its `approver_id` pointing at a user that belongs to tenant B.  The
//! reporting queries must NOT surface tenant B's user email as
//! `approver_name` in tenant A's results.
//!
//! Gated behind the `integration` feature so `cargo test` skips by default:
//!   cargo test -p billforge-reporting --features integration

use billforge_core::TenantId;
use billforge_reporting::ReportingService;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn test_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".into());
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect to test database")
}

/// Create the minimal tables needed for the test.
/// Uses `CREATE IF NOT EXISTS` so it is idempotent across runs.
async fn ensure_schema(pool: &PgPool) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tenants (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT NOT NULL,
            slug TEXT UNIQUE NOT NULL,
            settings JSONB NOT NULL DEFAULT '{}',
            enabled_modules JSONB NOT NULL DEFAULT '[]',
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create tenants table");

    // Users table (minimal, needed for the LEFT JOIN)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            email TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            name TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create users table");

    // Approval requests table (minimal columns the queries touch)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS approval_requests (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            invoice_id UUID NOT NULL DEFAULT gen_random_uuid(),
            approver_id UUID NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            decided_at TIMESTAMPTZ
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create approval_requests table");
}

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid, slug: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(format!("Tenant {slug}"))
    .bind(slug)
    .execute(pool)
    .await
    .expect("insert tenant");
}

/// Insert a user row. Returns its UUID.
async fn insert_user(pool: &PgPool, tenant_id: Uuid, email: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, 'hash', 'Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(email)
    .execute(pool)
    .await
    .expect("insert user");
    id
}

/// Insert an approval_request row. Returns its UUID.
async fn insert_approval_request(
    pool: &PgPool,
    tenant_id: Uuid,
    approver_id: Uuid,
    status: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO approval_requests (id, tenant_id, approver_id, status)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(approver_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert approval_request");
    id
}

/// Clean up test data for deterministic re-runs.
async fn cleanup(pool: &PgPool, tenant_a: Uuid, tenant_b: Uuid) {
    sqlx::query("DELETE FROM approval_requests WHERE tenant_id = $1 OR tenant_id = $2")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM users WHERE tenant_id = $1 OR tenant_id = $2")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM tenants WHERE id = $1 OR id = $2")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that `get_approval_analytics` does NOT return tenant B's user email
/// when tenant A has an approval_request whose `approver_id` points at a user
/// belonging to tenant B (drift).
#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn approver_workload_excludes_cross_tenant_user_email() {
    let pool = test_pool().await;
    ensure_schema(&pool).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    cleanup(&pool, tenant_a, tenant_b).await;
    insert_tenant(&pool, tenant_a, &format!("tenant-a-{tenant_a}")).await;
    insert_tenant(&pool, tenant_b, &format!("tenant-b-{tenant_b}")).await;

    // Create a user under tenant B with a distinctive email (the "leak" target)
    let user_b_id = insert_user(&pool, tenant_b, "leak@tenant-b.test").await;

    // Create a user under tenant A (legitimate approver)
    let user_a_id = insert_user(&pool, tenant_a, "legit@tenant-a.test").await;

    // Insert an approval_request under tenant A whose approver_id points at
    // the cross-tenant user B (simulates drift / stale FK).
    insert_approval_request(&pool, tenant_a, user_b_id, "approved").await;

    // Insert a legitimate approval_request under tenant A pointing at user A.
    insert_approval_request(&pool, tenant_a, user_a_id, "approved").await;

    // Query via ReportingService for tenant A
    let tenant_id_a = TenantId::from_uuid(tenant_a);
    let service = ReportingService::new();
    let result = service
        .get_approval_analytics(&tenant_id_a, &Arc::new(pool.clone()), None, None)
        .await
        .expect("get_approval_analytics should succeed");

    // Assert: no returned approver_name equals the cross-tenant email
    for wl in &result.approver_workloads {
        assert_ne!(
            wl.approver_name, "leak@tenant-b.test",
            "approver_workload leaked cross-tenant user email"
        );
    }

    // Assert: the drift row falls back to "Unknown"
    let drift_row = result
        .approver_workloads
        .iter()
        .find(|w| w.approver_id == user_b_id.to_string());
    assert!(
        drift_row.is_some(),
        "drift approval_request should still appear (LEFT JOIN keeps the row)"
    );
    assert_eq!(
        drift_row.unwrap().approver_name,
        "Unknown",
        "drift row must fall back to 'Unknown', not cross-tenant email"
    );

    // Assert: the legitimate row resolves correctly
    let legit_row = result
        .approver_workloads
        .iter()
        .find(|w| w.approver_id == user_a_id.to_string())
        .expect("legitimate approver row should exist");
    assert_eq!(legit_row.approver_name, "legit@tenant-a.test");

    cleanup(&pool, tenant_a, tenant_b).await;
}

/// Verify that without the tenant predicate the cross-tenant email *would*
/// leak (proving the fix actually changes behaviour).
#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn without_tenant_predicate_cross_tenant_email_leaks() {
    let pool = test_pool().await;
    ensure_schema(&pool).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    cleanup(&pool, tenant_a, tenant_b).await;
    insert_tenant(&pool, tenant_a, &format!("tenant-a-{tenant_a}")).await;
    insert_tenant(&pool, tenant_b, &format!("tenant-b-{tenant_b}")).await;

    let user_b_id = insert_user(&pool, tenant_b, "leak@tenant-b.test").await;

    // Approval_request in tenant A referencing cross-tenant user B
    insert_approval_request(&pool, tenant_a, user_b_id, "approved").await;

    // OLD (unfixed) join: only ar.approver_id = u.id, no tenant predicate
    let rows = sqlx::query(
        r#"
        SELECT COALESCE(u.email, 'Unknown') as approver_name
        FROM approval_requests ar
        LEFT JOIN users u ON ar.approver_id = u.id
        WHERE ar.tenant_id = $1
        GROUP BY ar.approver_id, u.email
        "#,
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("old join query");

    // With the old join, user B's cross-tenant email leaks through
    assert_eq!(rows.len(), 1, "should have one row");
    let name: String = rows[0].get("approver_name");
    assert_eq!(
        name, "leak@tenant-b.test",
        "unfixed join MUST leak cross-tenant email (proves the test setup is correct)"
    );

    // NEW (fixed) join: includes u.tenant_id = ar.tenant_id
    let rows_fixed = sqlx::query(
        r#"
        SELECT COALESCE(u.email, 'Unknown') as approver_name
        FROM approval_requests ar
        LEFT JOIN users u ON ar.approver_id = u.id AND u.tenant_id = ar.tenant_id
        WHERE ar.tenant_id = $1
        GROUP BY ar.approver_id, u.email
        "#,
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("fixed join query");

    assert_eq!(rows_fixed.len(), 1, "should have one row");
    let name_fixed: String = rows_fixed[0].get("approver_name");
    assert_eq!(
        name_fixed, "Unknown",
        "fixed join must fall back to 'Unknown', not cross-tenant email"
    );

    cleanup(&pool, tenant_a, tenant_b).await;
}
