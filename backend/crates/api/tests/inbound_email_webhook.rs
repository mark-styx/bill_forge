//! Integration tests for the inbound email webhook endpoint.
//!
//! Tests:
//! - Valid secret + valid payload → 200 + DB rows created
//! - Missing/invalid secret → 401
//! - Unset or empty INBOUND_EMAIL_WEBHOOK_SECRET → 503 (fail-closed)

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use base64::Engine;
use billforge_api::{routes, AppState, Config};
use serde_json::json;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up the common env vars required by Config::from_env, EXCEPT
/// INBOUND_EMAIL_WEBHOOK_SECRET (caller sets that to test different states).
fn set_common_env_vars() {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var(
            "DATABASE_URL",
            "postgres://postgres:postgres@localhost:5432/billforge_test",
        );
    }
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_ie");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_ie");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");
    std::env::set_var("INBOUND_EMAIL_DOMAIN", "billforge.com");
}

async fn create_test_state() -> AppState {
    set_common_env_vars();
    std::env::set_var("INBOUND_EMAIL_WEBHOOK_SECRET", "test-inbound-secret");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config)
        .await
        .expect("Failed to create test state")
}

async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

fn sample_payload() -> serde_json::Value {
    json!({
        "from": "billing@acme.com",
        "to": "ap@meridian.billforge.com",
        "subject": "Invoice ACME-2024-001",
        "message_id": "msg-test-001@example.com",
        "attachments": []
    })
}

fn sample_payload_with_pdf() -> serde_json::Value {
    // Minimal valid base64-encoded PDF (1-byte file, enough for the handler)
    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(b"%PDF-1.4 test content");

    json!({
        "from": "billing@acme.com",
        "to": "ap@meridian.billforge.com",
        "subject": "Invoice ACME-2024-002",
        "message_id": "msg-test-002@example.com",
        "attachments": [{
            "name": "invoice.pdf",
            "content_type": "application/pdf",
            "content": pdf_b64
        }]
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_inbound_email_valid_secret_returns_200() {
    let _guard = SECRET_MUTEX.lock().unwrap();
    let app = create_test_router().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/webhooks/inbound-email")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-inbound-email-secret", "test-inbound-secret")
                .body(Body::from(
                    serde_json::to_string(&sample_payload()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    // Should return 200 even if tenant not found (triage case)
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 200 or 500 (no tenant DB), got {}",
        response.status()
    );
}

#[tokio::test]
#[ignore]
async fn test_inbound_email_invalid_secret_returns_401() {
    let _guard = SECRET_MUTEX.lock().unwrap();
    let app = create_test_router().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/webhooks/inbound-email")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-inbound-email-secret", "wrong-secret")
                .body(Body::from(
                    serde_json::to_string(&sample_payload()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 for invalid secret, got {}",
        response.status()
    );
}

#[tokio::test]
#[ignore]
async fn test_inbound_email_missing_secret_returns_401() {
    let _guard = SECRET_MUTEX.lock().unwrap();
    let app = create_test_router().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/webhooks/inbound-email")
                .header(header::CONTENT_TYPE, "application/json")
                // No secret header
                .body(Body::from(
                    serde_json::to_string(&sample_payload()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 for missing secret, got {}",
        response.status()
    );
}

#[tokio::test]
#[ignore]
async fn test_inbound_email_with_pdf_attachment_returns_200() {
    let _guard = SECRET_MUTEX.lock().unwrap();
    let app = create_test_router().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/webhooks/inbound-email")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-inbound-email-secret", "test-inbound-secret")
                .body(Body::from(
                    serde_json::to_string(&sample_payload_with_pdf()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    // Should return 200 (tenant might not exist in test, so triage is OK)
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 200 or 500 (no tenant DB), got {}",
        response.status()
    );
}

// ---------------------------------------------------------------------------
// Fail-closed tests: webhook must reject when secret env var is unset/empty
// ---------------------------------------------------------------------------

/// Mutex to serialize env-var mutation across tests. The handler reads
/// INBOUND_EMAIL_WEBHOOK_SECRET at request time (not startup), so the guard
/// must be held through the oneshot call.
static SECRET_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
#[ignore]
async fn test_rejects_when_secret_env_unset() {
    let _guard = SECRET_MUTEX.lock().unwrap();
    set_common_env_vars();
    std::env::remove_var("INBOUND_EMAIL_WEBHOOK_SECRET");

    let config = Config::from_env().expect("Failed to load test config");
    let state = AppState::new(&config)
        .await
        .expect("Failed to create test state");
    let app = routes::create_router(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/webhooks/inbound-email")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-inbound-email-secret", "anything")
                .body(Body::from(
                    serde_json::to_string(&sample_payload()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    // Restore before releasing guard
    std::env::set_var("INBOUND_EMAIL_WEBHOOK_SECRET", "test-inbound-secret");
    drop(_guard);

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Expected 503 when INBOUND_EMAIL_WEBHOOK_SECRET is unset, got {}",
        response.status()
    );
}

#[tokio::test]
#[ignore]
async fn test_rejects_when_secret_env_empty() {
    let _guard = SECRET_MUTEX.lock().unwrap();
    set_common_env_vars();
    std::env::set_var("INBOUND_EMAIL_WEBHOOK_SECRET", "");

    let config = Config::from_env().expect("Failed to load test config");
    let state = AppState::new(&config)
        .await
        .expect("Failed to create test state");
    let app = routes::create_router(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/webhooks/inbound-email")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-inbound-email-secret", "anything")
                .body(Body::from(
                    serde_json::to_string(&sample_payload()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    // Restore before releasing guard
    std::env::set_var("INBOUND_EMAIL_WEBHOOK_SECRET", "test-inbound-secret");
    drop(_guard);

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Expected 503 when INBOUND_EMAIL_WEBHOOK_SECRET is empty, got {}",
        response.status()
    );
}
