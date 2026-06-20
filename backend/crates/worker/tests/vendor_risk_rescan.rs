//! Integration test for the continuous VendorRiskRescan worker (refs #381).
//!
//! Verifies that the rescan loop:
//!   1. Calls the OFAC screener for every active vendor in a tenant.
//!   2. Produces a `critical` `sanctions_hit` alert + sets payment_hold when a
//!      vendor's name newly matches a seed SDN entry.
//!   3. Is idempotent on re-run: the second pass does not insert a duplicate
//!      alert (payload_hash dedupe).
//!
//! Run: `cargo test -p billforge-worker --test vendor_risk_rescan -- --ignored`

#![allow(warnings)]

use billforge_core::TenantId;
use billforge_db::PgManager;
use billforge_worker::jobs::vendor_risk_rescan::rescan_tenant_with_provider;
use billforge_worker::jobs::vendor_risk_rescan::NullProvider;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers (mirror backend/crates/api/tests/notifications_inbox_test.rs)
// ---------------------------------------------------------------------------

async fn seed_tenant(pool: &sqlx::PgPool, tenant_id: &TenantId, name: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .bind(name.to_lowercase().replace(' ', "-"))
    .execute(pool)
    .await
    .expect("seed tenant");
}

async fn seed_vendor(pool: &sqlx::PgPool, tenant_id: &TenantId, vendor_id: Uuid, name: &str) {
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, vendor_type) \
         VALUES ($1, $2, $3, 'business') ON CONFLICT (id) DO NOTHING",
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .execute(pool)
    .await
    .expect("seed vendor");
}

async fn count_open_alerts(pool: &sqlx::PgPool, vendor_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM vendor_risk_alerts \
         WHERE vendor_id = $1 AND status = 'open'",
    )
    .bind(vendor_id)
    .fetch_one(pool)
    .await
    .expect("count open alerts")
}

async fn read_payment_hold(pool: &sqlx::PgPool, vendor_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT payment_hold FROM vendors WHERE id = $1")
        .bind(vendor_id)
        .fetch_one(pool)
        .await
        .expect("read payment_hold")
}

async fn read_last_rescan_at(
    pool: &sqlx::PgPool,
    vendor_id: Uuid,
) -> Option<chrono::DateTime<chrono::Utc>> {
    sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT last_risk_rescan_at FROM vendors WHERE id = $1",
    )
    .bind(vendor_id)
    .fetch_one(pool)
    .await
    .expect("read last_risk_rescan_at")
}

async fn setup_tenant(tag: &str) -> (PgManager, TenantId, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("PgManager");

    let tenant_id: TenantId =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("rescan-{tag}").as_bytes())
            .to_string()
            .parse()
            .unwrap();

    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(&tenant_id, &format!("Rescan Tenant {tag}"))
        .await
        .expect("create tenant");

    let pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();

    // run_tenant_migrations provisions vendor_risk_alerts (migration 135)
    // and the vendors.last_risk_rescan_at column.
    manager.run_tenant_migrations(&pool).await.expect("migrate");
    seed_tenant(&pool, &tenant_id, &format!("Rescan {tag}")).await;

    (manager, tenant_id, pool)
}

async fn teardown_tenant(manager: &PgManager, tenant_id: &TenantId) {
    manager.delete_tenant(tenant_id).await.ok();
}

// ===========================================================================
// Test 1: rescan flags a sanctioned vendor and is idempotent on re-run
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn rescan_flags_sanctioned_vendor_and_is_idempotent_on_rerun() {
    let (manager, tenant_id, pool) = setup_tenant("sanctions").await;

    // Vendor name that matches an SDN seed entry ("Al-Qaeda" is exercised in
    // the existing ofac_screening_test.rs and is bundled in the seed list).
    let sanctioned_vendor = Uuid::new_v4();
    // Innocent vendor that should never produce an alert.
    let clean_vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, sanctioned_vendor, "Al-Qaeda").await;
    seed_vendor(&pool, &tenant_id, clean_vendor, "Acme Coffee LLC").await;

    // First pass: sanctioned vendor gets a critical sanctions_hit alert,
    // payment_hold flips true, and last_risk_rescan_at is set on both vendors.
    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan pass 1");

    assert_eq!(
        count_open_alerts(&pool, sanctioned_vendor).await,
        1,
        "sanctioned vendor must produce exactly one alert on first pass"
    );
    assert_eq!(
        count_open_alerts(&pool, clean_vendor).await,
        0,
        "clean vendor must produce no alert"
    );
    assert!(
        read_payment_hold(&pool, sanctioned_vendor).await,
        "payment_hold must be set after a sanctions hit"
    );
    assert!(
        read_last_rescan_at(&pool, sanctioned_vendor)
            .await
            .is_some(),
        "last_risk_rescan_at must be updated by the rescan"
    );

    // Verify the alert carries the sanctions_hit discriminator + critical severity.
    let (alert_type, severity): (String, String) =
        sqlx::query_as("SELECT alert_type, severity FROM vendor_risk_alerts WHERE vendor_id = $1")
            .bind(sanctioned_vendor)
            .fetch_one(&pool)
            .await
            .expect("fetch alert row");
    assert_eq!(alert_type, "sanctions_hit");
    assert_eq!(severity, "critical");

    // Second pass: payload_hash dedupe must NOT create a duplicate alert.
    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan pass 2");

    assert_eq!(
        count_open_alerts(&pool, sanctioned_vendor).await,
        1,
        "second rescan pass must not duplicate the alert (idempotent on payload hash)"
    );

    teardown_tenant(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 2: rescan skips inactive vendors (only status='active' is scanned)
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn rescan_skips_inactive_vendors() {
    let (manager, tenant_id, pool) = setup_tenant("inactive").await;

    let inactive_vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, inactive_vendor, "Al-Qaeda").await;
    // Flip the vendor to inactive (the rescan must skip it).
    sqlx::query("UPDATE vendors SET status = 'inactive' WHERE id = $1")
        .bind(inactive_vendor)
        .execute(&pool)
        .await
        .expect("set inactive");

    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan");

    assert_eq!(
        count_open_alerts(&pool, inactive_vendor).await,
        0,
        "inactive vendor must not be rescanned"
    );

    teardown_tenant(&manager, &tenant_id).await;
}
