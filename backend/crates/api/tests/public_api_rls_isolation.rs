//! RLS isolation for the shared metadata DB's public-API platform tables (#411).
//!
//! Migration 146 enables FORCE ROW LEVEL SECURITY on `api_keys`,
//! `webhook_subscriptions`, and `webhook_deliveries`. These tests assert the
//! database-level guarantee: with `app.current_tenant_id` bound to tenant A,
//!   1. a predicate-free `SELECT * FROM webhook_subscriptions` returns ONLY
//!      tenant A's row (proves the policy enforces tenant isolation
//!      independent of the handler's WHERE clause), and
//!   2. inserting a row whose tenant_id is tenant B is rejected by the
//!      policy's WITH CHECK clause.
//!
//! Run with a Postgres test DB available as DATABASE_URL:
//!   cargo test -p billforge-api --test public_api_rls_isolation -- --ignored

#![allow(warnings)]

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Get a pool against DATABASE_URL and apply migrations 110 + 146 (idempotent).
/// Returns None when DATABASE_URL is unset so the suite can run without a DB.
async fn get_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()?;
    sqlx::raw_sql(include_str!(
        "../../../migrations/110_create_public_api_platform.sql"
    ))
    .execute(&pool)
    .await
    .ok()?;
    sqlx::raw_sql(include_str!(
        "../../../migrations/146_enable_rls_metadata_public_api.sql"
    ))
    .execute(&pool)
    .await
    .ok()?;
    Some(pool)
}

/// Seed a tenant in the metadata DB (idempotent).
async fn seed_tenant(pool: &PgPool, tenant_id: Uuid, name: &str) {
    let slug = format!("rls-{}", tenant_id.simple());
    sqlx::query(
        r#"INSERT INTO tenants (id, name, slug, is_active, created_at)
           VALUES ($1, $2, $3, true, NOW())
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(tenant_id)
    .bind(name)
    .bind(&slug)
    .execute(pool)
    .await
    .expect("seed tenant");
}

/// Insert an api_key + webhook_subscription for `tenant_id`, binding the
/// tenant context inside a transaction so RLS permits the writes. Returns
/// the subscription id.
async fn seed_subscription(pool: &PgPool, tenant_id: Uuid, target_url: &str) -> Uuid {
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("bind tenant context");

    let key_id = Uuid::new_v4();
    let bearer = format!("bfg_pat_{}", Uuid::new_v4().to_string().replace('-', ""));
    let token_hash = hex::encode(Sha256::digest(bearer.as_bytes()));
    let token_prefix = &bearer[..8];

    sqlx::query(
        r#"INSERT INTO api_keys (id, tenant_id, name, token_hash, token_prefix, scopes, rate_limit_per_minute, created_at)
           VALUES ($1, $2, 'rls-test-key', $3, $4, $5, 60, NOW())"#,
    )
    .bind(key_id)
    .bind(tenant_id)
    .bind(&token_hash)
    .bind(token_prefix)
    .bind(vec!["webhooks:read".to_string()])
    .execute(&mut *tx)
    .await
    .expect("seed api_key");

    let sub_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO webhook_subscriptions (id, tenant_id, api_key_id, target_url, event_types, signing_secret, is_active, created_at)
           VALUES ($1, $2, $3, $4, $5, 'secret', true, NOW())"#,
    )
    .bind(sub_id)
    .bind(tenant_id)
    .bind(key_id)
    .bind(target_url)
    .bind(vec!["invoice.created".to_string()])
    .execute(&mut *tx)
    .await
    .expect("seed subscription");

    tx.commit().await.expect("commit");
    sub_id
}

// ===========================================================================
// Test 1: predicate-free read is filtered by RLS to current tenant only.
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL.
async fn webhook_subscriptions_rls_filters_predicate_free_read() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    seed_tenant(&pool, tenant_a, "RLS Tenant A").await;
    seed_tenant(&pool, tenant_b, "RLS Tenant B").await;

    let sub_a = seed_subscription(&pool, tenant_a, "https://rls-a.example.com/hook").await;
    let sub_b = seed_subscription(&pool, tenant_b, "https://rls-b.example.com/hook").await;

    // Bind tenant A context and run a query with NO tenant_id predicate.
    // RLS must filter the result to tenant A's row only.
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_a.to_string())
        .execute(&mut *tx)
        .await
        .expect("bind tenant A");

    let rows: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT id, tenant_id FROM webhook_subscriptions ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .expect("predicate-free read should succeed");

    tx.commit().await.ok();

    assert!(
        rows.iter().any(|(id, _)| *id == sub_a),
        "tenant A must see its own subscription via predicate-free read"
    );
    assert!(
        !rows.iter().any(|(id, _)| *id == sub_b),
        "tenant A must NOT see tenant B's subscription — RLS failed to filter"
    );
    assert!(
        rows.iter().all(|(_, t)| *t == tenant_a),
        "every returned row must belong to tenant A (RLS contract)"
    );

    // Cleanup
    let mut tx = pool.begin().await.expect("begin cleanup A");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_a.to_string())
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query("DELETE FROM webhook_subscriptions WHERE id = $1")
        .bind(sub_a)
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query("DELETE FROM api_keys WHERE tenant_id = $1")
        .bind(tenant_a)
        .execute(&mut *tx)
        .await
        .ok();
    tx.commit().await.ok();

    let mut tx = pool.begin().await.expect("begin cleanup B");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_b.to_string())
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query("DELETE FROM webhook_subscriptions WHERE id = $1")
        .bind(sub_b)
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query("DELETE FROM api_keys WHERE tenant_id = $1")
        .bind(tenant_b)
        .execute(&mut *tx)
        .await
        .ok();
    tx.commit().await.ok();
}

// ===========================================================================
// Test 2: INSERT for a foreign tenant_id is rejected by RLS WITH CHECK.
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL.
async fn webhook_subscriptions_rls_blocks_cross_tenant_insert() {
    let Some(pool) = get_pool().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    seed_tenant(&pool, tenant_a, "RLS Insert Tenant A").await;
    seed_tenant(&pool, tenant_b, "RLS Insert Tenant B").await;

    // Need an api_key in tenant B to satisfy the FK on the offending INSERT.
    let key_b = seed_subscription(&pool, tenant_b, "https://rls-b-fk.example.com/hook").await;
    let _ = key_b; // we only need the api_key row that seed_subscription created

    // Look up tenant B's api_key id (bind tenant B context to read it back).
    let mut tx = pool.begin().await.expect("begin lookup");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_b.to_string())
        .execute(&mut *tx)
        .await
        .expect("bind tenant B for lookup");
    let api_key_id: Uuid =
        sqlx::query_scalar("SELECT id FROM api_keys WHERE tenant_id = $1 LIMIT 1")
            .bind(tenant_b)
            .fetch_one(&mut *tx)
            .await
            .expect("api_key for tenant B");
    tx.commit().await.ok();

    // Bind tenant A context and attempt to INSERT a webhook_subscription
    // whose tenant_id is tenant B. RLS WITH CHECK must reject it.
    let mut tx = pool.begin().await.expect("begin attack");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_a.to_string())
        .execute(&mut *tx)
        .await
        .expect("bind tenant A");

    let result = sqlx::query(
        r#"INSERT INTO webhook_subscriptions
              (id, tenant_id, api_key_id, target_url, event_types, signing_secret, is_active, created_at)
           VALUES ($1, $2, $3, 'https://attacker.example.com/hook', $4, 'secret', true, NOW())"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_b) // foreign tenant_id
    .bind(api_key_id)
    .bind(vec!["invoice.created".to_string()])
    .execute(&mut *tx)
    .await;

    assert!(
        result.is_err(),
        "RLS WITH CHECK must reject INSERT into a foreign tenant's webhook_subscriptions \
         while app.current_tenant_id is bound to a different tenant — got Ok({:?})",
        result.ok().map(|r| r.rows_affected()),
    );

    let _ = tx.rollback().await;

    // Cleanup tenant B's seeded rows.
    let mut tx = pool.begin().await.expect("begin cleanup");
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_b.to_string())
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query("DELETE FROM webhook_subscriptions WHERE tenant_id = $1")
        .bind(tenant_b)
        .execute(&mut *tx)
        .await
        .ok();
    sqlx::query("DELETE FROM api_keys WHERE tenant_id = $1")
        .bind(tenant_b)
        .execute(&mut *tx)
        .await
        .ok();
    tx.commit().await.ok();
}
