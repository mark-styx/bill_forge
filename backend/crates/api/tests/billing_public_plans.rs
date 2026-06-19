//! Integration test for the public pricing plans endpoint.
//!
//! Verifies the route wiring of GET /api/public/plans:
//! - Reachable WITHOUT authentication (returns 200, not 401).
//! - Body contains the public plans Free, Starter, Professional with their
//!   backend-truth prices.
//! - Body does NOT contain the Enterprise plan (is_public=false in
//!   `backend/crates/billing/src/plans.rs`).
//!
//! The data-layer contract (handler output) is also covered by a non-ignored
//! unit test in `src/routes/public_signup.rs`; this HTTP test additionally
//! proves the route is mounted unauthenticated and the response parses.

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
};
use billforge_api::{routes, AppState, Config};
use serde_json::Value;
use tower::util::ServiceExt;

fn set_common_env_vars() {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var(
            "DATABASE_URL",
            "postgres://postgres:postgres@localhost:5432/billforge_test",
        );
    }
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_pbp");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_pbp");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");
}

async fn create_test_router() -> axum::Router {
    set_common_env_vars();
    let config = Config::from_env().expect("Failed to load test config");
    let state = AppState::new(&config)
        .await
        .expect("Failed to create test state");
    routes::create_router(state)
}

#[tokio::test]
#[ignore = "requires live DATABASE_URL; run with --ignored when DB is available"]
async fn public_plans_endpoint_is_reachable_unauthenticated_and_excludes_enterprise() {
    let app = create_test_router().await;

    // No Authorization header - this must NOT be rejected by require_auth.
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/public/plans")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/public/plans must be reachable without authentication"
    );

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let body: Value = serde_json::from_slice(&bytes).expect("body is JSON");

    let arr = body
        .as_array()
        .expect("/api/public/plans returns a JSON array of plans");

    let ids: Vec<&str> = arr
        .iter()
        .map(|p| p["id"].as_str().expect("plan has id"))
        .collect();
    let names: Vec<&str> = arr
        .iter()
        .map(|p| p["name"].as_str().expect("plan has name"))
        .collect();

    // Public plans present.
    assert!(ids.contains(&"free"), "Free plan must be public");
    assert!(ids.contains(&"starter"), "Starter plan must be public");
    assert!(
        ids.contains(&"professional"),
        "Professional plan must be public"
    );

    // Enterprise (is_public=false) must never be returned.
    assert!(
        !ids.contains(&"enterprise"),
        "Enterprise plan id must not appear in public listing"
    );
    assert!(
        !names.contains(&"Enterprise"),
        "Enterprise plan name must not appear in public listing"
    );

    // Sanity-check that prices survive the wire (sourced from plans.rs, not
    // hardcoded on the client).
    let starter = arr
        .iter()
        .find(|p| p["id"] == "starter")
        .expect("starter present");
    assert_eq!(starter["monthly_price_cents"], 4900);

    let pro = arr
        .iter()
        .find(|p| p["id"] == "professional")
        .expect("professional present");
    assert_eq!(pro["monthly_price_cents"], 14900);
}
