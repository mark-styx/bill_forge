//! Unit tests for the OCR exception resolution workflow.
//!
//! The `resolve_ocr_exception` handler accepts `approve` or `reject` actions
//! and transitions `ocr_exception_status` from `pending` to the chosen state.
//! These tests exercise the validation predicate inline without a running database.
//!
//! Integration tests at the bottom of this file verify the full routing pipeline
//! (capture_status advancement, queue movement) against a live database.

use axum::{
    body::Body,
    http::{header, Method, Request},
};
use billforge_api::{routes, AppState, Config};
use serde_json::Value;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Unit tests (no database required)
// ---------------------------------------------------------------------------

/// Mirrors the validation in `resolve_ocr_exception`:
///
/// ```ignore
/// let action = body.action.to_lowercase();
/// if action != "approve" && action != "reject" {
///     return Err(...)
/// }
/// ```
fn is_valid_resolve_action(action: &str) -> bool {
    matches!(action.to_lowercase().as_str(), "approve" | "reject")
}

#[test]
fn resolve_accepts_approve() {
    assert!(
        is_valid_resolve_action("approve"),
        "'approve' should be a valid resolve action"
    );
}

#[test]
fn resolve_accepts_reject() {
    assert!(
        is_valid_resolve_action("reject"),
        "'reject' should be a valid resolve action"
    );
}

#[test]
fn resolve_rejects_invalid_action() {
    assert!(
        !is_valid_resolve_action("foobar"),
        "'foobar' should not be a valid resolve action"
    );
}

#[test]
fn resolve_rejects_empty_string() {
    assert!(
        !is_valid_resolve_action(""),
        "empty string should not be a valid resolve action"
    );
}

#[test]
fn resolve_is_case_insensitive() {
    assert!(is_valid_resolve_action("Approve"));
    assert!(is_valid_resolve_action("REJECT"));
    assert!(is_valid_resolve_action("ApPrOvE"));
}

/// Simulates the status transition: pending -> approved/rejected.
fn next_ocr_exception_status(current: &str, action: &str) -> Result<String, &'static str> {
    if current != "pending" {
        return Err("already resolved");
    }
    match action.to_lowercase().as_str() {
        "approve" => Ok("approved".to_string()),
        "reject" => Ok("rejected".to_string()),
        _ => Err("invalid action"),
    }
}

#[test]
fn pending_approve_transitions_to_approved() {
    let result = next_ocr_exception_status("pending", "approve").unwrap();
    assert_eq!(result, "approved");
}

#[test]
fn pending_reject_transitions_to_rejected() {
    let result = next_ocr_exception_status("pending", "reject").unwrap();
    assert_eq!(result, "rejected");
}

#[test]
fn already_approved_cannot_transition() {
    let result = next_ocr_exception_status("approved", "reject");
    assert!(result.is_err());
}

#[test]
fn already_rejected_cannot_transition() {
    let result = next_ocr_exception_status("rejected", "approve");
    assert!(result.is_err());
}

#[test]
fn invalid_action_returns_error() {
    let result = next_ocr_exception_status("pending", "delete");
    assert!(result.is_err());
}

/// Mirrors the routing-decision logic added to `resolve_ocr_exception`:
/// only `approve` triggers advancement; `reject` does not.
fn should_advance_invoice(action: &str) -> bool {
    action.to_lowercase().as_str() == "approve"
}

#[test]
fn approve_triggers_routing_advancement() {
    assert!(
        should_advance_invoice("approve"),
        "'approve' action should trigger routing advancement"
    );
}

#[test]
fn reject_does_not_trigger_routing() {
    assert!(
        !should_advance_invoice("reject"),
        "'reject' action should NOT trigger routing advancement"
    );
}

#[test]
fn routing_decision_is_case_insensitive() {
    assert!(should_advance_invoice("Approve"));
    assert!(should_advance_invoice("APPROVE"));
    assert!(!should_advance_invoice("Reject"));
}

// ---------------------------------------------------------------------------
// Integration helpers
// ---------------------------------------------------------------------------

async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var(
        "DATABASE_URL",
        "postgres://postgres@localhost:5432/billforge_test",
    );
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files");
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

/// Resolve an OCR exception via the API and return (status, body).
async fn resolve_ocr_exception(
    app: &axum::Router,
    token: &str,
    invoice_id: &str,
    action: &str,
) -> (axum::http::StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/invoices/{}/ocr-exception/resolve",
                    invoice_id
                ))
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({ "action": action })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Resolve request failed");

    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    (status, json)
}

/// Fetch an invoice by ID and return its JSON body.
async fn fetch_invoice(
    app: &axum::Router,
    token: &str,
    invoice_id: &str,
) -> (axum::http::StatusCode, Value) {
    let response = app
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

    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    (status, json)
}

/// Create a minimal 1x1 PNG image.
fn create_minimal_image() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
        0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
        0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

/// Upload a file and return the response JSON.
async fn upload_file(
    app: &axum::Router,
    token: &str,
    filename: &str,
    content_type: &str,
    data: &[u8],
) -> (axum::http::StatusCode, Value) {
    let boundary = "----TestBoundary12345";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{}\r\nContent-Disposition: form-data; name=\"file\"; \
             filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            boundary, filename, content_type
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
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

// ---------------------------------------------------------------------------
// Integration tests (require running PostgreSQL with seeded data)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_resolve_ocr_exception_advances_capture_status() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // Upload a minimal image that will trigger low-confidence OCR and land in
    // the Exception queue with capture_status = ready_for_review.
    let image_data = create_minimal_image();
    let (status, json) = upload_file(&app, &token, "exception.png", "image/png", &image_data).await;
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Upload should succeed, got {}",
        status
    );

    let invoice_id = json["invoice_id"]
        .as_str()
        .expect("Response should have invoice_id");

    // Fetch invoice to verify pre-conditions: it should be in an exception-like
    // state (capture_status = ready_for_review, current_queue_id set).
    let (_, invoice_before) = fetch_invoice(&app, &token, invoice_id).await;
    let capture_before = invoice_before["capture_status"].as_str().unwrap_or("");
    let queue_before = invoice_before["current_queue_id"].as_str().map(|s| s.to_string());

    // The invoice must be in a pre-submission state for routing to trigger.
    assert_eq!(
        capture_before, "ready_for_review",
        "Pre-condition: capture_status should be ready_for_review, got '{}'",
        capture_before
    );

    // Resolve the OCR exception with approve.
    let (resolve_status, resolve_body) =
        resolve_ocr_exception(&app, &token, invoice_id, "approve").await;
    assert!(
        resolve_status.is_success(),
        "Resolve should succeed, got {}: {:?}",
        resolve_status,
        resolve_body
    );
    assert_eq!(
        resolve_body["ocr_exception_status"].as_str().unwrap_or(""),
        "approved"
    );

    // Fetch invoice again and verify routing advancement.
    let (_, invoice_after) = fetch_invoice(&app, &token, invoice_id).await;

    // capture_status should have advanced (no longer ready_for_review).
    let capture_after = invoice_after["capture_status"].as_str().unwrap_or("");
    assert_ne!(
        capture_after, "ready_for_review",
        "After approve, capture_status should have advanced from ready_for_review, got '{}'",
        capture_after
    );

    // current_queue_id should no longer point to the Exception queue.
    let queue_after = invoice_after["current_queue_id"].as_str().map(|s| s.to_string());
    assert_ne!(
        queue_before, queue_after,
        "After approve, invoice should have moved off the Exception queue"
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_resolve_ocr_exception_rejected_does_not_route() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    let image_data = create_minimal_image();
    let (status, json) = upload_file(&app, &token, "reject_test.png", "image/png", &image_data).await;
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Upload should succeed, got {}",
        status
    );

    let invoice_id = json["invoice_id"]
        .as_str()
        .expect("Response should have invoice_id");

    // Fetch pre-resolution state.
    let (_, invoice_before) = fetch_invoice(&app, &token, invoice_id).await;
    let capture_before = invoice_before["capture_status"].as_str().unwrap_or("").to_string();
    let queue_before = invoice_before["current_queue_id"].as_str().map(|s| s.to_string());

    // Resolve with reject — should NOT trigger routing.
    let (resolve_status, resolve_body) =
        resolve_ocr_exception(&app, &token, invoice_id, "reject").await;
    assert!(
        resolve_status.is_success(),
        "Reject resolve should succeed, got {}: {:?}",
        resolve_status,
        resolve_body
    );
    assert_eq!(
        resolve_body["ocr_exception_status"].as_str().unwrap_or(""),
        "rejected"
    );

    // Verify capture_status and queue are unchanged.
    let (_, invoice_after) = fetch_invoice(&app, &token, invoice_id).await;
    let capture_after = invoice_after["capture_status"].as_str().unwrap_or("");
    assert_eq!(
        capture_after, capture_before,
        "Reject should NOT change capture_status"
    );

    let queue_after = invoice_after["current_queue_id"].as_str().map(|s| s.to_string());
    assert_eq!(
        queue_before, queue_after,
        "Reject should NOT move the invoice off its current queue"
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_resolve_ocr_exception_idempotent() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    let image_data = create_minimal_image();
    let (status, json) =
        upload_file(&app, &token, "idempotent_test.png", "image/png", &image_data).await;
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Upload should succeed, got {}",
        status
    );

    let invoice_id = json["invoice_id"]
        .as_str()
        .expect("Response should have invoice_id");

    // First resolve — should advance routing.
    let (resolve_status_1, _) =
        resolve_ocr_exception(&app, &token, invoice_id, "approve").await;
    assert!(resolve_status_1.is_success(), "First resolve should succeed");

    let (_, invoice_after_first) = fetch_invoice(&app, &token, invoice_id).await;
    let capture_after_first = invoice_after_first["capture_status"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let queue_after_first = invoice_after_first["current_queue_id"]
        .as_str()
        .map(|s| s.to_string());

    // Second resolve (duplicate) — should succeed without error and not
    // double-route. The handler is idempotent because capture_status is no
    // longer ready_for_review after the first resolve.
    let (resolve_status_2, _) =
        resolve_ocr_exception(&app, &token, invoice_id, "approve").await;
    assert!(
        resolve_status_2.is_success(),
        "Second resolve should succeed (idempotent), got {}",
        resolve_status_2
    );

    let (_, invoice_after_second) = fetch_invoice(&app, &token, invoice_id).await;
    let capture_after_second = invoice_after_second["capture_status"]
        .as_str()
        .unwrap_or("");
    let queue_after_second = invoice_after_second["current_queue_id"]
        .as_str()
        .map(|s| s.to_string());

    // State should be identical after the second resolve (no double-routing).
    assert_eq!(
        capture_after_second, capture_after_first,
        "Idempotent: capture_status should not change on second resolve"
    );
    assert_eq!(
        queue_after_second, queue_after_first,
        "Idempotent: queue should not change on second resolve"
    );
}
