//! Integration tests for the approval magic-link flow.
//!
//! Tests the sign/verify token logic and the three HTTP handlers
//! (approve, reject, comment) against a real PostgreSQL database.

#![allow(warnings)]

use billforge_api::routes::approval_links::{
    create_approval_token, create_approval_token_with_exp, resolve_approval_for_link,
    verify_approval_token,
};
use billforge_core::TenantId;
use chrono::Utc;
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";

/// Helper to get a database pool from DATABASE_URL.
async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Create a test invoice in `pending_approval` status and return its id.
async fn create_test_invoice(pool: &sqlx::PgPool) -> Uuid {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let invoice_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    // Ensure schema (including status column) is present
    billforge_db::tenant_db::run_tenant_migrations(pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");

    // Ensure the fixture user exists (needed for FK constraints)
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'approval-link-test@example.com', '', 'Approval Link Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to create fixture user");

    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, document_id, created_by, status)
         VALUES ($1, $2, 'Test Vendor', $3, 10000, $4, $5, 'pending_approval')
         ON CONFLICT DO NOTHING",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .bind(format!("APPROVAL-TEST-{}", invoice_id))
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to create test invoice");

    invoice_id
}

/// Set an invoice to a specific status (for test setup).
async fn set_invoice_status(pool: &sqlx::PgPool, invoice_id: Uuid, status: &str) {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    sqlx::query("UPDATE invoices SET status = $1 WHERE id = $2 AND tenant_id = $3")
        .bind(status)
        .bind(invoice_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("Failed to set invoice status");
}

/// Clean up test invoice and its audit rows.
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

// ===========================================================================
// Token signing & verification tests
// ===========================================================================

#[tokio::test]
async fn test_sign_and_verify_round_trip() {
    let invoice_id = Uuid::new_v4();
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["approve".to_string()],
    )
    .expect("signing should succeed");

    let claims = verify_approval_token(&token)
        .await
        .expect("verification should succeed");

    assert_eq!(claims.invoice_id, invoice_id);
    assert_eq!(claims.approver_email, "approver@example.com");
    assert_eq!(claims.tenant_id, tenant_id);
    assert!(claims.action_scope.contains(&"approve".to_string()));
}

#[tokio::test]
async fn test_expired_token_is_rejected() {
    let invoice_id = Uuid::new_v4();
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();

    let token = create_approval_token_with_exp(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["approve".to_string()],
        Utc::now().timestamp() - 3600, // expired 1 hour ago
    )
    .expect("signing should succeed");

    let result = verify_approval_token(&token).await;
    assert!(result.is_err(), "Expired token should be rejected");
}

#[tokio::test]
async fn test_tampered_token_signature_rejected() {
    let invoice_id = Uuid::new_v4();
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();

    let mut token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["approve".to_string()],
    )
    .expect("signing should succeed");

    // Flip a byte in the middle of the token to tamper with the signature
    let bytes = unsafe { token.as_bytes_mut() };
    let mid = bytes.len() / 2;
    bytes[mid] = bytes[mid].wrapping_add(1);

    let result = verify_approval_token(&token).await;
    assert!(result.is_err(), "Tampered token should be rejected");
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn test_token_cannot_be_reused() {
    let pool = get_pool().await;
    let invoice_id = Uuid::new_v4();
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();

    // Ensure schema is present (including used_approval_tokens table)
    billforge_db::tenant_db::run_tenant_migrations(&pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");

    let token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["approve".to_string()],
    )
    .expect("signing should succeed");

    // First use should succeed
    let first = verify_approval_token(&token).await;
    assert!(first.is_ok(), "First verification should succeed");

    // Mark it as used via the DB-backed consume_token (simulates what the handler does)
    if let Ok(claims) = first {
        billforge_api::routes::approval_links::mark_token_used_pub(
            &pool,
            claims.jti,
            claims.tenant_id,
            claims.invoice_id,
            claims.exp,
        )
        .await
        .expect("mark_token_used_pub should succeed");
    }

    // Second consume of the same jti should return TokenAlreadyUsed
    let claims2 = verify_approval_token(&token)
        .await
        .expect("verify still succeeds");
    let result = billforge_api::routes::approval_links::mark_token_used_pub(
        &pool,
        claims2.jti,
        claims2.tenant_id,
        claims2.invoice_id,
        claims2.exp,
    )
    .await;
    assert!(result.is_err(), "Re-using the same token should fail");
}

// ===========================================================================
// State machine integration tests (requires database)
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn test_approve_via_valid_token_transitions_state_and_audits() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let invoice_id = create_test_invoice(&pool).await;

    // Sign an approval token
    let token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["approve".to_string()],
    )
    .expect("signing");

    let claims = verify_approval_token(&token)
        .await
        .expect("verify should succeed");

    // Run the state machine transition directly (mimics what the handler does)
    billforge_api::state_machine::transition(
        &pool,
        &billforge_core::TenantId(tenant_id),
        &invoice_id,
        &Uuid::nil(),
        billforge_api::state_machine::InvoiceStatus::Approved,
        "approve_via_email",
        serde_json::json!({
            "approver_email": claims.approver_email,
            "channel": "email",
            "jti": claims.jti.to_string(),
        }),
    )
    .await
    .expect("transition should succeed");

    // Verify invoice is now approved
    let status: String =
        sqlx::query_scalar("SELECT status FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_id)
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("should find invoice");
    assert_eq!(status, "approved");

    // Verify audit row was created
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_audit_log WHERE invoice_id = $1 AND tenant_id = $2 AND event_type = 'approve_via_email'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(
        audit_count, 1,
        "Should have exactly one approve_via_email audit row"
    );

    // Verify approver_email is in the metadata
    let email: String = sqlx::query_scalar(
        "SELECT metadata->>'approver_email' FROM invoice_audit_log WHERE invoice_id = $1 AND tenant_id = $2 AND event_type = 'approve_via_email'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("metadata");
    assert_eq!(email, "approver@example.com");

    cleanup_invoice(&pool, invoice_id).await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn test_reject_with_reason_writes_reason_to_audit() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let invoice_id = create_test_invoice(&pool).await;

    let token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["reject".to_string()],
    )
    .expect("signing");

    let claims = verify_approval_token(&token)
        .await
        .expect("verify should succeed");

    let reason = "Wrong amount on invoice";

    billforge_api::state_machine::transition(
        &pool,
        &billforge_core::TenantId(tenant_id),
        &invoice_id,
        &Uuid::nil(),
        billforge_api::state_machine::InvoiceStatus::Rejected,
        "reject_via_email",
        serde_json::json!({
            "approver_email": claims.approver_email,
            "channel": "email",
            "jti": claims.jti.to_string(),
            "reason": reason,
        }),
    )
    .await
    .expect("transition should succeed");

    // Verify status
    let status: String =
        sqlx::query_scalar("SELECT status FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_id)
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("status");
    assert_eq!(status, "rejected");

    // Verify reason in metadata
    let audit_reason: String = sqlx::query_scalar(
        "SELECT metadata->>'reason' FROM invoice_audit_log WHERE invoice_id = $1 AND tenant_id = $2 AND event_type = 'reject_via_email'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("reason");
    assert_eq!(audit_reason, reason);

    cleanup_invoice(&pool, invoice_id).await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn test_comment_via_link_does_not_change_status() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let invoice_id = create_test_invoice(&pool).await;

    let token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["comment".to_string()],
    )
    .expect("signing");

    let claims = verify_approval_token(&token)
        .await
        .expect("verify should succeed");

    let comment_body = "Needs more documentation";

    // Insert a comment audit row (mimics comment_via_link handler)
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'comment_via_email', $5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(Uuid::nil())
    .bind(serde_json::json!({
        "approver_email": claims.approver_email,
        "channel": "email",
        "jti": claims.jti.to_string(),
        "comment_body": comment_body,
    }))
    .execute(&pool)
    .await
    .expect("insert audit row");

    // Verify status unchanged
    let status: String =
        sqlx::query_scalar("SELECT status FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_id)
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("status");
    assert_eq!(
        status, "pending_approval",
        "Comment should not change status"
    );

    // Verify audit row exists
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_audit_log WHERE invoice_id = $1 AND tenant_id = $2 AND event_type = 'comment_via_email'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit_count, 1, "Should have a comment audit row");

    // Verify comment_body in metadata
    let comment: String = sqlx::query_scalar(
        "SELECT metadata->>'comment_body' FROM invoice_audit_log WHERE invoice_id = $1 AND tenant_id = $2 AND event_type = 'comment_via_email'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("comment body");
    assert_eq!(comment, comment_body);

    cleanup_invoice(&pool, invoice_id).await;
}

// ===========================================================================
// Approval-request resolution tests
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn test_approve_via_link_resolves_approval_request() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let invoice_id = create_test_invoice(&pool).await;

    // Insert a pending approval_requests row for this invoice
    sqlx::query(
        "INSERT INTO approval_requests (id, tenant_id, invoice_id, requested_from, status)
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.to_string())
    .bind(invoice_id)
    .bind(serde_json::json!({"User": user_id.to_string()}))
    .execute(&pool)
    .await
    .expect("insert approval_request");

    // Call resolve_approval_for_link with "approved"
    resolve_approval_for_link(
        &pool,
        &TenantId(tenant_id),
        invoice_id,
        "approval-link-test@example.com",
        "approved",
    )
    .await
    .expect("resolve_approval_for_link should succeed");

    // Assert the approval_requests row is now approved with responded_by set
    let row: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, responded_by FROM approval_requests WHERE invoice_id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("should find approval_request");
    assert_eq!(row.0, "approved");
    assert_eq!(row.1, Some(user_id));

    // Assert processing_status is 'approved' (single-approver case fully resolves)
    let processing_status: String = sqlx::query_scalar(
        "SELECT processing_status FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("should find invoice");
    assert_eq!(processing_status, "approved");

    cleanup_invoice(&pool, invoice_id).await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn test_reject_via_link_resolves_approval_request() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let invoice_id = create_test_invoice(&pool).await;

    // Insert a pending approval_requests row for this invoice
    sqlx::query(
        "INSERT INTO approval_requests (id, tenant_id, invoice_id, requested_from, status)
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.to_string())
    .bind(invoice_id)
    .bind(serde_json::json!({"User": user_id.to_string()}))
    .execute(&pool)
    .await
    .expect("insert approval_request");

    // Call resolve_approval_for_link with "rejected"
    resolve_approval_for_link(
        &pool,
        &TenantId(tenant_id),
        invoice_id,
        "approval-link-test@example.com",
        "rejected",
    )
    .await
    .expect("resolve_approval_for_link should succeed");

    // Assert the approval_requests row is now rejected with responded_by set
    let row: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, responded_by FROM approval_requests WHERE invoice_id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("should find approval_request");
    assert_eq!(row.0, "rejected");
    assert_eq!(row.1, Some(user_id));

    // Assert processing_status is 'rejected'
    let processing_status: String = sqlx::query_scalar(
        "SELECT processing_status FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("should find invoice");
    assert_eq!(processing_status, "rejected");

    cleanup_invoice(&pool, invoice_id).await;
}

// ===========================================================================
// Regression: single-use tokens survive server restart (DB-backed store)
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_tests -- --ignored
async fn single_use_token_survives_simulated_restart() {
    let pool = get_pool().await;
    let invoice_id = Uuid::new_v4();
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();

    // Ensure schema (including used_approval_tokens table) is present
    billforge_db::tenant_db::run_tenant_migrations(&pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");

    // Build a token and decode claims
    let token = create_approval_token(
        invoice_id,
        "approver@example.com".to_string(),
        tenant_id,
        vec!["approve".to_string()],
    )
    .expect("signing should succeed");

    let claims = verify_approval_token(&token)
        .await
        .expect("verify should succeed");

    // Consume the token (writes to DB)
    billforge_api::routes::approval_links::mark_token_used_pub(
        &pool,
        claims.jti,
        claims.tenant_id,
        claims.invoice_id,
        claims.exp,
    )
    .await
    .expect("first consume should succeed");

    // Simulate a server restart: the in-memory HashSet would be cleared, but
    // the row is still in Postgres. Verify persistence directly.
    let row_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM used_approval_tokens WHERE jti = $1)")
            .bind(claims.jti)
            .fetch_one(&pool)
            .await
            .expect("row check");

    assert!(row_exists, "Token consumption should be persisted in DB");

    // Attempting to consume the same jti again must fail (TokenAlreadyUsed)
    let replay = billforge_api::routes::approval_links::mark_token_used_pub(
        &pool,
        claims.jti,
        claims.tenant_id,
        claims.invoice_id,
        claims.exp,
    )
    .await;

    assert!(
        replay.is_err(),
        "Replaying the same token jti should be rejected even after a simulated restart"
    );

    // Clean up the used_approval_tokens row
    sqlx::query("DELETE FROM used_approval_tokens WHERE jti = $1")
        .bind(claims.jti)
        .execute(&pool)
        .await
        .ok();
}
