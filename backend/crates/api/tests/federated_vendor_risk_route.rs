//! Integration tests for the Federated Vendor Risk Network (#408).
//!
//! Drives the `federated_risk` module against a real metadata Postgres so we
//! can verify:
//!   1. Aggregates with fewer than 5 distinct contributing tenants are
//!      suppressed (k-anonymity floor).
//!   2. Once the floor is met, the aggregate surfaces with the templated
//!      'why flagged' explanation that references only signal_type +
//!      contributor_count (no tenant identifiers).
//!   3. `contribute_signal` is a strict no-op for tenants that have not
//!      opted in.
//!
//! Run: `cargo test -p billforge-api --test federated_vendor_risk_route -- --ignored`

#![allow(warnings)]

use billforge_vendor_mgmt::federated_risk::{
    aggregate_for_vendor, contribute_signal, is_tenant_opted_in, opt_in_tenant,
    vendor_hash as compute_vendor_hash, FederatedSignalType, DEFAULT_K_ANONYMITY_FLOOR,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

const TEST_SALT: &str = "federated-risk-integration-salt";

async fn metadata_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect metadata pool")
}

/// Apply migration 141 against the metadata DB. The migration is
/// idempotent (CREATE TABLE IF NOT EXISTS) so it's safe to re-run.
async fn ensure_migration(pool: &PgPool) {
    let up = include_str!("../../../migrations/141_federated_vendor_risk_network.up.sql");
    sqlx::raw_sql(up)
        .execute(pool)
        .await
        .expect("apply migration 141");
}

/// Insert a `tenants` row so the foreign key on
/// `tenant_risk_network_consent` is satisfied. Idempotent.
async fn seed_tenant_row(pool: &PgPool, tenant_id: Uuid, name: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(name.to_lowercase().replace(' ', "-"))
    .execute(pool)
    .await
    .expect("seed tenant row");
}

/// Wipe federated_vendor_signals + consent rows for a fixed vendor_hash /
/// tenant set so reruns don't accumulate.
async fn cleanup(pool: &PgPool, vendor_hash: &str, tenants: &[Uuid]) {
    sqlx::query("DELETE FROM federated_vendor_signals WHERE vendor_hash = $1")
        .bind(vendor_hash)
        .execute(pool)
        .await
        .ok();
    for t in tenants {
        sqlx::query("DELETE FROM tenant_risk_network_consent WHERE tenant_id = $1")
            .bind(t)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(t)
            .execute(pool)
            .await
            .ok();
    }
}

#[tokio::test]
#[ignore = "requires Postgres metadata DB; run with --ignored"]
async fn k_anon_floor_suppresses_below_five_and_surfaces_at_five() {
    let pool = metadata_pool().await;
    ensure_migration(&pool).await;

    // Five unique tenant ids tied to this test's namespace so reruns are
    // deterministic across machines.
    let tenants: Vec<Uuid> = (0..5)
        .map(|i| Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("federated-k-anon-{i}").as_bytes()))
        .collect();

    let vendor_hash = compute_vendor_hash(
        "acme inc",
        Some("12-3456789"),
        Some("last4-9999"),
        TEST_SALT,
    );

    cleanup(&pool, &vendor_hash, &tenants).await;
    for (i, t) in tenants.iter().enumerate() {
        seed_tenant_row(&pool, *t, &format!("Federated K-Anon Tenant {i}")).await;
    }

    // Contribute from 4 tenants only - aggregate must be suppressed.
    for t in &tenants[..4] {
        opt_in_tenant(&pool, *t).await.expect("opt in");
        contribute_signal(
            &pool,
            *t,
            &vendor_hash,
            FederatedSignalType::OfacNearMatch,
            1.0,
            TEST_SALT,
        )
        .await
        .expect("contribute signal");
    }

    let aggregates = aggregate_for_vendor(&pool, &vendor_hash, DEFAULT_K_ANONYMITY_FLOOR)
        .await
        .expect("aggregate");
    assert!(
        aggregates.is_empty(),
        "below k-anonymity floor: must suppress all aggregates"
    );

    // Add the 5th contributor — aggregate now surfaces.
    opt_in_tenant(&pool, tenants[4]).await.expect("opt in 5");
    contribute_signal(
        &pool,
        tenants[4],
        &vendor_hash,
        FederatedSignalType::OfacNearMatch,
        1.0,
        TEST_SALT,
    )
    .await
    .expect("contribute 5");

    let aggregates = aggregate_for_vendor(&pool, &vendor_hash, DEFAULT_K_ANONYMITY_FLOOR)
        .await
        .expect("aggregate");
    assert_eq!(aggregates.len(), 1, "single signal_type, single row");
    let agg = &aggregates[0];
    assert_eq!(agg.signal_type, FederatedSignalType::OfacNearMatch);
    assert_eq!(agg.contributor_count, 5, "all 5 distinct contributors");
    assert!(agg.weighted_score >= 5.0);

    // Network-grounded explanation: must mention the count + signal label,
    // and must not leak any tenant identifier.
    let exp = agg.explanation.to_ascii_lowercase();
    assert!(exp.contains("5"), "explanation must include contributor count");
    assert!(
        exp.contains("ofac near-match"),
        "explanation must reference signal label"
    );
    for t in &tenants {
        assert!(
            !exp.contains(&t.to_string()),
            "explanation must NOT leak any tenant identifier"
        );
    }
    assert!(!exp.contains("tenant_id"));
    assert!(!exp.contains("vendor_id"));

    cleanup(&pool, &vendor_hash, &tenants).await;
}

#[tokio::test]
#[ignore = "requires Postgres metadata DB; run with --ignored"]
async fn contribute_signal_is_noop_when_tenant_not_opted_in() {
    let pool = metadata_pool().await;
    ensure_migration(&pool).await;

    let tenant = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"federated-noop");
    let vendor_hash = compute_vendor_hash("optout vendor", None, None, TEST_SALT);
    cleanup(&pool, &vendor_hash, std::slice::from_ref(&tenant)).await;
    seed_tenant_row(&pool, tenant, "Federated Noop Tenant").await;

    // Tenant has not opted in.
    assert!(!is_tenant_opted_in(&pool, tenant).await.unwrap());

    contribute_signal(
        &pool,
        tenant,
        &vendor_hash,
        FederatedSignalType::DisputeRateHigh,
        1.0,
        TEST_SALT,
    )
    .await
    .expect("contribute must succeed as a no-op");

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM federated_vendor_signals WHERE vendor_hash = $1",
    )
    .bind(&vendor_hash)
    .fetch_one(&pool)
    .await
    .expect("count signals");
    assert_eq!(
        count.0, 0,
        "contribute_signal must be a strict no-op without consent"
    );

    cleanup(&pool, &vendor_hash, std::slice::from_ref(&tenant)).await;
}
