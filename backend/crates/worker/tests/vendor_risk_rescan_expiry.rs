//! Integration tests for the compliance soft-hit dimensions of the continuous
//! VendorRiskRescan worker (refs #441).
//!
//! Verifies that, in the same rescan pass that today emits `sanctions_hit`:
//!   1. W-9 expiry within 30 days emits a `w9_expiring` medium alert and never
//!      sets payment_hold.
//!   2. An expired W-9 evaluated during 1099 season escalates to severity=high.
//!   3. An expired COI emits a `coi_expired` high alert without payment_hold.
//!   4. A 1099-eligible vendor over the $600 YTD threshold with no W-9 emits
//!      `threshold_1099_no_w9` (high), payment_hold stays false.
//!   5. When a vendor has BOTH a sanctions hit and an expired W-9, two alerts
//!      are emitted but the sanctions path is what flips payment_hold=true.
//!   6. Re-running the job is idempotent on the new alert kinds: one row per
//!      (vendor, kind, payload_hash).
//!
//! Run: `cargo test -p billforge-worker --test vendor_risk_rescan_expiry -- --ignored`

#![allow(warnings)]

use billforge_core::TenantId;
use billforge_db::PgManager;
use billforge_worker::jobs::vendor_risk_rescan::{rescan_tenant_with_provider, NullProvider};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers (mirror vendor_risk_rescan.rs harness)
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

#[derive(Default, Clone)]
struct ComplianceState {
    w9_on_file: bool,
    w9_expires_on: Option<NaiveDate>,
    w8_received_date: Option<NaiveDate>,
    w8_expires_on: Option<NaiveDate>,
    coi_expires_on: Option<NaiveDate>,
    is_1099_eligible: bool,
    ytd_paid_cents: i64,
}

async fn set_compliance(pool: &sqlx::PgPool, vendor_id: Uuid, c: ComplianceState) {
    sqlx::query(
        r#"
        UPDATE vendors SET
            w9_on_file = $2,
            w9_expires_on = $3,
            w8_received_date = $4,
            w8_expires_on = $5,
            coi_expires_on = $6,
            is_1099_eligible = $7,
            ytd_paid_cents = $8
        WHERE id = $1
        "#,
    )
    .bind(vendor_id)
    .bind(c.w9_on_file)
    .bind(c.w9_expires_on)
    .bind(c.w8_received_date)
    .bind(c.w8_expires_on)
    .bind(c.coi_expires_on)
    .bind(c.is_1099_eligible)
    .bind(c.ytd_paid_cents)
    .execute(pool)
    .await
    .expect("set compliance state");
}

async fn fetch_alerts(pool: &sqlx::PgPool, vendor_id: Uuid) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT alert_type, severity FROM vendor_risk_alerts \
         WHERE vendor_id = $1 AND status = 'open' ORDER BY alert_type",
    )
    .bind(vendor_id)
    .fetch_all(pool)
    .await
    .expect("fetch alerts")
}

async fn read_payment_hold(pool: &sqlx::PgPool, vendor_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT payment_hold FROM vendors WHERE id = $1")
        .bind(vendor_id)
        .fetch_one(pool)
        .await
        .expect("read payment_hold")
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
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("expiry-{tag}").as_bytes())
            .to_string()
            .parse()
            .unwrap();

    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(&tenant_id, &format!("Expiry Tenant {tag}"))
        .await
        .expect("create tenant");

    let pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();
    manager.run_tenant_migrations(&pool).await.expect("migrate");
    seed_tenant(&pool, &tenant_id, &format!("Expiry {tag}")).await;

    (manager, tenant_id, pool)
}

async fn teardown(manager: &PgManager, tenant_id: &TenantId) {
    manager.delete_tenant(tenant_id).await.ok();
}

// ===========================================================================
// Test 1: W-9 expiring within 30 days emits a single medium alert and does
// NOT flip payment_hold.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn w9_expiring_emits_medium_alert_without_payment_hold() {
    let (manager, tenant_id, pool) = setup_tenant("w9-expiring").await;
    let vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, vendor, "Soon Expiring W9 LLC").await;
    let today = Utc::now().date_naive();
    set_compliance(
        &pool,
        vendor,
        ComplianceState {
            w9_on_file: true,
            w9_expires_on: Some(today + Duration::days(20)),
            ..Default::default()
        },
    )
    .await;

    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan");

    let alerts = fetch_alerts(&pool, vendor).await;
    assert_eq!(
        alerts,
        vec![("w9_expiring".to_string(), "medium".to_string())],
        "exactly one w9_expiring (medium) alert"
    );
    assert!(
        !read_payment_hold(&pool, vendor).await,
        "soft hit must not flip payment_hold"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 2: Expired W-9 evaluated during 1099 season escalates to high.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn w9_expired_escalates_to_high_during_1099_season() {
    let (manager, tenant_id, pool) = setup_tenant("w9-expired-season").await;
    let vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, vendor, "Expired W9 Vendor").await;

    let today = Utc::now().date_naive();
    // Only run the escalation assertion during 1099 season (Nov/Dec/Jan).
    // Outside that window, the alert is medium, which is still asserted.
    let in_season = matches!(today.month(), 11 | 12 | 1);
    let expected_severity = if in_season { "high" } else { "medium" };

    set_compliance(
        &pool,
        vendor,
        ComplianceState {
            w9_on_file: true,
            w9_expires_on: Some(today - Duration::days(5)),
            ..Default::default()
        },
    )
    .await;

    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan");

    let alerts = fetch_alerts(&pool, vendor).await;
    assert_eq!(
        alerts,
        vec![("w9_expired".to_string(), expected_severity.to_string())],
        "w9_expired severity matches 1099-season escalation rule"
    );
    assert!(
        !read_payment_hold(&pool, vendor).await,
        "soft hit must not flip payment_hold"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 3: Expired COI emits a high alert; payment_hold remains false.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn coi_expired_emits_high_alert_without_payment_hold() {
    let (manager, tenant_id, pool) = setup_tenant("coi-expired").await;
    let vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, vendor, "COI Lapsed Inc").await;
    let today = Utc::now().date_naive();
    set_compliance(
        &pool,
        vendor,
        ComplianceState {
            coi_expires_on: Some(today - Duration::days(1)),
            ..Default::default()
        },
    )
    .await;

    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan");

    let alerts = fetch_alerts(&pool, vendor).await;
    assert_eq!(
        alerts,
        vec![("coi_expired".to_string(), "high".to_string())],
        "exactly one coi_expired (high) alert"
    );
    assert!(
        !read_payment_hold(&pool, vendor).await,
        "soft hit must not flip payment_hold"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 4: 1099-eligible vendor over $600 YTD with no W-9 emits high alert.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn threshold_1099_without_w9_emits_high_alert() {
    let (manager, tenant_id, pool) = setup_tenant("threshold-1099").await;
    let vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, vendor, "1099 Eligible Vendor").await;
    set_compliance(
        &pool,
        vendor,
        ComplianceState {
            w9_on_file: false,
            is_1099_eligible: true,
            ytd_paid_cents: 700_00,
            ..Default::default()
        },
    )
    .await;

    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan");

    let alerts = fetch_alerts(&pool, vendor).await;
    assert_eq!(
        alerts,
        vec![("threshold_1099_no_w9".to_string(), "high".to_string())],
        "exactly one threshold_1099_no_w9 (high) alert"
    );
    assert!(
        !read_payment_hold(&pool, vendor).await,
        "soft hit must not flip payment_hold"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 5: Sanctions hit + expired W-9 -> two alerts, payment_hold=true
// (sanctions remains the only path that hard-blocks payment).
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn sanctions_plus_expired_w9_keeps_payment_hold_owned_by_sanctions() {
    let (manager, tenant_id, pool) = setup_tenant("sanctions+w9").await;
    let vendor = Uuid::new_v4();
    // "Al-Qaeda" matches the bundled OFAC seed used by the existing test suite.
    seed_vendor(&pool, &tenant_id, vendor, "Al-Qaeda").await;
    let today = Utc::now().date_naive();
    set_compliance(
        &pool,
        vendor,
        ComplianceState {
            w9_on_file: true,
            w9_expires_on: Some(today - Duration::days(10)),
            ..Default::default()
        },
    )
    .await;

    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
        .await
        .expect("rescan");

    let alerts = fetch_alerts(&pool, vendor).await;
    assert_eq!(alerts.len(), 2, "expected two alerts (sanctions + w9_expired)");
    assert!(
        alerts.iter().any(|(k, s)| k == "sanctions_hit" && s == "critical"),
        "sanctions_hit critical alert is present"
    );
    assert!(
        alerts.iter().any(|(k, _)| k == "w9_expired"),
        "w9_expired soft alert is present"
    );
    assert!(
        read_payment_hold(&pool, vendor).await,
        "payment_hold is owned by sanctions_hit, which still fires"
    );

    teardown(&manager, &tenant_id).await;
}

// ===========================================================================
// Test 6: Re-running the job does not duplicate the new soft alerts.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn rescan_is_idempotent_for_soft_hits() {
    let (manager, tenant_id, pool) = setup_tenant("idem-soft").await;
    let vendor = Uuid::new_v4();
    seed_vendor(&pool, &tenant_id, vendor, "Idem Soft Vendor").await;
    let today = Utc::now().date_naive();
    set_compliance(
        &pool,
        vendor,
        ComplianceState {
            w9_on_file: true,
            w9_expires_on: Some(today + Duration::days(10)),
            coi_expires_on: Some(today - Duration::days(2)),
            ..Default::default()
        },
    )
    .await;

    for _ in 0..2 {
        rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &NullProvider, None)
            .await
            .expect("rescan");
    }

    let alerts = fetch_alerts(&pool, vendor).await;
    assert_eq!(
        alerts.len(),
        2,
        "two passes still produce one alert per (vendor, kind, payload_hash)"
    );
    let kinds: Vec<&str> = alerts.iter().map(|(k, _)| k.as_str()).collect();
    assert!(kinds.contains(&"w9_expiring"));
    assert!(kinds.contains(&"coi_expired"));

    teardown(&manager, &tenant_id).await;
}
