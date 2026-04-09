//! Integration tests for billing usage metering
//!
//! Verifies that `get_tenant_usage` correctly counts invoices and vendors
//! per-tenant within the billing period window.
//!
//! These tests require a running Postgres instance and are gated behind
//! `#[cfg_attr(not(feature = "integration"), ignore)]` so `cargo test`
//! skips them by default. Run with `--features integration` or `-- --ignored`.

use billforge_billing::get_tenant_usage;
use billforge_core::TenantId;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers (minimal rows to satisfy FK constraints)
// ---------------------------------------------------------------------------

async fn seed_user(pool: &PgPool, tenant_id: &TenantId, user_id: Uuid) {
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

async fn seed_vendor(pool: &PgPool, tenant_id: &TenantId, vendor_id: Uuid) {
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

/// Insert a minimal invoice row. `created_at` is explicitly set so tests can
/// control the period window.
async fn seed_invoice_at(
    pool: &PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
    vendor_id: Uuid,
    user_id: Uuid,
    created_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number,
                                total_amount_cents, document_id, created_by, created_at)
         VALUES ($1, $2, $3, $4, $5, 1000, $6, $7, $8)",
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id)
    .bind("Test Vendor")
    .bind(format!("INV-{}", invoice_id))
    .bind(Uuid::new_v4()) // document_id
    .bind(user_id)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("seed invoice");
}

// ---------------------------------------------------------------------------
// Pool + migration setup
// ---------------------------------------------------------------------------

async fn setup_pool() -> PgPool {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".to_string());
    let pool = PgPool::connect(&db_url).await.expect("connect to test DB");

    // Run migrations so invoices/vendors tables exist
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    pool
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_empty_tenant_has_zero_invoices() {
    let pool = setup_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let now = Utc::now();
    let period_start = now - Duration::days(30);
    let period_end = now + Duration::days(30);

    let usage = get_tenant_usage(&pool, &tenant_id, period_start, period_end)
        .await
        .expect("get_tenant_usage");

    assert_eq!(usage.invoices_count, 0);
    assert_eq!(usage.vendor_count, 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_invoice_count_respects_period_window() {
    let pool = setup_pool().await;
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();

    seed_user(&pool, &tenant_id, user_id).await;
    seed_vendor(&pool, &tenant_id, vendor_id).await;

    let now = Utc::now();
    let period_start = now - Duration::days(30);
    let period_end = now + Duration::days(1);

    // 1 invoice BEFORE the window
    seed_invoice_at(
        &pool,
        &tenant_id,
        Uuid::new_v4(),
        vendor_id,
        user_id,
        period_start - Duration::days(5),
    )
    .await;

    // 2 invoices INSIDE the window
    seed_invoice_at(
        &pool,
        &tenant_id,
        Uuid::new_v4(),
        vendor_id,
        user_id,
        period_start + Duration::days(1),
    )
    .await;
    seed_invoice_at(
        &pool,
        &tenant_id,
        Uuid::new_v4(),
        vendor_id,
        user_id,
        now,
    )
    .await;

    let usage = get_tenant_usage(&pool, &tenant_id, period_start, period_end)
        .await
        .expect("get_tenant_usage");

    assert_eq!(usage.invoices_count, 2, "only invoices within the period should be counted");
    assert_eq!(usage.vendor_count, 1);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_invoice_count_isolated_by_tenant() {
    let pool = setup_pool().await;
    let tenant_a = TenantId::from_uuid(Uuid::new_v4());
    let tenant_b = TenantId::from_uuid(Uuid::new_v4());

    let user_a = Uuid::new_v4();
    let vendor_a = Uuid::new_v4();
    seed_user(&pool, &tenant_a, user_a).await;
    seed_vendor(&pool, &tenant_a, vendor_a).await;

    let user_b = Uuid::new_v4();
    let vendor_b = Uuid::new_v4();
    seed_user(&pool, &tenant_b, user_b).await;
    seed_vendor(&pool, &tenant_b, vendor_b).await;

    let now = Utc::now();
    let period_start = now - Duration::days(30);
    let period_end = now + Duration::days(1);

    // 3 invoices for tenant A
    for _ in 0..3 {
        seed_invoice_at(
            &pool,
            &tenant_a,
            Uuid::new_v4(),
            vendor_a,
            user_a,
            now,
        )
        .await;
    }

    // 1 invoice for tenant B
    seed_invoice_at(
        &pool,
        &tenant_b,
        Uuid::new_v4(),
        vendor_b,
        user_b,
        now,
    )
    .await;

    let usage_a = get_tenant_usage(&pool, &tenant_a, period_start, period_end)
        .await
        .expect("get_tenant_usage A");
    assert_eq!(usage_a.invoices_count, 3, "tenant A should see only its own invoices");

    let usage_b = get_tenant_usage(&pool, &tenant_b, period_start, period_end)
        .await
        .expect("get_tenant_usage B");
    assert_eq!(usage_b.invoices_count, 1, "tenant B should see only its own invoices");
}
