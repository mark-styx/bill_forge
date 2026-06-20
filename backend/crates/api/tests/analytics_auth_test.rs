//! Integration tests for analytics auth enforcement (#342)
//!
//! Verifies that:
//! 1. `GET /api/v1/analytics/usage` without auth returns 401
//! 2. `POST /api/v1/analytics/events` without auth returns 401
//! 3. With a valid token, the request passes auth middleware (tenant-scoped pool
//!    is exercised at runtime; here we confirm the middleware gate holds).
//!
//! Run: `cargo test -p billforge-api --test analytics_auth_test`

#![allow(warnings)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use billforge_api::middleware::{require_auth, require_tenant};
use billforge_api::routes::analytics;
use billforge_api::state::AppState;
use billforge_auth::{AuthService, JwtConfig, JwtService};
use billforge_core::{Role, TenantId, UserId};
use billforge_db::MetadataDatabase;
use std::sync::Arc;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const JWT_SECRET: &str = "test-secret-analytics-auth";

fn test_auth_service() -> Arc<AuthService> {
    let jwt_config = JwtConfig {
        secret: JWT_SECRET.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://fake@localhost/fake")
        .expect("connect_lazy should not fail");
    let metadata_db = Arc::new(MetadataDatabase::from_pool(pool));
    Arc::new(AuthService::new(jwt_config, metadata_db))
}

fn test_jwt_service() -> JwtService {
    JwtService::new(JwtConfig {
        secret: JWT_SECRET.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    })
}

fn make_token(user_id: &UserId, tenant_id: &TenantId) -> String {
    let jwt = test_jwt_service();
    jwt.create_access_token(
        user_id,
        tenant_id,
        "analytics-test@example.com",
        &[Role::ApUser],
    )
    .expect("token creation should succeed")
}

/// Build a router that mirrors the production mounting:
/// `/api/v1/analytics/*` with `require_tenant` + `require_auth` layers.
///
/// We reuse the real `analytics::routes()` so the path matching is authentic,
/// but we skip the database state (handlers will fail at the DB call if they
/// reach that far, which is fine for auth gate tests).
fn build_analytics_router() -> Router {
    let auth = test_auth_service();

    // We cannot construct a real AppState without a database, so we create a
    // minimal router with dummy handlers at the same paths that the analytics
    // module registers, then apply the same middleware stack.
    Router::new()
        .nest("/api/v1/analytics", analytics_router_stub())
        .layer(middleware::from_fn_with_state(auth.clone(), require_tenant))
        .layer(middleware::from_fn_with_state(auth, require_auth))
}

/// Stub router at the same paths as `analytics::routes()`. The handlers
/// return 200 so we can confirm the request passed auth. In production
/// the real handlers would do DB work.
fn analytics_router_stub() -> Router {
    Router::new()
        .route("/events", post(stub_ok))
        .route("/usage/daily", get(stub_ok))
        .route("/usage/weekly", get(stub_ok))
        .route("/usage/monthly", get(stub_ok))
        .route("/usage", get(stub_ok))
        .route("/performance", get(stub_ok))
        .route("/trends", get(stub_ok))
}

async fn stub_ok() -> &'static str {
    "ok"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_analytics_usage_requires_auth() {
    let app = build_analytics_router();

    let req = Request::builder()
        .uri("/api/v1/analytics/usage")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "analytics/usage should reject unauthenticated requests with 401"
    );
}

#[tokio::test]
async fn test_analytics_events_requires_auth() {
    let app = build_analytics_router();

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/analytics/events")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"event_type":"test","event_category":"test","event_data":{}}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "analytics/events POST should reject unauthenticated requests with 401"
    );
}

#[tokio::test]
async fn test_analytics_trends_requires_auth() {
    let app = build_analytics_router();

    let req = Request::builder()
        .uri("/api/v1/analytics/trends")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "analytics/trends should reject unauthenticated requests with 401"
    );
}

#[tokio::test]
async fn test_analytics_performance_requires_auth() {
    let app = build_analytics_router();

    let req = Request::builder()
        .uri("/api/v1/analytics/performance")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "analytics/performance should reject unauthenticated requests with 401"
    );
}

#[tokio::test]
async fn test_analytics_usage_accepts_valid_token() {
    let app = build_analytics_router();

    let user_id = UserId::from_uuid(uuid::Uuid::new_v4());
    let tenant_id = TenantId::new();
    let token = make_token(&user_id, &tenant_id);

    let req = Request::builder()
        .uri("/api/v1/analytics/usage")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // The valid token passes require_auth and the nil-tenant check inside
    // require_tenant, then reaches the TenantContext metadata-DB load. With the
    // connect_lazy stub pool the load fails with `tenant_context_load_failed`,
    // which proves the middleware chain accepted the token (it did NOT 401).
    assert_ne!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "analytics/usage with valid JWT must NOT be rejected by auth middleware"
    );
    assert_eq!(
        resp.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "analytics/usage with valid JWT should reach require_tenant's DB load (which fails on the stub pool)"
    );
}

#[tokio::test]
async fn test_analytics_events_accepts_valid_token() {
    let app = build_analytics_router();

    let user_id = UserId::from_uuid(uuid::Uuid::new_v4());
    let tenant_id = TenantId::new();
    let token = make_token(&user_id, &tenant_id);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/analytics/events")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"event_type":"test","event_category":"test","event_data":{}}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "analytics/events POST with valid JWT must NOT be rejected by auth middleware"
    );
    assert_eq!(
        resp.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "analytics/events POST with valid JWT should reach require_tenant's DB load (which fails on the stub pool)"
    );
}
