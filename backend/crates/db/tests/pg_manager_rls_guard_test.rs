//! Integration tests for the fail-closed RLS guard in PgManager::new
//!
//! Proves that PgManager refuses to start when the connected database role
//! is a superuser or has BYPASSRLS, and accepts a restricted role such as
//! billforge_app (created by migration 120_force_rls_and_app_role.sql).
//!
//! Run in CI:
//!   cargo test -p billforge-db --test pg_manager_rls_guard_test --features integration
//!
//! Run locally (requires Postgres):
//!   cargo test -p billforge-db --test pg_manager_rls_guard_test -- --ignored

use billforge_db::PgManager;

const APP_ROLE: &str = "billforge_app";
const APP_ROLE_PASSWORD: &str = "billforge_app_dev"; // dev default from migration 120

/// Resolve the metadata database URL used by all tests in this file.
fn metadata_url() -> String {
    std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    })
}

/// Derive the billforge_app URL from the default (superuser) URL by swapping
/// the user:password segment.  Only works for the standard `postgres://user:pass@...` form.
fn app_role_url() -> String {
    let url = metadata_url();
    let Some(rest) = url.strip_prefix("postgres://") else {
        return url;
    };
    let Some((_, host_and_path)) = rest.split_once('@') else {
        return url;
    };
    format!("postgres://{APP_ROLE}:{APP_ROLE_PASSWORD}@{host_and_path}")
}

fn tenant_template() -> String {
    std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string())
}

// ===========================================================================
// Test 1: PgManager rejects a superuser role
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn pg_manager_rejects_superuser_role() {
    let result = PgManager::new(&metadata_url(), tenant_template()).await;

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("PgManager should reject a superuser connection"),
    };
    let err_msg = format!("{:#}", err).to_lowercase();
    assert!(
        err_msg.contains("superuser") || err_msg.contains("bypassrls"),
        "Error message should mention superuser or bypassrls, got: {}",
        err_msg
    );
}

// ===========================================================================
// Test 2: PgManager accepts the billforge_app restricted role
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn pg_manager_accepts_billforge_app_role() {
    let url = app_role_url();
    let template = tenant_template()
        .replace("postgres://postgres:postgres@", &format!("postgres://{APP_ROLE}:{APP_ROLE_PASSWORD}@"));

    let result = PgManager::new(&url, template).await;

    assert!(
        result.is_ok(),
        "PgManager should accept billforge_app, got error: {:#?}",
        result.as_ref().err()
    );
}
