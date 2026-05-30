//! Integration tests for the discounts KPI query.
//!
//! Verifies that `fetch_kpi_stats` (now using parameterised `make_interval(days => $2)`)
//! correctly partitions discount data by the bound day count and returns zero counts
//! for tenants with no discount activity.
//!
//! Run: `cargo test -p billforge-api --test discounts_kpi_test -- --ignored`

use sqlx::Row;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Insert a test vendor and return its ID.
async fn insert_vendor(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, status, routing_rules, created_at, updated_at)
           VALUES ($1, $2, $3, 'active', '{}'::jsonb, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("Failed to insert test vendor");
    id
}

/// Insert a minimal invoice row and return its ID.
async fn insert_invoice(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    invoice_number: &str,
    total_amount_cents: i64,
    discount_percent: f64,
) -> Uuid {
    let id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Ensure the user row exists for the FK constraint on created_by
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name) \
         VALUES ($1, $2, 'kpi-test@example.com', '', 'KPI Test') \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .ok();

    sqlx::query(
        r#"INSERT INTO invoices
               (id, tenant_id, vendor_id, vendor_name, invoice_number, document_id,
                currency, total_amount_cents, capture_status, processing_status,
                discount_percent, created_by, created_at, updated_at)
           VALUES ($1, $2, $3, 'KPI Test Vendor', $4, $5, 'USD', $6, 'complete', 'received', $7, $8, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(invoice_number)
    .bind(doc_id)
    .bind(total_amount_cents)
    .bind(discount_percent)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to insert test invoice");
    id
}

/// The parameterised KPI query from `fetch_kpi_stats`.
/// Mirrors the SQL in `backend/crates/api/src/routes/discounts.rs`.
async fn fetch_kpi_stats(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    interval_days: i32,
) -> (i64, i64, i64, i64) {
    let row = sqlx::query(
        r#"SELECT
             COUNT(*) FILTER (WHERE discount_captured_at >= NOW() - make_interval(days => $2)) AS captured_count,
             COALESCE(SUM(
               CASE WHEN discount_captured_at >= NOW() - make_interval(days => $2)
                    THEN ROUND(total_amount_cents * discount_percent / 100.0)
                    ELSE 0 END
             ), 0) AS captured_savings_cents,
             COUNT(*) FILTER (WHERE discount_missed_at >= NOW() - make_interval(days => $2)) AS missed_count,
             COALESCE(SUM(
               CASE WHEN discount_missed_at >= NOW() - make_interval(days => $2)
                    THEN ROUND(total_amount_cents * discount_percent / 100.0)
                    ELSE 0 END
             ), 0) AS missed_savings_cents
           FROM invoices
           WHERE tenant_id = $1
             AND (discount_captured_at IS NOT NULL OR discount_missed_at IS NOT NULL)"#,
    )
    .bind(tenant_id)
    .bind(interval_days)
    .fetch_one(pool)
    .await
    .expect("KPI query should succeed");

    (
        row.get::<i64, _>("captured_count"),
        row.get::<i64, _>("captured_savings_cents"),
        row.get::<i64, _>("missed_count"),
        row.get::<i64, _>("missed_savings_cents"),
    )
}

/// Cleanup helper.
async fn cleanup_test_data(pool: &sqlx::PgPool, tenant_id: Uuid, prefix: &str) {
    sqlx::query("DELETE FROM invoices WHERE tenant_id = $1 AND invoice_number LIKE $2")
        .bind(tenant_id)
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 AND name LIKE $2")
        .bind(tenant_id)
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
}

// ===========================================================================
// Test 1: Empty tenant returns zero counts
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test discounts_kpi_test -- --ignored
async fn get_kpi_returns_zero_counts_for_empty_tenant() {
    let pool = get_pool().await;
    let tenant_id = Uuid::new_v4();

    // No invoices exist for this tenant, so all KPI fields should be 0
    let (captured_count, captured_savings, missed_count, missed_savings) =
        fetch_kpi_stats(&pool, tenant_id, 30).await;

    assert_eq!(
        captured_count, 0,
        "captured_count should be 0 for empty tenant"
    );
    assert_eq!(captured_savings, 0, "captured_savings_cents should be 0");
    assert_eq!(missed_count, 0, "missed_count should be 0 for empty tenant");
    assert_eq!(missed_savings, 0, "missed_savings_cents should be 0");

    // Also verify the 90d window
    let (captured_count_90, captured_savings_90, missed_count_90, missed_savings_90) =
        fetch_kpi_stats(&pool, tenant_id, 90).await;

    assert_eq!(captured_count_90, 0);
    assert_eq!(captured_savings_90, 0);
    assert_eq!(missed_count_90, 0);
    assert_eq!(missed_savings_90, 0);
}

// ===========================================================================
// Test 2: Invoices are partitioned by 30 and 90 day windows
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test discounts_kpi_test -- --ignored
async fn get_kpi_partitions_by_30_and_90_day_windows() {
    let pool = get_pool().await;
    let tenant_id = Uuid::new_v4();
    let prefix = "KPI-WIN";

    // Create vendor
    let vendor_id = insert_vendor(&pool, tenant_id, "KPI-WIN Vendor").await;

    // Invoice 1: captured 10 days ago (within both 30d and 90d windows)
    let inv1 = insert_invoice(&pool, tenant_id, vendor_id, "KPI-WIN-001", 10_000, 2.0).await;
    sqlx::query(
        "UPDATE invoices SET discount_captured_at = NOW() - INTERVAL '10 days', discount_percent = 2.0 WHERE id = $1",
    )
    .bind(inv1)
    .execute(&pool)
    .await
    .expect("Failed to set discount_captured_at");

    // Invoice 2: captured 60 days ago (outside 30d, within 90d window)
    let inv2 = insert_invoice(&pool, tenant_id, vendor_id, "KPI-WIN-002", 20_000, 2.0).await;
    sqlx::query(
        "UPDATE invoices SET discount_captured_at = NOW() - INTERVAL '60 days', discount_percent = 2.0 WHERE id = $1",
    )
    .bind(inv2)
    .execute(&pool)
    .await
    .expect("Failed to set discount_captured_at");

    // Invoice 3: missed 100 days ago (outside both 30d and 90d windows)
    let inv3 = insert_invoice(&pool, tenant_id, vendor_id, "KPI-WIN-003", 15_000, 2.0).await;
    sqlx::query(
        "UPDATE invoices SET discount_missed_at = NOW() - INTERVAL '100 days', discount_percent = 2.0 WHERE id = $1",
    )
    .bind(inv3)
    .execute(&pool)
    .await
    .expect("Failed to set discount_missed_at");

    // 30d window: only invoice 1 should be counted as captured
    let (captured_count_30, captured_savings_30, missed_count_30, missed_savings_30) =
        fetch_kpi_stats(&pool, tenant_id, 30).await;

    assert_eq!(captured_count_30, 1, "30d captured_count should be 1");
    assert_eq!(
        captured_savings_30, 200,
        "30d captured_savings should be 200 (10000 * 2%)"
    );
    assert_eq!(
        missed_count_30, 0,
        "30d missed_count should be 0 (invoice 3 is outside 30d)"
    );
    assert_eq!(missed_savings_30, 0, "30d missed_savings should be 0");

    // 90d window: invoices 1 and 2 should be counted as captured
    let (captured_count_90, captured_savings_90, missed_count_90, missed_savings_90) =
        fetch_kpi_stats(&pool, tenant_id, 90).await;

    assert_eq!(captured_count_90, 2, "90d captured_count should be 2");
    assert_eq!(
        captured_savings_90, 600,
        "90d captured_savings should be 600 (200 + 400)"
    );
    assert_eq!(
        missed_count_90, 0,
        "90d missed_count should be 0 (invoice 3 is outside 90d)"
    );
    assert_eq!(missed_savings_90, 0, "90d missed_savings should be 0");

    // Cleanup
    cleanup_test_data(&pool, tenant_id, prefix).await;
}
