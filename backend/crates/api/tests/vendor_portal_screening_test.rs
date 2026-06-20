//! Integration tests for vendor-portal invoice submission screening.
//!
//! Verifies that #393 is closed: vendor portal submissions enforce
//! `vendor.payment_hold` and run OFAC + fraud_guard screening before
//! creating invoice rows.
//!
//! Tests are `#[ignore]`d so they only run when a PostgreSQL DB and the
//! seeded sandbox tenant are available (matching neighbor vendor-portal tests).

use axum::{
    body::Body,
    http::{header, Method, Request},
};
use billforge_api::{routes, AppState, Config};
use billforge_auth::{JwtConfig, JwtService};
use billforge_core::domain::VendorId;
use billforge_core::TenantId;
use tower::util::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const JWT_SECRET: &str = "test-secret-key-for-testing-32-bytes";
const SANDBOX_TENANT: &str = "00000000-0000-0000-0000-000000000001";

async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", JWT_SECRET);
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var(
        "DATABASE_URL",
        "postgres://postgres@localhost:5432/billforge_test",
    );
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_vp_screen");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_vp_screen");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config)
        .await
        .expect("Failed to create test state")
}

fn vendor_portal_token(tenant_id: &TenantId, vendor_id: &VendorId) -> String {
    let svc = JwtService::new(JwtConfig {
        secret: JWT_SECRET.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    });
    svc.create_vendor_portal_token(tenant_id, vendor_id)
        .expect("create vendor portal token")
}

/// Insert a minimal vendor row directly so the test does not need to go
/// through the full vendor_repo create path (which would itself screen).
/// `payment_hold` defaults to false; pass a `reason` to flip it.
async fn seed_vendor(
    state: &AppState,
    tenant_id: &TenantId,
    name: &str,
    payment_hold: bool,
    payment_hold_reason: Option<&str>,
) -> VendorId {
    let pool = state.db.tenant(tenant_id).await.expect("tenant pool");
    let vendor_uuid = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors
            (id, tenant_id, name, vendor_type, status, payment_hold, payment_hold_reason, created_at, updated_at)
           VALUES ($1, $2, $3, 'business', 'active', $4, $5, NOW(), NOW())"#,
    )
    .bind(vendor_uuid)
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .bind(payment_hold)
    .bind(payment_hold_reason)
    .execute(&*pool)
    .await
    .expect("seed vendor row");
    VendorId(vendor_uuid)
}

async fn count_invoices_for_vendor(state: &AppState, tenant_id: &TenantId, vendor_id: &VendorId) -> i64 {
    let pool = state.db.tenant(tenant_id).await.expect("tenant pool");
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoices WHERE tenant_id = $1 AND vendor_id = $2",
    )
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id.0)
    .fetch_one(&*pool)
    .await
    .expect("count invoices");
    n
}

async fn vendor_is_on_hold(state: &AppState, tenant_id: &TenantId, vendor_id: &VendorId) -> (bool, Option<String>) {
    let pool = state.db.tenant(tenant_id).await.expect("tenant pool");
    let row: (bool, Option<String>) = sqlx::query_as(
        "SELECT payment_hold, payment_hold_reason FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id.0)
    .bind(*tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .expect("fetch vendor hold state");
    row
}

fn build_pdf_multipart(boundary: &str, invoice_number: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"file\"; filename=\"invoice.pdf\"\r\n".as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/pdf\r\n\r\n");
    body.extend_from_slice(b"%PDF-1.4\n%%EOF\n");
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
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn submit_invoice_rejects_when_vendor_on_payment_hold() {
    let state = create_test_state().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT).unwrap());
    let vendor_id = seed_vendor(
        &state,
        &tenant_id,
        "Held Vendor LLC",
        true,
        Some("manual hold for test"),
    )
    .await;

    let token = vendor_portal_token(&tenant_id, &vendor_id);
    let app = routes::create_router(state.clone());

    let body = serde_json::json!({
        "invoice_number": "INV-HOLD-001",
        "amount": 12345,
    });
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/invoices")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "payment_hold vendor must be rejected with 403"
    );
    assert_eq!(
        count_invoices_for_vendor(&state, &tenant_id, &vendor_id).await,
        0,
        "no invoice row should be created when submission is screened out"
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn upload_invoice_pdf_rejects_when_vendor_on_payment_hold() {
    let state = create_test_state().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT).unwrap());
    let vendor_id = seed_vendor(
        &state,
        &tenant_id,
        "Held PDF Vendor",
        true,
        Some("bank-change flagged"),
    )
    .await;

    let token = vendor_portal_token(&tenant_id, &vendor_id);
    let app = routes::create_router(state.clone());
    let boundary = "screenboundary123";
    let body = build_pdf_multipart(boundary, "INV-HOLD-PDF-001");

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
        axum::http::StatusCode::FORBIDDEN,
        "payment_hold vendor must be rejected on PDF upload too"
    );
    assert_eq!(
        count_invoices_for_vendor(&state, &tenant_id, &vendor_id).await,
        0,
        "no invoice row should be created when PDF upload is screened out"
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn submit_invoice_rejects_when_vendor_name_matches_ofac() {
    let state = create_test_state().await;
    let tenant_id = TenantId::from_uuid(Uuid::parse_str(SANDBOX_TENANT).unwrap());
    // "AL-QAEDA" is in the embedded SDN seed; OFAC screen returns "fail".
    let vendor_id = seed_vendor(&state, &tenant_id, "Al-Qaeda", false, None).await;

    let token = vendor_portal_token(&tenant_id, &vendor_id);
    let app = routes::create_router(state.clone());

    let body = serde_json::json!({
        "invoice_number": "INV-OFAC-001",
        "amount": 50000,
    });
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/vendor-portal/invoices")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("request failed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "vendor matching OFAC SDN must be rejected with 403"
    );
    assert_eq!(
        count_invoices_for_vendor(&state, &tenant_id, &vendor_id).await,
        0,
        "no invoice row should be created when OFAC blocks the submission"
    );

    let (hold, reason) = vendor_is_on_hold(&state, &tenant_id, &vendor_id).await;
    assert!(hold, "vendor should be flipped onto payment_hold by screening");
    assert!(
        reason.as_deref().unwrap_or("").contains("OFAC"),
        "payment_hold_reason should mention OFAC, got: {:?}",
        reason
    );
}
