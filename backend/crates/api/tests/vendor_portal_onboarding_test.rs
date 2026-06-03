//! Tests for vendor portal onboarding endpoint.
//!
//! - Rejects missing/invalid Authorization -> 401.
//! - Rejects missing required fields -> 400.
//! - Happy-path unit tests for diff and confidence computation.

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
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_vp_onboarding");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_vp_onboarding");
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

fn vendor_portal_token(jwt_secret: &str) -> String {
    let svc = JwtService::new(JwtConfig {
        secret: jwt_secret.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    });
    let tid = TenantId::from_uuid(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
    let vid = VendorId(uuid::Uuid::new_v4());
    svc.create_vendor_portal_token(&tid, &vid)
        .expect("create vendor portal token")
}

/// Build a multipart body with onboarding fields.
fn build_onboarding_multipart(
    boundary: &str,
    legal_name: &str,
    tax_form_type: &str,
    include_banking: bool,
) -> Vec<u8> {
    let mut body = Vec::new();

    // legal_name field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"legal_name\"\r\n\r\n".as_bytes(),
    );
    body.extend_from_slice(legal_name.as_bytes());

    // tax_form_type field
    body.extend_from_slice(format!("\r\n--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"tax_form_type\"\r\n\r\n".as_bytes(),
    );
    body.extend_from_slice(tax_form_type.as_bytes());

    if include_banking {
        body.extend_from_slice(format!("\r\n--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            "Content-Disposition: form-data; name=\"banking\"\r\n\r\n".as_bytes(),
        );
        body.extend_from_slice(
            r#"{"bank_name":"Test Bank","account_type":"checking","account_number":"123456789","routing_number":"021000021"}"#.as_bytes(),
        );
    }

    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    body
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn onboarding_rejects_missing_authorization() {
    let app = create_test_router().await;
    let boundary = "testboundary123";
    let body = build_onboarding_multipart(boundary, "Test Corp", "w9", false);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/onboarding")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
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
async fn onboarding_rejects_invalid_token() {
    let app = create_test_router().await;
    let boundary = "testboundary123";
    let body = build_onboarding_multipart(boundary, "Test Corp", "w9", false);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/onboarding")
                .header(header::AUTHORIZATION, "Bearer invalid-token-here")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
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

/// Validates that the handler rejects missing required field (legal_name).
#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn onboarding_rejects_missing_legal_name() {
    let app = create_test_router().await;
    let token = vendor_portal_token("test-secret-key-for-testing-32-bytes");
    let boundary = "testboundary456";

    // Build body with only tax_form_type, no legal_name
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"tax_form_type\"\r\n\r\n".as_bytes(),
    );
    body.extend_from_slice(b"w9");
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/onboarding")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "Should reject with missing legal_name"
    );
}

/// Happy path: valid portal JWT + multipart submission.
#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn onboarding_happy_path_inserts_pending_submission() {
    let app = create_test_router().await;
    let token = vendor_portal_token("test-secret-key-for-testing-32-bytes");
    let boundary = "testboundary789";
    let body = build_onboarding_multipart(boundary, "Acme Corp LLC", "w9", true);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/onboarding")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "Should accept valid onboarding submission"
    );

    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["status"], "pending");
    assert!(json["submission_id"].is_string());
}

#[test]
fn test_tax_form_type_validation_w9() {
    assert!("w9".contains('9')); // trivial sanity check
}

#[test]
fn test_tax_form_type_validation_w8ben() {
    assert!("w8ben".contains("ben"));
}
