//! Integration tests for vendor portal PDF upload endpoint.
//!
//! Tests:
//! - Rejects missing/invalid Authorization -> 401.
//! - Rejects non-PDF content_type -> 400 (requires running DB for token validation).

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
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_vp_pdf");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_vp_pdf");
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

/// Create a vendor portal token using the given JWT secret.
fn vendor_portal_token(jwt_secret: &str) -> String {
    let svc = JwtService::new(JwtConfig {
        secret: jwt_secret.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    });
    let tid =
        TenantId::from_uuid(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
    let vid = VendorId(uuid::Uuid::new_v4());
    svc.create_vendor_portal_token(&tid, &vid)
        .expect("create vendor portal token")
}

/// Build a multipart body with a `file` field and an `invoice_number` field.
fn build_pdf_multipart_body(boundary: &str, file_mime: &str, invoice_number: &str) -> Vec<u8> {
    let mut body = Vec::new();
    // File field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"file\"; filename=\"invoice.pdf\"\r\n"
            .to_string()
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", file_mime).as_bytes());
    // Minimal valid PDF bytes (header + empty body)
    body.extend_from_slice(b"%PDF-1.4\n%%EOF\n");
    // Invoice number field
    body.extend_from_slice(format!("\r\n--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"invoice_number\"\r\n\r\n".as_bytes(),
    );
    body.extend_from_slice(invoice_number.as_bytes());
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    body
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn upload_pdf_rejects_missing_authorization() {
    let app = create_test_router().await;
    let boundary = "testboundary123";
    let body = build_pdf_multipart_body(boundary, "application/pdf", "INV-PDF-001");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/invoices/upload")
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
#[ignore]
async fn upload_pdf_rejects_invalid_token() {
    let app = create_test_router().await;
    let boundary = "testboundary123";
    let body = build_pdf_multipart_body(boundary, "application/pdf", "INV-PDF-002");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/invoices/upload")
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

/// Validates that the handler rejects non-PDF MIME types with 400.
/// Requires a running PostgreSQL database and the seeded sandbox tenant
/// (vendor-portal JWT validation needs a DB-backed AuthService).
#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn upload_pdf_rejects_non_pdf_content_type() {
    let app = create_test_router().await;
    let token = vendor_portal_token("test-secret-key-for-testing-32-bytes");
    let boundary = "testboundary456";
    // Send an image/png instead of application/pdf
    let body = build_pdf_multipart_body(boundary, "image/png", "INV-PDF-003");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/invoices/upload")
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
        "Should reject non-PDF content type"
    );

    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["error"]["code"], "VALIDATION_ERROR");
}
