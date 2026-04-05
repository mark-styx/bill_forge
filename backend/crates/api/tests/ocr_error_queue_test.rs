//! Integration tests for OCR error queue routing during invoice upload.
//!
//! Tests verify:
//! - OCR failures route invoices to the OcrError queue via WorkQueueRepository
//! - Missing OcrError queue causes graceful degradation (invoice still created)

use axum::{
    body::Body,
    http::{header, Method, Request},
};
use billforge_api::{routes, AppState, Config};
use serde_json::Value;
use tower::util::ServiceExt;
use uuid::Uuid;

/// Helper to create test app state with PostgreSQL
async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var("DATABASE_URL", "postgres://postgres@localhost:5432/billforge_test");
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config).await.expect("Failed to create test state")
}

/// Helper to create the test router
async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

/// Helper to get auth token for test user
async fn get_auth_token(app: &axum::Router) -> String {
    let login_body = serde_json::json!({
        "tenant_id": "00000000-0000-0000-0000-000000000001",
        "email": "admin@example.com",
        "password": "password123"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("Login failed");

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    json["token"].as_str().unwrap().to_string()
}

/// Create a minimal 1x1 PNG image that will trigger OCR failure (no readable text).
fn create_minimal_image() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xDE,
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54,
        0x08, 0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00,
        0x05, 0xFE, 0x02, 0xFE,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
        0xAE, 0x42, 0x60, 0x82,
    ]
}

/// Upload a file and return the response JSON.
async fn upload_file(app: &axum::Router, token: &str, filename: &str, content_type: &str, data: &[u8]) -> (axum::http::StatusCode, Value) {
    let boundary = "----TestBoundary12345";
    let mut body = Vec::new();
    body.extend_from_slice(format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
        boundary, filename, content_type
    ).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={}", boundary))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Upload request failed");

    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    (status, json)
}

// ============================================================================
// Test 1: OCR failure routes to error queue
// ============================================================================

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn ocr_failure_routes_to_error_queue() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // Upload a 1x1 pixel image - OCR will produce very low confidence (< 0.3)
    let image_data = create_minimal_image();
    let (status, json) = upload_file(&app, &token, "unocrable.png", "image/png", &image_data).await;

    // Upload should succeed
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Expected 200/201, got {}",
        status
    );

    let invoice_id = json["invoice_id"].as_str().expect("Response should have invoice_id");

    // The message should indicate OCR failure
    let message = json["message"].as_str().expect("Response should have message");
    assert!(
        message.to_lowercase().contains("ocr") || message.to_lowercase().contains("error"),
        "Message should indicate OCR failure, got: {}",
        message
    );

    // Fetch the invoice and verify capture_status is "failed"
    let invoice_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/invoices/{}", invoice_id))
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Failed to fetch invoice");

    assert!(invoice_response.status().is_success());

    let invoice_body = axum::body::to_bytes(invoice_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let invoice: Value = serde_json::from_slice(&invoice_body).unwrap();

    // Verify the capture status is "failed"
    let capture_status = invoice["capture_status"].as_str().unwrap_or("");
    assert_eq!(
        capture_status, "failed",
        "Low-confidence upload should have capture_status 'failed', got '{}'",
        capture_status
    );

    // Verify current_queue_id is set (pointing to the OCR error queue)
    let current_queue_id = invoice["current_queue_id"].as_str();
    assert!(
        current_queue_id.is_some(),
        "Invoice should have current_queue_id set after OCR failure routing"
    );
    let queue_id = current_queue_id.unwrap();
    assert!(
        Uuid::parse_str(queue_id).is_ok(),
        "current_queue_id should be a valid UUID, got: {}",
        queue_id
    );

    // The seeded OCR error queue ID should match
    let expected_error_queue_id = "11111111-4444-5555-6666-777777770001";
    assert_eq!(
        queue_id, expected_error_queue_id,
        "Invoice should be routed to the OCR error queue"
    );
}

// ============================================================================
// Test 2: OCR failure still creates invoice when queue is missing
// ============================================================================

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant with OCR error queue removed"]
async fn ocr_failure_logs_when_queue_missing() {
    // This test verifies graceful degradation: if the OcrError queue doesn't
    // exist for a tenant, the upload still succeeds and the invoice is created,
    // but it won't appear in any workflow queue. The tracing::error! log allows
    // operators to find these orphaned invoices.

    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // Upload a 1x1 pixel image - OCR will produce very low confidence
    let image_data = create_minimal_image();
    let (status, json) = upload_file(&app, &token, "unocrable_no_queue.png", "image/png", &image_data).await;

    // Upload should still succeed even without the error queue
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Expected 200/201 even without error queue, got {}",
        status
    );

    let invoice_id = json["invoice_id"].as_str().expect("Response should have invoice_id");

    // The message should still indicate OCR failure
    let message = json["message"].as_str().expect("Response should have message");
    assert!(
        message.to_lowercase().contains("ocr") || message.to_lowercase().contains("error") || message.to_lowercase().contains("failed"),
        "Message should indicate OCR failure, got: {}",
        message
    );

    // Fetch the invoice and verify it was created with failed capture status
    let invoice_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/invoices/{}", invoice_id))
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Failed to fetch invoice");

    assert!(invoice_response.status().is_success());

    let invoice_body = axum::body::to_bytes(invoice_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let invoice: Value = serde_json::from_slice(&invoice_body).unwrap();

    // Verify the invoice was created with failed status
    let capture_status = invoice["capture_status"].as_str().unwrap_or("");
    assert_eq!(
        capture_status, "failed",
        "Invoice should still have capture_status 'failed' even without error queue"
    );

    // Verify current_queue_id is NOT set (no queue to route to)
    let current_queue_id = invoice["current_queue_id"].as_str();
    assert!(
        current_queue_id.is_none() || current_queue_id.unwrap().is_empty(),
        "Invoice should not have current_queue_id when no OcrError queue exists"
    );
}
