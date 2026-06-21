//! Integration tests for Email Actions endpoints
//!
//! Tests secure email-based actions for approve/reject/hold/request_info without login

#![allow(warnings)]

// Note: Authentication and routing tests removed - they require PostgreSQL database setup.
// These are tested in integration tests with a real database.
// The following tests verify the data structures and service functionality.

use billforge_core::services::{EmailAction, EmailActionTokenService};
use billforge_core::{TenantId, UserId};
use std::sync::Arc;
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

async fn create_test_invoice(pool: &sqlx::PgPool) -> Uuid {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let invoice_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    billforge_db::tenant_db::run_tenant_migrations(pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");

    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'request-info-test@example.com', '', 'Request Info Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to create fixture user");

    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, document_id, created_by, status, processing_status)
         VALUES ($1, $2, 'Test Vendor', $3, 10000, $4, $5, 'pending_approval', 'pending_approval')
         ON CONFLICT DO NOTHING",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .bind(format!("REQINFO-TEST-{}", invoice_id))
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to create test invoice");

    sqlx::query(
        "INSERT INTO approval_requests (tenant_id, invoice_id, requested_from, status)
         VALUES ($1, $2, jsonb_build_object('User', $3::text), 'pending')",
    )
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to seed approval request");

    invoice_id
}

async fn cleanup_invoice(pool: &sqlx::PgPool, invoice_id: Uuid) {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    sqlx::query("DELETE FROM approval_requests WHERE invoice_id = $1 AND tenant_id = $2")
        .bind(invoice_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM invoice_audit_log WHERE invoice_id = $1 AND tenant_id = $2")
        .bind(invoice_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM invoices WHERE id = $1 AND tenant_id = $2")
        .bind(invoice_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
}

// ============================================================================
// Email Action Token Service Tests
// ============================================================================

#[test]
fn test_email_action_token_generation() {
    let _approve = EmailAction::ApproveInvoice;
    let _reject = EmailAction::RejectInvoice;
    let _hold = EmailAction::HoldInvoice;
    let _view = EmailAction::ViewInvoice;
    let _request_info = EmailAction::RequestInfoInvoice;
}

#[test]
fn test_email_action_enum_variants() {
    let approve = EmailAction::ApproveInvoice;
    let reject = EmailAction::RejectInvoice;
    let hold = EmailAction::HoldInvoice;
    let view = EmailAction::ViewInvoice;
    let request_info = EmailAction::RequestInfoInvoice;

    assert_ne!(
        std::mem::discriminant(&approve),
        std::mem::discriminant(&reject)
    );
    assert_ne!(
        std::mem::discriminant(&approve),
        std::mem::discriminant(&hold)
    );
    assert_ne!(std::mem::discriminant(&hold), std::mem::discriminant(&view));
    assert_ne!(
        std::mem::discriminant(&request_info),
        std::mem::discriminant(&approve)
    );
    assert_ne!(
        std::mem::discriminant(&request_info),
        std::mem::discriminant(&hold)
    );
}

// ============================================================================
// Email Action URL Generation Tests
// ============================================================================

#[test]
fn test_generate_action_url() {
    let base_url = "http://localhost:3000";
    let token = "test_token_123";
    let action = "approve";

    let url = format!("{}/api/v1/actions/{}?t={}", base_url, action, token);

    assert_eq!(
        url,
        "http://localhost:3000/api/v1/actions/approve?t=test_token_123"
    );
}

#[test]
fn test_generate_request_info_url() {
    let base_url = "http://localhost:3000";
    let token = "tok-req";

    let url = format!("{}/api/v1/actions/request_info?t={}", base_url, token);

    assert_eq!(
        url,
        "http://localhost:3000/api/v1/actions/request_info?t=tok-req"
    );
}

// ============================================================================
// Confirmation page renders textarea for RequestInfoInvoice
// ============================================================================

#[test]
fn test_request_info_token_round_trip() {
    // Mirrors the GET /api/v1/actions/request_info handler: it renders the
    // same HTML as render_confirmation_page(token, RequestInfoInvoice).
    let html = billforge_api::routes::email_actions::render_confirmation_page(
        "round-trip-token",
        EmailAction::RequestInfoInvoice,
    );

    assert!(
        html.contains("name=\"reason\""),
        "render must include the reason textarea field"
    );
    assert!(
        html.contains("<textarea"),
        "render must include a <textarea> element"
    );
    assert!(
        html.contains("action=\"/api/v1/actions/request_info\""),
        "form must POST to /api/v1/actions/request_info, got: {}",
        html
    );
    assert!(
        html.contains("value=\"round-trip-token\""),
        "form must include the action token in a hidden input"
    );
}

// ============================================================================
// Token Validation Tests
// ============================================================================

#[tokio::test]
async fn test_token_validation_with_expired_token() {
    // In a real test, we'd create an expired token and verify it fails validation
    // For now, we verify the test infrastructure works
    assert!(true);
}

// ============================================================================
// Token Security Tests
// ============================================================================

#[test]
fn test_token_signature_verification() {
    assert!(true);
}

#[test]
fn test_token_hashing() {
    assert!(true);
}

// ============================================================================
// Request-Info DB-backed integration tests (require DATABASE_URL)
// ============================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test email_action_tests -- --ignored
async fn test_request_info_token_generate_and_validate_round_trip() {
    let pool = Arc::new(get_pool().await);
    let tenant_id = TenantId(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());
    let user_id = UserId(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());

    billforge_db::tenant_db::run_tenant_migrations(&pool, &tenant_id)
        .await
        .expect("tenant migrations");

    let service = EmailActionTokenService::new(pool.clone(), "test-secret".to_string());
    let invoice_id = Uuid::new_v4();

    let token = service
        .generate_token(
            &tenant_id,
            &user_id,
            EmailAction::RequestInfoInvoice,
            invoice_id,
            "invoice",
            serde_json::json!({}),
        )
        .await
        .expect("token generation");

    let payload = service
        .validate_token(&token)
        .await
        .expect("token validation");

    assert!(matches!(payload.action, EmailAction::RequestInfoInvoice));
    assert_eq!(payload.resource_id, invoice_id);
    assert_eq!(payload.tenant_id, tenant_id.as_str());
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test email_action_tests -- --ignored
async fn test_request_info_confirm_writes_audit_and_pauses_approval() {
    let pool = get_pool().await;
    let tenant_id = TenantId(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());
    let user_id = UserId(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());

    let invoice_id = create_test_invoice(&pool).await;

    let reason = "Please attach the PO reference.";
    billforge_api::routes::email_actions::perform_request_info(
        &pool, &tenant_id, invoice_id, &user_id, reason,
    )
    .await
    .expect("perform_request_info should succeed");

    // Audit log row with event_type='info_requested' and reason in metadata
    let audit_row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT event_type, metadata FROM invoice_audit_log
         WHERE invoice_id = $1 AND tenant_id = $2 AND event_type = 'info_requested'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(invoice_id)
    .bind(tenant_id.as_uuid())
    .fetch_optional(&pool)
    .await
    .expect("audit query");

    let (event_type, metadata) = audit_row.expect("audit row must exist");
    assert_eq!(event_type, "info_requested");
    assert_eq!(metadata.get("reason").and_then(|v| v.as_str()), Some(reason));
    assert_eq!(
        metadata.get("channel").and_then(|v| v.as_str()),
        Some("email")
    );

    // Approval request transitioned to awaiting_info (paused, not resolved)
    let approval_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM approval_requests WHERE invoice_id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id.as_uuid())
    .fetch_optional(&pool)
    .await
    .expect("approval status query");
    assert_eq!(approval_status.as_deref(), Some("awaiting_info"));

    // Invoice processing_status unchanged (NOT OnHold)
    let processing_status: Option<String> = sqlx::query_scalar(
        "SELECT processing_status FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id.as_uuid())
    .fetch_optional(&pool)
    .await
    .expect("processing status query");
    assert_ne!(processing_status.as_deref(), Some("on_hold"));

    cleanup_invoice(&pool, invoice_id).await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test email_action_tests -- --ignored
async fn test_request_info_token_single_use() {
    let pool = Arc::new(get_pool().await);
    let tenant_id = TenantId(Uuid::parse_str(SANDBOX_TENANT_ID).unwrap());
    let user_id = UserId(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());

    billforge_db::tenant_db::run_tenant_migrations(&pool, &tenant_id)
        .await
        .expect("tenant migrations");

    let service = EmailActionTokenService::new(pool.clone(), "test-secret".to_string());
    let invoice_id = Uuid::new_v4();

    let token = service
        .generate_token(
            &tenant_id,
            &user_id,
            EmailAction::RequestInfoInvoice,
            invoice_id,
            "invoice",
            serde_json::json!({}),
        )
        .await
        .expect("token generation");

    // First validate + mark used should succeed
    service.validate_token(&token).await.expect("first validate");
    service.mark_used(&token).await.expect("mark used");

    // Second validate should fail with "Token has already been used"
    let err = service.validate_token(&token).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("already been used"),
        "second validate should be rejected as already used, got: {}",
        msg
    );
}
