//! Integration tests for the new compliance soft-hit kinds in the per-tenant
//! vendor-risk inbox surface (refs #441).
//!
//! Verifies that:
//!   1. The four new soft-hit alert kinds round-trip through the
//!      `vendor_risk_alerts` table without payment_hold being touched.
//!   2. Tenant A's alerts stay invisible from tenant B's RLS-scoped pool.
//!
//! Run: `cargo test -p billforge-api --test vendor_risk_alerts_expiry -- --ignored`

#![allow(warnings)]

use billforge_api::routes::vendor_risk_alerts::insert_alert;
use billforge_core::TenantId;
use billforge_db::PgManager;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers
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

async fn read_payment_hold(pool: &sqlx::PgPool, vendor_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT payment_hold FROM vendors WHERE id = $1")
        .bind(vendor_id)
        .fetch_one(pool)
        .await
        .expect("read payment_hold")
}

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

    let tenant_a: TenantId =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("expiry-a-{tag}").as_bytes())
            .to_string()
            .parse()
            .unwrap();
    let tenant_b: TenantId =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("expiry-b-{tag}").as_bytes())
            .to_string()
            .parse()
            .unwrap();

    manager.delete_tenant(&tenant_a).await.ok();
    manager.delete_tenant(&tenant_b).await.ok();
    manager
        .create_tenant(&tenant_a, &format!("Expiry A {tag}"))
        .await
        .expect("create A");
    manager
        .create_tenant(&tenant_b, &format!("Expiry B {tag}"))
        .await
        .expect("create B");

    let pool_a = (*manager.tenant(&tenant_a).await.expect("pool A")).clone();
    let pool_b = (*manager.tenant(&tenant_b).await.expect("pool B")).clone();
    manager.run_tenant_migrations(&pool_a).await.expect("migrate A");
    manager.run_tenant_migrations(&pool_b).await.expect("migrate B");
    seed_tenant(&pool_a, &tenant_a, &format!("Expiry A {tag}")).await;
    seed_tenant(&pool_b, &tenant_b, &format!("Expiry B {tag}")).await;

    (manager, tenant_a, tenant_b, pool_a, pool_b)
}

async fn teardown(manager: &PgManager, a: &TenantId, b: &TenantId) {
    manager.delete_tenant(a).await.ok();
    manager.delete_tenant(b).await.ok();
}

// ===========================================================================
// Test 1: Each new soft-hit alert kind round-trips and never sets payment_hold.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn new_alert_kinds_round_trip_without_payment_hold() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("kinds").await;

    let vendor_a = Uuid::new_v4();
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Compliance Vendor A").await;

    // The four soft-hit kinds with their expected severities (per #441 contract).
    let cases: &[(&str, &str)] = &[
        ("w9_expiring", "medium"),
        ("w9_expired", "medium"),
        ("coi_expired", "high"),
        ("threshold_1099_no_w9", "high"),
    ];

    for (kind, sev) in cases {
        insert_alert(
            &pool_a,
            &tenant_a,
            vendor_a,
            kind,
            sev,
            serde_json::json!({"kind": kind}),
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("insert {kind}: {e}"));
    }

    // All four rows are stored and selectable under the tenant pool.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM vendor_risk_alerts WHERE vendor_id = $1 AND status = 'open'",
    )
    .bind(vendor_a)
    .fetch_one(&pool_a)
    .await
    .expect("count");
    assert_eq!(count, cases.len() as i64);

    // Critical contract: none of the new kinds touches payment_hold. Only the
    // sanctions/banking critical path flips it. insert_alert only flips
    // payment_hold when severity = "critical".
    assert!(
        !read_payment_hold(&pool_a, vendor_a).await,
        "no soft-hit kind may set payment_hold"
    );

    // The kind + severity columns serialize back as written.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT alert_type, severity FROM vendor_risk_alerts \
         WHERE vendor_id = $1 ORDER BY alert_type",
    )
    .bind(vendor_a)
    .fetch_all(&pool_a)
    .await
    .expect("fetch rows");
    let mut expected: Vec<(String, String)> = cases
        .iter()
        .map(|(k, s)| (k.to_string(), s.to_string()))
        .collect();
    expected.sort();
    let mut actual = rows.clone();
    actual.sort();
    assert_eq!(actual, expected);

    teardown(&manager, &tenant_a, &_tenant_b).await;
}

// ===========================================================================
// Test 2: Tenant isolation — tenant A's compliance alerts are invisible from
// tenant B's RLS-scoped pool.
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn compliance_alerts_are_tenant_isolated_under_rls() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) = setup_two_tenants("rls").await;

    let vendor_a = Uuid::new_v4();
    seed_vendor(&pool_a, &tenant_a, vendor_a, "Tenant A Vendor").await;
    insert_alert(
        &pool_a,
        &tenant_a,
        vendor_a,
        "w9_expiring",
        "medium",
        serde_json::json!({"expires_on": "2026-12-31", "days_until_expiry": 15}),
        None,
    )
    .await
    .expect("insert tenant A alert");

    // Seed an unrelated alert under tenant B as a control.
    let vendor_b = Uuid::new_v4();
    seed_vendor(&pool_b, &tenant_b, vendor_b, "Tenant B Vendor").await;
    insert_alert(
        &pool_b,
        &tenant_b,
        vendor_b,
        "coi_expired",
        "high",
        serde_json::json!({"expires_on": "2026-01-01", "days_overdue": 5}),
        None,
    )
    .await
    .expect("insert tenant B alert");

    // Tenant A only sees its own row.
    let a_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM vendor_risk_alerts")
        .fetch_one(&pool_a)
        .await
        .expect("count A");
    assert_eq!(a_count, 1, "tenant A's pool sees exactly one alert");

    // RLS hides A's row from B's pool when scoped by vendor_id.
    let leaked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM vendor_risk_alerts WHERE vendor_id = $1",
    )
    .bind(vendor_a)
    .fetch_one(&pool_b)
    .await
    .expect("cross-tenant count");
    assert_eq!(leaked, 0, "RLS must hide tenant A's compliance alerts from tenant B");

    teardown(&manager, &tenant_a, &tenant_b).await;
}
