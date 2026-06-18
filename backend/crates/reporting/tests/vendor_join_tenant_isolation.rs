//! Integration test: vendor joins must be tenant-qualified.
//!
//! Simulates import drift where an invoice belonging to tenant A has its
//! `vendor_id` pointing at a vendor that belongs to tenant B.  The reporting
//! queries must NOT surface tenant B's vendor name/metrics in tenant A's
//! results.
//!
//! Gated behind the `integration` feature so `cargo test` skips by default:
//!   cargo test -p billforge-reporting --features integration

use billforge_core::TenantId;
use billforge_reporting::ReportingService;
use chrono::NaiveDate;
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

    // Vendors table (same DDL as migration 003)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS vendors (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            name TEXT NOT NULL,
            tax_id TEXT,
            address JSONB,
            contact_email TEXT,
            contact_phone TEXT,
            payment_terms TEXT,
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, name)
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create vendors table");

    // Users table (minimal, needed for FK)
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

    // Invoices table (same DDL as migration 004)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS invoices (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            vendor_id UUID REFERENCES vendors(id),
            vendor_name TEXT NOT NULL,
            invoice_number TEXT NOT NULL,
            invoice_date DATE,
            due_date DATE,
            po_number TEXT,
            subtotal_cents BIGINT,
            tax_amount_cents BIGINT,
            total_amount_cents BIGINT NOT NULL,
            currency TEXT NOT NULL DEFAULT 'USD',
            line_items JSONB NOT NULL DEFAULT '[]',
            capture_status TEXT NOT NULL DEFAULT 'pending',
            processing_status TEXT NOT NULL DEFAULT 'draft',
            status TEXT NOT NULL DEFAULT 'received',
            current_queue_id UUID,
            assigned_to UUID REFERENCES users(id),
            document_id UUID NOT NULL,
            supporting_documents JSONB NOT NULL DEFAULT '[]',
            ocr_confidence REAL,
            department TEXT,
            gl_code TEXT,
            cost_center TEXT,
            notes TEXT,
            tags JSONB NOT NULL DEFAULT '[]',
            custom_fields JSONB NOT NULL DEFAULT '{}',
            created_by UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, invoice_number)
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create invoices table");
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

/// Insert a vendor row.  Returns its UUID.
async fn insert_vendor(pool: &PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert vendor");
    id
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

struct InvoiceFixture<'a> {
    tenant_id: Uuid,
    vendor_id: Uuid,
    vendor_name: &'a str,
    user_id: Uuid,
    invoice_number: &'a str,
    amount_cents: i64,
    invoice_date: NaiveDate,
}

/// Insert an invoice row with a specific vendor_id.
async fn insert_invoice(pool: &PgPool, invoice: InvoiceFixture<'_>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by, invoice_date)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(invoice.tenant_id)
    .bind(invoice.vendor_id)
    .bind(invoice.vendor_name)
    .bind(invoice.invoice_number)
    .bind(invoice.amount_cents)
    .bind(Uuid::new_v4()) // document_id
    .bind(invoice.user_id)
    .bind(invoice.invoice_date)
    .execute(pool)
    .await
    .expect("insert invoice");
    id
}

/// Clean up test data for deterministic re-runs.
async fn cleanup(pool: &PgPool, tenant_a: Uuid, tenant_b: Uuid) {
    sqlx::query("DELETE FROM invoices WHERE tenant_id = $1 OR tenant_id = $2")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 OR tenant_id = $2")
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

/// Verify that `get_vendor_spend` does NOT return vendor B's name when
/// tenant A has an invoice whose `vendor_id` points at vendor B (drift).
#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn vendor_spend_excludes_cross_tenant_vendor() {
    let pool = test_pool().await;
    ensure_schema(&pool).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    cleanup(&pool, tenant_a, tenant_b).await;
    insert_tenant(&pool, tenant_a, &format!("tenant-a-{tenant_a}")).await;
    insert_tenant(&pool, tenant_b, &format!("tenant-b-{tenant_b}")).await;

    // Create a user under tenant A (needed for created_by FK)
    let user_a = insert_user(&pool, tenant_a, "vs-user-a@test.com").await;

    // Create vendor B belonging to tenant B
    let vendor_b_id = insert_vendor(&pool, tenant_b, "Vendor B - CrossTenant").await;

    // Also create a legitimate vendor A belonging to tenant A
    let vendor_a_id = insert_vendor(&pool, tenant_a, "Vendor A - Legitimate").await;

    let today = NaiveDate::from_ymd_opt(2026, 5, 29).unwrap();

    // Insert an invoice under tenant A but pointing at vendor B (simulates drift)
    insert_invoice(
        &pool,
        InvoiceFixture {
            tenant_id: tenant_a,
            vendor_id: vendor_b_id,
            vendor_name: "Drift Fallback Name",
            user_id: user_a,
            invoice_number: "DRIFT-001",
            amount_cents: 50_000,
            invoice_date: today,
        },
    )
    .await;

    // Insert a legitimate invoice under tenant A pointing at vendor A
    insert_invoice(
        &pool,
        InvoiceFixture {
            tenant_id: tenant_a,
            vendor_id: vendor_a_id,
            vendor_name: "Vendor A - Legitimate",
            user_id: user_a,
            invoice_number: "LEGIT-001",
            amount_cents: 30_000,
            invoice_date: today,
        },
    )
    .await;

    // Query via ReportingService for tenant A
    let tenant_id_a = TenantId::from_uuid(tenant_a);
    let service = ReportingService::new();
    let result = service
        .get_vendor_spend(&tenant_id_a, &Arc::new(pool.clone()), None, None, 100)
        .await
        .expect("get_vendor_spend should succeed");

    // Assert: vendor B's real name must NOT appear in any result row
    for row in &result {
        assert_ne!(
            row.vendor_name, "Vendor B - CrossTenant",
            "vendor_spend leaked cross-tenant vendor name"
        );
    }

    // Assert: the drift invoice should show the fallback name (from i.vendor_name),
    // not vendor B's real name. With the fix, LEFT JOIN misses so v.name is NULL,
    // and COALESCE falls through to i.vendor_name.
    let drift_row = result
        .iter()
        .find(|r| r.vendor_id == vendor_b_id.to_string());
    assert!(
        drift_row.is_some(),
        "drift invoice should still appear (LEFT JOIN keeps the row)"
    );
    let drift_row = drift_row.unwrap();
    assert_eq!(
        drift_row.vendor_name, "Drift Fallback Name",
        "drift invoice must use fallback vendor_name, not the cross-tenant vendor's real name"
    );

    // Assert: the legitimate row resolves correctly
    let legit_row = result
        .iter()
        .find(|r| r.vendor_id == vendor_a_id.to_string())
        .expect("legitimate vendor row should exist");
    assert_eq!(legit_row.vendor_name, "Vendor A - Legitimate");

    cleanup(&pool, tenant_a, tenant_b).await;
}

/// Verify the raw SQL pattern directly: LEFT JOIN with tenant predicate
/// returns NULL for vendor fields when vendor_id points across tenants.
#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn vendor_performance_excludes_cross_tenant_vendor() {
    let pool = test_pool().await;
    ensure_schema(&pool).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    cleanup(&pool, tenant_a, tenant_b).await;
    insert_tenant(&pool, tenant_a, &format!("tenant-a-{tenant_a}")).await;
    insert_tenant(&pool, tenant_b, &format!("tenant-b-{tenant_b}")).await;

    let user_a = insert_user(&pool, tenant_a, "vp-user-a@test.com").await;

    // Vendor B belongs to tenant B
    let vendor_b_id = insert_vendor(&pool, tenant_b, "Vendor B - PerfCross").await;
    // Vendor A belongs to tenant A
    let vendor_a_id = insert_vendor(&pool, tenant_a, "Vendor A - PerfLegit").await;

    let today = NaiveDate::from_ymd_opt(2026, 5, 29).unwrap();

    // Drift invoice: tenant A, vendor_id -> vendor B
    insert_invoice(
        &pool,
        InvoiceFixture {
            tenant_id: tenant_a,
            vendor_id: vendor_b_id,
            vendor_name: "Perf Drift Fallback",
            user_id: user_a,
            invoice_number: "PERF-DRIFT-001",
            amount_cents: 80_000,
            invoice_date: today,
        },
    )
    .await;

    // Legitimate invoice: tenant A, vendor_id -> vendor A
    insert_invoice(
        &pool,
        InvoiceFixture {
            tenant_id: tenant_a,
            vendor_id: vendor_a_id,
            vendor_name: "Vendor A - PerfLegit",
            user_id: user_a,
            invoice_number: "PERF-LEGIT-001",
            amount_cents: 40_000,
            invoice_date: today,
        },
    )
    .await;

    // Run the vendor_performance SQL directly (same pattern as service.rs)
    let rows = sqlx::query(
        r#"
        WITH vendor_stats AS (
            SELECT
                i.vendor_id::text,
                COALESCE(v.name, i.vendor_name) as vendor_name,
                COUNT(*) as total_invoices,
                COALESCE(SUM(i.total_amount_cents), 0) / 100.0 as total_spend
            FROM invoices i
            LEFT JOIN vendors v ON i.vendor_id = v.id AND v.tenant_id = i.tenant_id
            WHERE i.vendor_id IS NOT NULL AND i.tenant_id = $1
            GROUP BY i.vendor_id, v.name, i.vendor_name
        )
        SELECT vendor_id, vendor_name, total_invoices, total_spend
        FROM vendor_stats
        ORDER BY total_spend DESC
        "#,
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("vendor performance query");

    // Assert: vendor B's real name must NOT appear
    for row in &rows {
        let name: String = row.get("vendor_name");
        assert_ne!(
            name, "Vendor B - PerfCross",
            "vendor_performance leaked cross-tenant vendor name"
        );
    }

    // Assert: drift row uses fallback name
    let drift_row = rows
        .iter()
        .find(|r| r.get::<String, _>("vendor_id") == vendor_b_id.to_string())
        .expect("drift invoice row should exist");
    assert_eq!(
        drift_row.get::<String, _>("vendor_name"),
        "Perf Drift Fallback",
        "drift invoice must use fallback vendor_name"
    );

    // Assert: legitimate row resolves correctly
    let legit_row = rows
        .iter()
        .find(|r| r.get::<String, _>("vendor_id") == vendor_a_id.to_string())
        .expect("legitimate vendor row should exist");
    assert_eq!(
        legit_row.get::<String, _>("vendor_name"),
        "Vendor A - PerfLegit"
    );

    cleanup(&pool, tenant_a, tenant_b).await;
}

/// Verify that without the tenant predicate the cross-tenant vendor name
/// *would* leak (proving the fix actually changes behaviour).
#[tokio::test]
#[ignore = "requires billforge_app role + RLS-aware fixtures; see #345 follow-up"]
async fn without_tenant_predicate_cross_tenant_name_leaks() {
    let pool = test_pool().await;
    ensure_schema(&pool).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    cleanup(&pool, tenant_a, tenant_b).await;
    insert_tenant(&pool, tenant_a, &format!("tenant-a-{tenant_a}")).await;
    insert_tenant(&pool, tenant_b, &format!("tenant-b-{tenant_b}")).await;

    let user_a = insert_user(&pool, tenant_a, "leak-user-a@test.com").await;
    let vendor_b_id = insert_vendor(&pool, tenant_b, "Vendor B - Should Leak").await;

    let today = NaiveDate::from_ymd_opt(2026, 5, 29).unwrap();

    insert_invoice(
        &pool,
        InvoiceFixture {
            tenant_id: tenant_a,
            vendor_id: vendor_b_id,
            vendor_name: "Original Fallback",
            user_id: user_a,
            invoice_number: "LEAK-001",
            amount_cents: 10_000,
            invoice_date: today,
        },
    )
    .await;

    // OLD (unfixed) join: only i.vendor_id = v.id, no tenant predicate
    let rows = sqlx::query(
        r#"
        SELECT COALESCE(v.name, i.vendor_name) as vendor_name
        FROM invoices i
        LEFT JOIN vendors v ON i.vendor_id = v.id
        WHERE i.vendor_id IS NOT NULL AND i.tenant_id = $1
        "#,
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("old join query");

    // With the old join, vendor B's real name leaks through
    assert_eq!(rows.len(), 1, "should have one row");
    let name: String = rows[0].get("vendor_name");
    assert_eq!(
        name, "Vendor B - Should Leak",
        "unfixed join MUST leak cross-tenant name (proves the test setup is correct)"
    );

    // NEW (fixed) join: includes v.tenant_id = i.tenant_id
    let rows_fixed = sqlx::query(
        r#"
        SELECT COALESCE(v.name, i.vendor_name) as vendor_name
        FROM invoices i
        LEFT JOIN vendors v ON i.vendor_id = v.id AND v.tenant_id = i.tenant_id
        WHERE i.vendor_id IS NOT NULL AND i.tenant_id = $1
        "#,
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("fixed join query");

    assert_eq!(rows_fixed.len(), 1, "should have one row");
    let name_fixed: String = rows_fixed[0].get("vendor_name");
    assert_eq!(
        name_fixed, "Original Fallback",
        "fixed join must use fallback name, not cross-tenant vendor's real name"
    );

    cleanup(&pool, tenant_a, tenant_b).await;
}
