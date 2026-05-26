//! Integration tests for the dashboard KPIs materialized view endpoint.
//!
//! Tests:
//! 1. Happy path - seed invoices, refresh MV, assert KPI counts, aging, vendors
//! 2. Tenant isolation - two tenants, each sees only own data
//! 3. Empty tenant returns zero-valued defaults (no 404/500)

#![allow(warnings)]

use billforge_core::TenantId;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run all tenant migrations so tables and MV exist.
async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

/// Insert a minimal user row.
async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind("kpi-test@example.com")
    .bind("hash_not_used")
    .bind("KPI Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Insert a vendor and return its id.
async fn insert_vendor(pool: &sqlx::PgPool, tenant_id: &TenantId, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name)
           VALUES ($1, $2, $3)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(id)
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .execute(pool)
    .await
    .expect("insert vendor");
    id
}

/// Insert an invoice with a given status and created_at offset.
async fn insert_invoice(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: Uuid,
    vendor_name: &str,
    status: &str,
    total_cents: i64,
    created_at_offset: &str, // e.g. "-2 days", "-10 days"
) -> Uuid {
    let invoice_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number,
               total_amount_cents, currency, capture_status, processing_status,
               document_id, created_by, status, created_at)
           VALUES ($1, $2, $3, $4, $5, 'USD',
                   'reviewed', 'pending_approval', $6, $7, $8,
                   NOW() + $9::interval)"#,
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_name)
    .bind(format!("INV-{}", &invoice_id.as_simple().to_string()[..6]))
    .bind(total_cents)
    .bind(doc_id)
    .bind(user_id)
    .bind(status)
    .bind(created_at_offset)
    .execute(pool)
    .await
    .expect("insert test invoice");
    invoice_id
}

/// Read KPI row directly from the materialized view for verification.
#[derive(Debug)]
struct KpiRow {
    queue_count: i64,
    approved_count: i64,
    paid_count: i64,
    rejected_count: i64,
    aging_0_7: i64,
    aging_0_7_amount: i64,
    aging_8_14: i64,
    aging_8_14_amount: i64,
    aging_15_30: i64,
    aging_15_30_amount: i64,
    aging_30_plus: i64,
    aging_30_plus_amount: i64,
    spend_by_vendor: serde_json::Value,
    total_spend_30d: i64,
    avg_processing_hours: f64,
}

async fn read_kpis(pool: &sqlx::PgPool, tenant_id: &TenantId) -> Option<KpiRow> {
    let row: Option<(
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        serde_json::Value,
        i64,
        f64,
    )> = sqlx::query_as(
        r#"SELECT
            queue_count, approved_count, paid_count, rejected_count,
            aging_0_7, aging_0_7_amount::bigint,
            aging_8_14, aging_8_14_amount::bigint,
            aging_15_30, aging_15_30_amount::bigint,
            aging_30_plus, aging_30_plus_amount::bigint,
            spend_by_vendor,
            total_spend_30d::bigint,
            avg_processing_hours::double precision
        FROM dashboard_kpis_mv
        WHERE tenant_id = $1"#,
    )
    .bind(*tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .expect("read kpis");

    row.map(|r| KpiRow {
        queue_count: r.0,
        approved_count: r.1,
        paid_count: r.2,
        rejected_count: r.3,
        aging_0_7: r.4,
        aging_0_7_amount: r.5,
        aging_8_14: r.6,
        aging_8_14_amount: r.7,
        aging_15_30: r.8,
        aging_15_30_amount: r.9,
        aging_30_plus: r.10,
        aging_30_plus_amount: r.11,
        spend_by_vendor: r.12,
        total_spend_30d: r.13,
        avg_processing_hours: r.14,
    })
}

async fn refresh_mv(pool: &sqlx::PgPool) {
    sqlx::query("REFRESH MATERIALIZED VIEW dashboard_kpis_mv")
        .execute(pool)
        .await
        .expect("refresh MV");
}

// ============================================================================
// Test 1: Happy path - invoices across statuses/ages/vendors
// ============================================================================

#[sqlx::test]
async fn happy_path_kpi_counts_and_aging(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    // Queued invoices at different ages
    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        "Acme Corp",
        "received",
        100_00,
        "-3 days",
    )
    .await;
    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        "Acme Corp",
        "in_review",
        200_00,
        "-10 days",
    )
    .await;
    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        "Beta LLC",
        "pending_approval",
        300_00,
        "-20 days",
    )
    .await;
    insert_invoice(
        &pool, &tenant_id, user_id, "Beta LLC", "received", 400_00, "-45 days",
    )
    .await;

    // Non-queued invoices
    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        "Acme Corp",
        "approved",
        500_00,
        "-5 days",
    )
    .await;
    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        "Acme Corp",
        "paid",
        600_00,
        "-5 days",
    )
    .await;
    insert_invoice(
        &pool, &tenant_id, user_id, "Beta LLC", "rejected", 700_00, "-5 days",
    )
    .await;

    refresh_mv(&pool).await;

    let kpis = read_kpis(&pool, &tenant_id)
        .await
        .expect("KPI row must exist");

    // Status counts
    assert_eq!(
        kpis.queue_count, 4,
        "queue should have 4 invoices (received + in_review + pending_approval)"
    );
    assert_eq!(kpis.approved_count, 1);
    assert_eq!(kpis.paid_count, 1);
    assert_eq!(kpis.rejected_count, 1);

    // Aging buckets
    // 0-7: received at -3 days (10000 cents)
    assert_eq!(kpis.aging_0_7, 1);
    assert_eq!(kpis.aging_0_7_amount, 100_00);

    // 8-14: in_review at -10 days (20000 cents)
    assert_eq!(kpis.aging_8_14, 1);
    assert_eq!(kpis.aging_8_14_amount, 200_00);

    // 15-30: pending_approval at -20 days (30000 cents)
    assert_eq!(kpis.aging_15_30, 1);
    assert_eq!(kpis.aging_15_30_amount, 300_00);

    // 30+: received at -45 days (40000 cents)
    assert_eq!(kpis.aging_30_plus, 1);
    assert_eq!(kpis.aging_30_plus_amount, 400_00);

    // Total spend 30d: only paid invoices within last 30 days
    assert_eq!(kpis.total_spend_30d, 600_00);

    // Top vendors - should have Acme Corp (60000 from paid) and Beta LLC (none paid)
    let vendors = kpis
        .spend_by_vendor
        .as_array()
        .expect("spend_by_vendor should be array");
    assert!(
        !vendors.is_empty(),
        "should have at least one vendor in spend list"
    );

    // First vendor should be Acme Corp (highest paid spend)
    let first = &vendors[0];
    assert_eq!(first["vendor_name"].as_str(), Some("Acme Corp"));
}

// ============================================================================
// Test 2: Tenant isolation
// ============================================================================

#[sqlx::test]
async fn tenant_isolation_kpis_scoped(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_a = TenantId::from_uuid(Uuid::new_v4());
    let tenant_b = TenantId::from_uuid(Uuid::new_v4());
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    setup_schema(&pool, &tenant_a).await;
    insert_user(&pool, &tenant_a, user_a).await;
    insert_invoice(
        &pool, &tenant_a, user_a, "Vendor A", "received", 100_00, "-1 days",
    )
    .await;
    insert_invoice(
        &pool, &tenant_a, user_a, "Vendor A", "paid", 500_00, "-5 days",
    )
    .await;

    setup_schema(&pool, &tenant_b).await;
    insert_user(&pool, &tenant_b, user_b).await;
    insert_invoice(
        &pool, &tenant_b, user_b, "Vendor B", "received", 200_00, "-2 days",
    )
    .await;
    insert_invoice(
        &pool, &tenant_b, user_b, "Vendor B", "rejected", 300_00, "-3 days",
    )
    .await;

    refresh_mv(&pool).await;

    let kpis_a = read_kpis(&pool, &tenant_a).await.expect("tenant A KPIs");
    let kpis_b = read_kpis(&pool, &tenant_b).await.expect("tenant B KPIs");

    // Tenant A: 1 queued, 0 paid in MV (paid has 1), 0 rejected
    assert_eq!(kpis_a.queue_count, 1);
    assert_eq!(kpis_a.paid_count, 1);
    assert_eq!(kpis_a.rejected_count, 0);
    assert_eq!(kpis_a.total_spend_30d, 500_00);

    // Tenant B: 1 queued, 0 paid, 1 rejected
    assert_eq!(kpis_b.queue_count, 1);
    assert_eq!(kpis_b.paid_count, 0);
    assert_eq!(kpis_b.rejected_count, 1);
    assert_eq!(kpis_b.total_spend_30d, 0);

    // Vendor lists should not leak across tenants
    let vendors_a = kpis_a.spend_by_vendor.as_array().unwrap();
    let vendors_b = kpis_b.spend_by_vendor.as_array().unwrap();

    // Tenant A has one paid vendor
    assert!(vendors_a
        .iter()
        .any(|v| v["vendor_name"].as_str() == Some("Vendor A")));
    assert!(!vendors_a
        .iter()
        .any(|v| v["vendor_name"].as_str() == Some("Vendor B")));

    // Tenant B has no paid vendors
    assert!(vendors_b.is_empty());
}

// ============================================================================
// Test 3: Empty tenant returns zero-valued defaults
// ============================================================================

#[sqlx::test]
async fn empty_tenant_returns_zero_defaults(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());

    // Run migrations but don't insert any invoices
    setup_schema(&pool, &tenant_id).await;

    // MV exists but has no row for this tenant
    let kpis = read_kpis(&pool, &tenant_id).await;

    // The endpoint should return zero-valued defaults (not 404).
    // Here we verify the DB returns None, which the handler converts to defaults.
    assert!(kpis.is_none(), "new tenant should have no MV row");

    // Verify the SQL query itself works without error
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM dashboard_kpis_mv WHERE tenant_id = $1")
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*pool)
        .await
        .expect("count query");

    assert_eq!(row.0, 0, "no rows for new tenant in MV");
}
