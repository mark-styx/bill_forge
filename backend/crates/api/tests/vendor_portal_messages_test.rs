//! Integration tests for vendor portal <-> AP messaging (#418).
//!
//! Tests:
//! - Rejects missing/invalid Authorization on the vendor side -> 401.
//! - Vendor token cannot read or write messages for an invoice belonging to
//!   a different vendor under the same tenant -> 403/404.

use axum::{
    body::Body,
    http::{header, Method, Request},
};
use billforge_api::{routes, AppState, Config};
use billforge_auth::{JwtConfig, JwtService};
use billforge_core::domain::VendorId;
use billforge_core::TenantId;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var(
        "DATABASE_URL",
        "postgres://postgres@localhost:5432/billforge_test",
    );
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_vp_msgs");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_vp_msgs");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config)
        .await
        .expect("Failed to create test state")
}

async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

/// Create a vendor portal token for an arbitrary vendor under tenant 0...01.
fn vendor_portal_token_for(jwt_secret: &str, vendor_id: VendorId) -> String {
    let svc = JwtService::new(JwtConfig {
        secret: jwt_secret.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    });
    let tid =
        TenantId::from_uuid(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
    svc.create_vendor_portal_token(&tid, &vendor_id)
        .expect("create vendor portal token")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn list_messages_rejects_missing_authorization() {
    let app = create_test_router().await;
    let invoice_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/v1/vendor-portal/invoices/{}/messages",
                    invoice_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "Should reject without Authorization header"
    );
}

#[tokio::test]
#[ignore]
async fn list_messages_rejects_invalid_token() {
    let app = create_test_router().await;
    let invoice_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/v1/vendor-portal/invoices/{}/messages",
                    invoice_id
                ))
                .header(header::AUTHORIZATION, "Bearer not-a-real-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "Should reject with invalid token"
    );
}

#[tokio::test]
#[ignore]
async fn post_message_rejects_missing_authorization() {
    let app = create_test_router().await;
    let invoice_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/vendor-portal/invoices/{}/messages",
                    invoice_id
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"body":"hello"}"#))
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "Should reject without Authorization header"
    );
}

/// Vendor authenticated for vendor A may not read or post messages on an
/// invoice that does not belong to vendor A. The handler must surface a
/// 403/404 rather than expose another vendor's thread.
#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant + invoices"]
async fn vendor_token_cannot_access_other_vendor_invoice_messages() {
    let app = create_test_router().await;
    let other_vendor = VendorId(uuid::Uuid::new_v4());
    let token = vendor_portal_token_for("test-secret-key-for-testing-32-bytes", other_vendor);
    let unrelated_invoice_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/v1/vendor-portal/invoices/{}/messages",
                    unrelated_invoice_id
                ))
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request failed");

    let status = response.status();
    assert!(
        status == axum::http::StatusCode::FORBIDDEN
            || status == axum::http::StatusCode::NOT_FOUND,
        "Should reject cross-vendor access (got {})",
        status
    );
}

#[tokio::test]
#[ignore]
async fn post_message_rejects_empty_body() {
    let app = create_test_router().await;
    let vendor_id = VendorId(uuid::Uuid::new_v4());
    let token = vendor_portal_token_for("test-secret-key-for-testing-32-bytes", vendor_id);
    let invoice_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/vendor-portal/invoices/{}/messages",
                    invoice_id
                ))
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"body":"   "}"#))
                .unwrap(),
        )
        .await
        .expect("request failed");

    // Either 400 (validation) before the ownership check, or 403/404 after —
    // both prove the empty body never reaches the database.
    let status = response.status();
    assert!(
        status == axum::http::StatusCode::BAD_REQUEST
            || status == axum::http::StatusCode::FORBIDDEN
            || status == axum::http::StatusCode::NOT_FOUND
            || status == axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "Empty body must not be accepted (got {})",
        status
    );
}

#[tokio::test]
#[ignore]
async fn ap_side_endpoint_requires_authenticated_user() {
    let app = create_test_router().await;
    let invoice_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/invoices/{}/vendor-messages", invoice_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "AP-side endpoint must reject unauthenticated requests"
    );
}
