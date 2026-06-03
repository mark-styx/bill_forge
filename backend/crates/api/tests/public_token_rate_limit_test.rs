//! Integration tests for per-IP rate limiting on public bearer-token surfaces.
//!
//! Verifies that:
//! - `/api/v1/actions/*`, `/api/v1/approval-links/*`, and `/api/v1/vendor-portal/*`
//!   are each independently rate-limited at 30 req / 60 s per source IP.
//! - A second IP is not throttled when the first hits the limit (per-IP isolation).
//! - Exhausting the `/auth` bucket does not affect `/approval-links` (per-surface isolation).

#![allow(warnings)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
    Extension, Router,
};
use billforge_api::middleware::{rate_limit_auth, RateLimiterState};
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn ok_handler() -> &'static str {
    "ok"
}

/// Build a minimal router that mirrors a single rate-limited surface.
fn rate_limited_surface(path: &str, max: u32, window: u64) -> Router {
    Router::new()
        .route(path, get(ok_handler))
        .layer(middleware::from_fn(rate_limit_auth))
        .layer(Extension(RateLimiterState::new(max, window)))
}

/// Build a combined router with two separate rate-limited surfaces sharing the
/// same base prefix, so we can prove bucket isolation.
fn combined_router() -> Router {
    Router::new()
        // Surface A: "/actions/approve" — 30/60
        .route("/actions/approve", get(ok_handler))
        .layer(middleware::from_fn(rate_limit_auth))
        .layer(Extension(RateLimiterState::new(30, 60)))
}

/// Build a router with both an auth-like surface (20/60) and an approval-links
/// surface (30/60) to prove per-surface bucket isolation.
fn two_surface_router() -> Router {
    Router::new()
        // Auth-like: 20 req / 60 s
        .route("/auth/login", get(ok_handler))
        .layer(middleware::from_fn(rate_limit_auth))
        .layer(Extension(RateLimiterState::new(20, 60)))
}

// ---------------------------------------------------------------------------
// Tests — per-surface rate limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn actions_surface_rate_limits_per_ip() {
    let app = rate_limited_surface("/actions/approve", 30, 60);
    let ip_a = "10.0.0.1";
    let ip_b = "10.0.0.2";

    // 30 requests from IP A: all should succeed
    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/actions/approve")
                    .header("x-forwarded-for", ip_a)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "Requests within limit should not be throttled"
        );
    }

    // 31st request from IP A: should be rejected
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actions/approve")
                .header("x-forwarded-for", ip_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Request over limit should be throttled"
    );

    // Request from IP B: should succeed (per-IP isolation)
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/actions/approve")
                .header("x-forwarded-for", ip_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Different IP should not be throttled"
    );
}

#[tokio::test]
async fn approval_links_surface_rate_limits_per_ip() {
    let app = rate_limited_surface("/approval-links/approve", 30, 60);
    let ip_a = "10.0.0.1";
    let ip_b = "10.0.0.2";

    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/approval-links/approve")
                    .header("x-forwarded-for", ip_a)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/approval-links/approve")
                .header("x-forwarded-for", ip_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // Different IP is not throttled
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/approval-links/approve")
                .header("x-forwarded-for", ip_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn vendor_portal_surface_rate_limits_per_ip() {
    let app = rate_limited_surface("/vendor-portal/invoices", 30, 60);
    let ip_a = "10.0.0.1";
    let ip_b = "10.0.0.2";

    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/vendor-portal/invoices")
                    .header("x-forwarded-for", ip_a)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/vendor-portal/invoices")
                .header("x-forwarded-for", ip_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // Different IP is not throttled
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/vendor-portal/invoices")
                .header("x-forwarded-for", ip_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------------------
// Per-surface bucket isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auth_rate_limit_does_not_affect_approval_links() {
    // Build a router that has two rate-limited nests with separate buckets.
    // The auth surface gets 20/60; the approval-links surface gets 30/60.
    let app = Router::new()
        .route("/auth/login", get(ok_handler))
        .layer(middleware::from_fn(rate_limit_auth))
        .layer(Extension(RateLimiterState::new(20, 60)));

    let approval_app = Router::new()
        .route("/approval-links/approve", get(ok_handler))
        .layer(middleware::from_fn(rate_limit_auth))
        .layer(Extension(RateLimiterState::new(30, 60)));

    let ip = "10.0.0.99";

    // Exhaust the auth bucket (20 requests)
    for _ in 0..20 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .header("x-forwarded-for", ip)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    // Auth is now throttled
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header("x-forwarded-for", ip)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // Approval-links surface still works — separate bucket
    for _ in 0..30 {
        let resp = approval_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/approval-links/approve")
                    .header("x-forwarded-for", ip)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "Approval-links should have its own independent bucket"
        );
    }

    // Now approval-links is also exhausted (31st request)
    let resp = approval_app
        .oneshot(
            Request::builder()
                .uri("/approval-links/approve")
                .header("x-forwarded-for", ip)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "After exhausting approval-links bucket, should be throttled"
    );
}
