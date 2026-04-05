//! Integration tests for the require_auth middleware.
//!
//! Verifies that the gatekeeper middleware correctly rejects unauthenticated
//! requests to protected endpoints while allowing public paths through.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use tower::util::ServiceExt;

use billforge_api::middleware::require_auth;

/// A simple handler that always returns 200 OK.
async fn ok_handler() -> &'static str {
    "ok"
}

/// Build a minimal test router that mirrors the structure of the real API:
/// public paths are registered, and the `require_auth` middleware is applied.
fn build_test_router() -> Router {
    Router::new()
        // Public paths
        .route("/api/v1/auth/login", post(ok_handler))
        .route("/api/v1/auth/register", post(ok_handler))
        .route("/api/v1/auth/provision", post(ok_handler))
        .route("/api/v1/auth/refresh", post(ok_handler))
        .route("/api/v1/actions/:token", post(ok_handler))
        .route("/api/v1/edi/webhook/test", post(ok_handler))
        .route("/api/v1/billing/plans", get(ok_handler))
        // Protected path
        .route("/api/v1/invoices", get(ok_handler))
        .layer(middleware::from_fn(require_auth))
}

#[tokio::test]
async fn unauthenticated_api_request_returns_401() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Protected endpoint should reject unauthenticated requests with 401"
    );
}

#[tokio::test]
async fn public_paths_allow_unauthenticated() {
    // Test each public path - they should NOT return 401 from the middleware
    let public_paths = vec![
        ("/api/v1/auth/login", "POST"),
        ("/api/v1/auth/register", "POST"),
        ("/api/v1/auth/provision", "POST"),
        ("/api/v1/auth/refresh", "POST"),
        ("/api/v1/actions/some-token", "POST"),
        ("/api/v1/edi/webhook/test", "POST"),
        ("/api/v1/billing/plans", "GET"),
    ];

    for (path, method) in public_paths {
        let app = build_test_router();
        let builder = Request::builder().uri(path).method(method);
        let request = builder.body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Public path {} should NOT be blocked by auth middleware",
            path
        );
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Public path {} should pass through to handler",
            path
        );
    }
}

#[tokio::test]
async fn auth_header_present_passes_middleware() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", "Bearer some-token-value")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The middleware should let this through (200 from handler, not 401 from middleware).
    // Token validation is the extractor's job, not the middleware's.
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Request with Bearer header should pass the middleware gatekeeper"
    );
}

#[tokio::test]
async fn malformed_auth_header_still_rejected() {
    let app = build_test_router();

    // "Basic ..." is not "Bearer ..." so the middleware should reject it
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Non-Bearer auth scheme should be rejected by middleware"
    );
}
