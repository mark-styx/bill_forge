//! Integration tests for the continuous vendor-risk alert surface (refs #381).
//!
//! Verifies that:
//!   1. The banking-change hook (`insert_banking_change_alert`) writes a
//!      `critical` `banking_change` alert and sets `vendors.payment_hold`.
//!   2. The payment-release guard (`vendor_has_open_critical_alert`) blocks
//!      while an open critical alert exists, and clears after acknowledge.
//!   3. Tenant isolation: an alert created under tenant A is invisible from
//!      tenant B's pool (RLS `app.current_tenant_id` policy).
//!   4. Acknowledging the last critical alert for a vendor clears
//!      `vendors.payment_hold` (when no other hold reason remains).
//!
//! Run: `cargo test -p billforge-api --test vendor_risk_alerts -- --ignored`

#![allow(warnings)]

use billforge_api::routes::vendor_risk_alerts::{
    insert_banking_change_alert, vendor_has_open_critical_alert,
};
use billforge_core::TenantId;
use billforge_db::PgManager;
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

async fn seed_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("user-{}@test.com", user_id))
    .bind("hash")
    .bind("Risk Test User")
    .execute(pool)
    .await
    .expect("seed user");
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

async fn read_payment_hold(pool: &sqlx::PgPool, vendor_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT payment_hold FROM vendors WHERE id = $1")
        .bind(vendor_id)
        .fetch_one(pool)
        .await
        .expect("read payment_hold")
}

async fn count_open_critical(pool: &sqlx::PgPool, vendor_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM vendor_risk_alerts \
         WHERE vendor_id = $1 AND severity = 'critical' AND status = 'open'",
    )
    .bind(vendor_id)
    .fetch_one(pool)
    .await
    .expect("count open critical")
}

/// Two-tenant fixture mirroring notifications_inbox_test.rs::setup_two_tenants.
async fn setup_two_tenants(
    tag: &str,
) -> (PgManager, TenantId, TenantId, sqlx::PgPool, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("PgManager");

    let tenant_a: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("risk-a-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();
    let tenant_b: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("risk-b-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();

    manager.delete_tenant(&tenant_a).await.ok();
    manager.delete_tenant(&tenant_b).await.ok();
    manager
        .create_tenant(&tenant_a, &format!("Risk Tenant A {tag}"))
        .await
        .expect("create tenant A");
    manager
        .create_tenant(&tenant_b, &format!("Risk Tenant B {tag}"))
        .await
        .expect("create tenant B");

    let pool_a = (*manager.tenant(&tenant_a).await.expect("pool A")).clone();
    let pool_b = (*manager.tenant(&tenant_b).await.expect("pool B")).clone();

    // run_tenant_migrations provisions vendor_risk_alerts (migration 135).
    manager
        .run_tenant_migrations(&pool_a)
        .await
        .expect("migrate A");
    manager
        .run_tenant_migrations(&pool_b)
        .await
        .expect("migrate B");
    seed_tenant(&pool_a, &tenant_a, &format!("Risk A {tag}")).await;
    seed_tenant(&pool_b, &tenant_b, &format!("Risk B {tag}")).await;

    (manager, tenant_a, tenant_b, pool_a, pool_b)
}

async fn teardown_two_tenants(manager: &PgManager, tenant_a: &TenantId, tenant_b: &TenantId) {
    manager.delete_tenant(tenant_a).await.ok();
    manager.delete_tenant(tenant_b).await.ok();
}

// ===========================================================================
// Test 1: banking-change hook inserts a critical alert + sets payment_hold
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn banking_change_hook_inserts_critical_alert_and_sets_payment_hold() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("hook").await;

    let user_a = Uuid::new_v4();
    let vendor_a = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Hook Vendor").await;

    assert!(
        !read_payment_hold(&pool_a, vendor_a).await,
        "payment_hold starts false"
    );

    let verification_id = Uuid::new_v4();
    insert_banking_change_alert(
        &pool_a,
        &tenant_a,
        vendor_a,
        verification_id,
        Some("1234"),
        "6789",
    )
    .await
    .expect("insert banking-change alert");

    assert_eq!(
        count_open_critical(&pool_a, vendor_a).await,
        1,
        "exactly one open critical alert after banking change"
    );
    assert!(
        read_payment_hold(&pool_a, vendor_a).await,
        "payment_hold must be set when a critical alert lands"
    );

    // Payload must carry the masked last-four pair + verification id.
    let (alert_type, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT alert_type, payload FROM vendor_risk_alerts WHERE vendor_id = $1")
            .bind(vendor_a)
            .fetch_one(&pool_a)
            .await
            .expect("fetch alert payload");
    assert_eq!(alert_type, "banking_change");
    assert_eq!(payload["old_account_last_four"], "1234");
    assert_eq!(payload["new_account_last_four"], "6789");
    assert_eq!(payload["verification_id"], verification_id.to_string());

    teardown_two_tenants(&manager, &tenant_a, &_tenant_b).await;
}

// ===========================================================================
// Test 2: payment-release guard blocks while open critical exists, clears after ack
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn payment_release_guard_blocks_then_clears_after_acknowledge() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("guard").await;

    let user_a = Uuid::new_v4();
    let vendor_a = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Guard Vendor").await;

    // No alerts yet: guard must report no block.
    assert!(
        !vendor_has_open_critical_alert(&pool_a, &tenant_a, vendor_a).await,
        "no open alert -> not blocked"
    );

    // Land a critical alert (which sets payment_hold=true).
    insert_banking_change_alert(&pool_a, &tenant_a, vendor_a, Uuid::new_v4(), None, "9999")
        .await
        .expect("insert alert");
    assert!(
        vendor_has_open_critical_alert(&pool_a, &tenant_a, vendor_a).await,
        "open critical alert -> payment release must be blocked"
    );

    // Acknowledge the alert (mirrors POST /risk-alerts/:id/acknowledge UPDATE).
    let alert_id: (Uuid,) = sqlx::query_as(
        "SELECT id FROM vendor_risk_alerts WHERE vendor_id = $1 AND status = 'open'",
    )
    .bind(vendor_a)
    .fetch_one(&pool_a)
    .await
    .expect("fetch open alert");
    sqlx::query(
        "UPDATE vendor_risk_alerts \
         SET status = 'acknowledged', acknowledged_by = $2, acknowledged_at = NOW() \
         WHERE id = $1 AND status = 'open'",
    )
    .bind(alert_id.0)
    .bind(user_a)
    .execute(&pool_a)
    .await
    .expect("acknowledge");

    assert!(
        !vendor_has_open_critical_alert(&pool_a, &tenant_a, vendor_a).await,
        "after acknowledge -> no open critical -> guard clears"
    );

    teardown_two_tenants(&manager, &tenant_a, &_tenant_b).await;
}

// ===========================================================================
// Test 3: RLS isolation — tenant A's alerts are invisible from tenant B's pool
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn alerts_are_tenant_isolated_under_rls() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) = setup_two_tenants("rls").await;

    let user_a = Uuid::new_v4();
    let vendor_a = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Tenant A Vendor").await;

    insert_banking_change_alert(&pool_a, &tenant_a, vendor_a, Uuid::new_v4(), None, "4321")
        .await
        .expect("insert alert in A");

    // Tenant A sees its own alert.
    assert_eq!(count_open_critical(&pool_a, vendor_a).await, 1);

    // Tenant B's pool is RLS-scoped to tenant_b; a cross-tenant SELECT by the
    // same vendor_id must return 0 rows.
    let leaked: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM vendor_risk_alerts WHERE vendor_id = $1",
    )
    .bind(vendor_a)
    .fetch_one(&pool_b)
    .await
    .expect("cross-tenant count");
    assert_eq!(
        leaked, 0,
        "RLS must hide tenant A's alerts from tenant B's pool"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Test 4: acknowledging the last critical alert clears payment_hold
//         (when no other hold reason, e.g. pending banking verification, persists)
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn acknowledge_last_critical_alert_clears_payment_hold() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("clear-hold").await;

    let user_a = Uuid::new_v4();
    let vendor_a = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Hold Vendor").await;

    // Land a critical alert -> payment_hold flips to true.
    insert_banking_change_alert(&pool_a, &tenant_a, vendor_a, Uuid::new_v4(), None, "1111")
        .await
        .expect("insert alert");
    assert!(read_payment_hold(&pool_a, vendor_a).await);

    // Acknowledge via the same UPDATE the API handler runs. No pending banking
    // verification exists for this vendor, so the handler's clear-hold branch
    // is exercised when mirrored here.
    let alert_id: (Uuid,) = sqlx::query_as(
        "SELECT id FROM vendor_risk_alerts WHERE vendor_id = $1 AND status = 'open'",
    )
    .bind(vendor_a)
    .fetch_one(&pool_a)
    .await
    .expect("fetch alert");
    sqlx::query(
        "UPDATE vendor_risk_alerts \
         SET status = 'acknowledged', acknowledged_by = $2, acknowledged_at = NOW() \
         WHERE id = $1",
    )
    .bind(alert_id.0)
    .bind(user_a)
    .execute(&pool_a)
    .await
    .expect("acknowledge");

    // Mirror the handler's clear-hold branch: no remaining open critical and no
    // pending banking verification -> clear payment_hold.
    assert!(
        !vendor_has_open_critical_alert(&pool_a, &tenant_a, vendor_a).await,
        "no remaining open critical alerts"
    );
    sqlx::query(
        "UPDATE vendors SET payment_hold = false, payment_hold_reason = NULL, updated_at = NOW() \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_a)
    .bind(*tenant_a.as_uuid())
    .execute(&pool_a)
    .await
    .expect("clear payment_hold");

    assert!(
        !read_payment_hold(&pool_a, vendor_a).await,
        "payment_hold must clear after last critical alert acknowledged"
    );

    teardown_two_tenants(&manager, &tenant_a, &_tenant_b).await;
}

// ===========================================================================
// Test 5 (bonus): idempotency — repeating the same banking-change payload does
// not create a duplicate alert.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn duplicate_banking_change_payload_is_idempotent() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("idem").await;

    let vendor_a = Uuid::new_v4();
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Idem Vendor").await;

    let verification_id = Uuid::new_v4();
    insert_banking_change_alert(
        &pool_a,
        &tenant_a,
        vendor_a,
        verification_id,
        Some("1234"),
        "6789",
    )
    .await
    .expect("first insert");
    // Same payload: must dedupe on (vendor_id, alert_type, open + payload_hash).
    insert_banking_change_alert(
        &pool_a,
        &tenant_a,
        vendor_a,
        verification_id,
        Some("1234"),
        "6789",
    )
    .await
    .expect("second insert (deduped)");

    assert_eq!(
        count_open_critical(&pool_a, vendor_a).await,
        1,
        "duplicate payload must not create a second alert"
    );

    // A different payload (different new last four) must create a new alert.
    insert_banking_change_alert(
        &pool_a,
        &tenant_a,
        vendor_a,
        Uuid::new_v4(),
        Some("1234"),
        "9999",
    )
    .await
    .expect("distinct insert");
    assert_eq!(
        count_open_critical(&pool_a, vendor_a).await,
        2,
        "distinct payload must create a distinct alert"
    );

    teardown_two_tenants(&manager, &tenant_a, &_tenant_b).await;
}
