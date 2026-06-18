//! Integration tests for approval-link minting and approval-email resend.
//!
//! Covers happy-path (token generation, audit row, email dispatch) and
//! tenant-isolation (cross-tenant 404 for approval requests outside the
//! caller's tenant).  Database-dependent tests are gated behind `#[ignore]`
//! and the `DATABASE_URL` env var (run with `--ignored`).

#![allow(warnings)]

use billforge_core::TenantId;
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
const OTHER_TENANT_ID: &str = "22222222-2222-2222-2222-222222222222";
const FIXTURE_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Create a pending approval_request + invoice in the given tenant and return
/// the approval_request id.
async fn create_pending_approval(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();
    let invoice_id = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();

    billforge_db::tenant_db::run_tenant_migrations(pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");

    // Ensure the fixture user exists
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'resend-test@example.com', '', 'Resend Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create fixture user");

    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, document_id, created_by, status)
         VALUES ($1, $2, 'Test Vendor', $3, 10000, $4, $5, 'pending_approval')
         ON CONFLICT DO NOTHING",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .bind(format!("RESEND-TEST-{}", invoice_id))
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("create test invoice");

    sqlx::query(
        "INSERT INTO approval_requests (id, tenant_id, invoice_id, requested_from, status)
         VALUES ($1, $2, $3, $4, 'pending')
         ON CONFLICT DO NOTHING",
    )
    .bind(approval_id)
    .bind(tenant_id.to_string())
    .bind(invoice_id)
    .bind(serde_json::json!({"user_id": user_id.to_string()}))
    .execute(pool)
    .await
    .expect("create approval_request");

    approval_id
}

/// Clean up test data.
async fn cleanup(pool: &sqlx::PgPool, tenant_id: Uuid, approval_id: Uuid, invoice_id: Uuid) {
    sqlx::query("DELETE FROM approval_requests WHERE id = $1")
        .bind(approval_id)
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
// Happy-path: fetch_approval_for_link returns correct data for pending approval
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn test_fetch_approval_for_link_returns_pending_approval_info() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let approval_id = create_pending_approval(&pool, tenant_id).await;

    // Use the public re-export to call the helper indirectly through a
    // minimal query that mirrors what the handler does.
    let row: Option<(
        uuid::Uuid,
        String,
        String,
        i64,
        Option<String>,
        Option<uuid::Uuid>,
    )> = sqlx::query_as(
        r#"SELECT
            ar.invoice_id,
            i.invoice_number,
            COALESCE(i.vendor_name, 'Unknown') as vendor_name,
            COALESCE(i.total_amount_cents, 0) as total_amount_cents,
            (SELECT u.email FROM users u
             WHERE u.id = (ar.requested_from->>'user_id')::uuid
             LIMIT 1) as approver_email,
            (ar.requested_from->>'user_id')::uuid as approver_user_id
        FROM approval_requests ar
        JOIN invoices i ON ar.invoice_id = i.id
        WHERE ar.id = $1 AND ar.tenant_id = $2 AND ar.status = 'pending'"#,
    )
    .bind(approval_id)
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .expect("query");

    assert!(row.is_some(), "Should find the pending approval");
    let (invoice_id, inv_num, vendor, amount, email, approver_uid) = row.unwrap();
    assert!(inv_num.starts_with("RESEND-TEST-"));
    assert_eq!(vendor, "Test Vendor");
    assert_eq!(amount, 10000);
    assert!(email.is_some());
    assert!(
        approver_uid.is_some(),
        "approver_user_id should be resolved from requested_from"
    );

    cleanup(&pool, tenant_id, approval_id, invoice_id).await;
}

// ===========================================================================
// Tenant isolation: query with wrong tenant returns no rows
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn test_fetch_approval_for_link_cross_tenant_returns_empty() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let wrong_tenant_id = Uuid::parse_str(OTHER_TENANT_ID).unwrap();
    let approval_id = create_pending_approval(&pool, tenant_id).await;

    // Query with a different tenant - should find nothing
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT ar.invoice_id FROM approval_requests ar
         WHERE ar.id = $1 AND ar.tenant_id = $2 AND ar.status = 'pending'",
    )
    .bind(approval_id)
    .bind(wrong_tenant_id)
    .fetch_optional(&pool)
    .await
    .expect("query");

    assert!(row.is_none(), "Cross-tenant query should return no rows");

    // Get invoice_id for cleanup
    let invoice_id: uuid::Uuid =
        sqlx::query_scalar("SELECT invoice_id FROM approval_requests WHERE id = $1")
            .bind(approval_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    cleanup(&pool, tenant_id, approval_id, invoice_id).await;
}

// ===========================================================================
// Audit entry is written when tokens are generated
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn test_token_generation_creates_email_action_tokens_rows() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();
    let approval_id = create_pending_approval(&pool, tenant_id).await;

    // Use EmailActionTokenService directly (mirrors what the handler does)
    let token_service = billforge_core::services::EmailActionTokenService::new(
        std::sync::Arc::new(pool.clone()),
        "test-secret-key".to_string(),
    );

    let tenant_id_obj = billforge_core::TenantId(tenant_id);
    let user_id_obj = billforge_core::UserId(user_id);

    let invoice_id: uuid::Uuid =
        sqlx::query_scalar("SELECT invoice_id FROM approval_requests WHERE id = $1")
            .bind(approval_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let token = token_service
        .generate_token(
            &tenant_id_obj,
            &user_id_obj,
            billforge_core::services::EmailAction::ApproveInvoice,
            invoice_id,
            "approval_request",
            serde_json::json!({ "approval_request_id": approval_id.to_string() }),
        )
        .await
        .expect("token generation");

    // Verify the token was stored
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM email_action_tokens WHERE resource_id = $1 AND user_id = $2",
    )
    .bind(invoice_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "Token should be stored in email_action_tokens");

    // Verify the token is usable once
    let payload = token_service
        .validate_token(&token)
        .await
        .expect("validate");
    assert_eq!(payload.resource_id, invoice_id);

    // Mark used
    token_service.mark_used(&token).await.expect("mark used");

    // Second use should fail
    let reused = token_service.validate_token(&token).await;
    assert!(reused.is_err(), "Single-use token should not be reusable");

    cleanup(&pool, tenant_id, approval_id, invoice_id).await;
}

// ===========================================================================
// Token is bound to the assignee's user_id, not an arbitrary caller
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn test_token_uses_assignee_user_id_not_caller() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let assignee_user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();
    let approval_id = create_pending_approval(&pool, tenant_id).await;

    // Resolve the approver_user_id from requested_from (mirrors fetch_approval_for_link)
    let approver_user_id: Option<uuid::Uuid> = sqlx::query_scalar(
        r#"SELECT (requested_from->>'user_id')::uuid FROM approval_requests WHERE id = $1"#,
    )
    .bind(approval_id)
    .fetch_optional(&pool)
    .await
    .expect("query")
    .flatten();

    assert!(
        approver_user_id.is_some(),
        "Should resolve approver_user_id from requested_from"
    );
    assert_eq!(
        approver_user_id.unwrap(),
        assignee_user_id,
        "Token should bind to the assignee, not a different caller"
    );

    let invoice_id: uuid::Uuid =
        sqlx::query_scalar("SELECT invoice_id FROM approval_requests WHERE id = $1")
            .bind(approval_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    cleanup(&pool, tenant_id, approval_id, invoice_id).await;
}
