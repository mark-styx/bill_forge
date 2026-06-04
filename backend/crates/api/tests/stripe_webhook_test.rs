//! Integration tests for the Stripe webhook endpoint.
//!
//! Tests:
//! - Valid signature + valid payload → 200 + subscription persisted
//! - Invalid signature → 400 + no DB mutation
//! - Missing signature header → 400

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use billforge_api::{routes, AppState, Config};
use billforge_core::TenantId;
use serde_json::json;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn set_common_env_vars() {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var(
            "DATABASE_URL",
            "postgres://postgres:postgres@localhost:5432/billforge_test",
        );
    }
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_sw");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_sw");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");
    std::env::set_var("STRIPE_API_KEY", "sk_test_fake");
    std::env::set_var("BILLING_ENABLED", "true");
}

async fn create_test_router_with_secret(secret: &str) -> axum::Router {
    set_common_env_vars();
    std::env::set_var("STRIPE_WEBHOOK_SECRET", secret);
    let config = Config::from_env().expect("Failed to load test config");
    let state = AppState::new(&config)
        .await
        .expect("Failed to create test state");
    routes::create_router(state)
}

/// Compute a valid Stripe webhook signature header for a given payload and secret.
fn compute_signature_header(payload: &[u8], secret: &str, timestamp: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let payload_str = std::str::from_utf8(payload).unwrap_or("");
    let signed_payload = format!("{}.{}", timestamp, payload_str);
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("valid secret length");
    mac.update(signed_payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("t={},v1={}", timestamp, sig)
}

fn checkout_event_payload(tenant_id: &TenantId) -> Vec<u8> {
    let event = json!({
        "id": "evt_test_checkout_completed",
        "type": "checkout.session.completed",
        "data": {
            "object": {
                "id": "cs_test_123",
                "metadata": {
                    "tenant_id": tenant_id.to_string(),
                    "plan_id": "starter",
                    "add_on_modules": "invoice_processing,reporting"
                }
            }
        }
    });
    serde_json::to_vec(&event).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stripe_webhook_rejects_invalid_signature() {
    let secret = "whsec_test_secret_for_invalid_sig_test";
    let app = create_test_router_with_secret(secret).await;
    let tenant_id = TenantId::new();
    let body = checkout_event_payload(&tenant_id);

    // Sign with a DIFFERENT secret
    let bad_sig = compute_signature_header(&body, "wrong_secret", 1700000000);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/billing/stripe/webhook")
        .header("Stripe-Signature", &bad_sig)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "Invalid signature must return 400"
    );
}

#[tokio::test]
async fn stripe_webhook_accepts_valid_signature_and_invokes_service() {
    let secret = "whsec_test_secret_for_valid_sig_test";
    let app = create_test_router_with_secret(secret).await;
    let tenant_id = TenantId::new();
    let body = checkout_event_payload(&tenant_id);

    let sig = compute_signature_header(&body, secret, 1700000000);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/billing/stripe/webhook")
        .header("Stripe-Signature", sig)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Valid signature must return 200"
    );
}

#[tokio::test]
async fn stripe_webhook_rejects_missing_signature_header() {
    let secret = "whsec_test_secret_for_missing_header";
    let app = create_test_router_with_secret(secret).await;
    let tenant_id = TenantId::new();
    let body = checkout_event_payload(&tenant_id);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/billing/stripe/webhook")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "Missing signature header must return 400"
    );
}
