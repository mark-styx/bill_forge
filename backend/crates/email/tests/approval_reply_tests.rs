//! Reply-by-email approval ingest tests.
//!
//! Pure-function tests (parse_reply_command / extract_action_token) run
//! unconditionally. The end-to-end and replay tests require a real PostgreSQL
//! database and are gated behind `#[ignore]` + `DATABASE_URL` (run with
//! `cargo test -p billforge-email -- --ignored`).

#![allow(warnings)]

use billforge_email::{
    extract_action_token, handle_approval_reply, parse_reply_command, ApprovalReplyOutcome,
    InboundEmailPayload, ReplyCommand,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Pure-function tests (no DB required)
// ---------------------------------------------------------------------------

fn payload(reply_to: Option<&str>, subject: Option<&str>, html: Option<&str>) -> InboundEmailPayload {
    InboundEmailPayload {
        from: "approver@acme.com".to_string(),
        to: "ap@meridian.billforge.com".to_string(),
        subject: subject.map(|s| s.to_string()),
        message_id: Some("msg-1".to_string()),
        text_body: None,
        html_body: html.map(|s| s.to_string()),
        reply_to: reply_to.map(|s| s.to_string()),
        attachments: vec![],
    }
}

#[test]
fn parse_reply_command_recognises_approve_variants() {
    assert_eq!(parse_reply_command("APPROVE"), Some(ReplyCommand::Approve));
    assert_eq!(
        parse_reply_command("approve\n--\nSent from my iPhone"),
        Some(ReplyCommand::Approve)
    );
}

#[test]
fn parse_reply_command_recognises_reject_with_reason() {
    assert_eq!(
        parse_reply_command("REJECT duplicate invoice"),
        Some(ReplyCommand::Reject(Some("duplicate invoice".to_string())))
    );
}

#[test]
fn parse_reply_command_recognises_delegate() {
    assert_eq!(
        parse_reply_command("DELEGATE jane@acme.com"),
        Some(ReplyCommand::Delegate("jane@acme.com".to_string()))
    );
}

#[test]
fn parse_reply_command_returns_none_for_garbage() {
    assert_eq!(parse_reply_command("thanks, looks good"), None);
}

#[test]
fn parse_reply_command_returns_none_when_only_quoted_lines() {
    assert_eq!(parse_reply_command("> APPROVE\n> previous email"), None);
}

#[test]
fn extract_action_token_from_reply_to_plus_address() {
    let msg = payload(
        Some("\"Approvals\" <approvals+TOK123@billforge.com>"),
        None,
        None,
    );
    assert_eq!(extract_action_token(&msg), Some("TOK123".to_string()));
}

#[test]
fn extract_action_token_from_subject_marker() {
    let msg = payload(None, Some("Re: [BF-APR-SUBJ-TOK] Approval Required"), None);
    assert_eq!(extract_action_token(&msg), Some("SUBJ-TOK".to_string()));
}

#[test]
fn extract_action_token_from_hidden_html_span() {
    let html = r#"<p>Yes</p><span style="display:none">[bf-token:HTML-TOK]</span>"#;
    let msg = payload(None, None, Some(html));
    assert_eq!(extract_action_token(&msg), Some("HTML-TOK".to_string()));
}

#[test]
fn extract_action_token_returns_none_when_no_carrier_present() {
    let msg = payload(None, Some("Re: Approval Required"), Some("<p>ok</p>"));
    assert_eq!(extract_action_token(&msg), None);
}

// ---------------------------------------------------------------------------
// DB-backed end-to-end tests (require DATABASE_URL)
// ---------------------------------------------------------------------------

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
const FIXTURE_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

async fn seed_pending_approval(pool: &sqlx::PgPool, tenant_id: Uuid) -> (Uuid, Uuid) {
    let user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();
    let invoice_id = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();

    billforge_db::tenant_db::run_tenant_migrations(
        pool,
        &billforge_core::TenantId(tenant_id),
    )
    .await
    .expect("tenant migrations");

    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'approval-reply-test@example.com', '', 'Approval Reply Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create fixture user");

    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, document_id, created_by, status)
         VALUES ($1, $2, 'Reply Vendor', $3, 25000, $4, $5, 'pending_approval')
         ON CONFLICT DO NOTHING",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .bind(format!("REPLY-TEST-{}", invoice_id))
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("create invoice");

    sqlx::query(
        "INSERT INTO approval_requests (id, tenant_id, invoice_id, requested_from, status)
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(approval_id)
    .bind(tenant_id.to_string())
    .bind(invoice_id)
    .bind(serde_json::json!({ "User": user_id.to_string() }))
    .execute(pool)
    .await
    .expect("create approval_request");

    (invoice_id, approval_id)
}

async fn cleanup(pool: &sqlx::PgPool, tenant_id: Uuid, invoice_id: Uuid, approval_id: Uuid) {
    sqlx::query("DELETE FROM approval_requests WHERE id = $1 OR invoice_id = $2")
        .bind(approval_id)
        .bind(invoice_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM invoices WHERE id = $1 AND tenant_id = $2")
        .bind(invoice_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM audit_log WHERE resource_id = $1 AND tenant_id = $2")
        .bind(invoice_id.to_string())
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
}

async fn mint_approve_token(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    invoice_id: Uuid,
) -> String {
    use billforge_core::services::{EmailAction, EmailActionTokenService};
    use std::sync::Arc;

    std::env::set_var("TOKEN_SECRET_KEY", "approval-reply-tests-secret");
    let service = EmailActionTokenService::new(
        Arc::new(pool.clone()),
        "approval-reply-tests-secret".to_string(),
    );
    service
        .generate_token(
            &billforge_core::TenantId(tenant_id),
            &billforge_core::UserId(user_id),
            EmailAction::ApproveInvoice,
            invoice_id,
            "approval_request",
            serde_json::json!({}),
        )
        .await
        .expect("mint token")
}

#[tokio::test]
#[ignore]
async fn end_to_end_reply_approves_invoice_with_token_in_reply_to() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();

    let (invoice_id, approval_id) = seed_pending_approval(&pool, tenant_id).await;
    let token = mint_approve_token(&pool, tenant_id, user_id, invoice_id).await;

    let msg = InboundEmailPayload {
        from: "approval-reply-test@example.com".to_string(),
        to: "ap@meridian.billforge.com".to_string(),
        subject: Some("Re: Approval Required: Invoice".to_string()),
        message_id: Some("msg-reply-1".to_string()),
        text_body: Some("APPROVE\n\n> previous email content".to_string()),
        html_body: None,
        reply_to: Some(format!("approvals+{}@billforge.com", token)),
        attachments: vec![],
    };

    let recovered = extract_action_token(&msg).expect("token must be recovered");
    assert_eq!(recovered, token);
    let cmd = parse_reply_command(msg.text_body.as_deref().unwrap()).expect("must parse");

    let inbound_email_id = Uuid::new_v4();
    let outcome = handle_approval_reply(&pool, &pool, inbound_email_id, &msg, &recovered, &cmd)
        .await
        .expect("handle ok");

    match outcome {
        ApprovalReplyOutcome::Applied { action, .. } => assert_eq!(action, "approved"),
        other => panic!("expected Applied, got {:?}", other),
    }

    // approval_requests row should now be approved
    let status: String =
        sqlx::query_scalar("SELECT status FROM approval_requests WHERE id = $1")
            .bind(approval_id)
            .fetch_one(&pool)
            .await
            .expect("read approval status");
    assert_eq!(status, "approved");

    // token row should be marked consumed
    let used: Option<bool> = sqlx::query_scalar(
        "SELECT (used_at IS NOT NULL) FROM email_action_tokens WHERE user_id = $1 AND resource_id = $2",
    )
    .bind(user_id)
    .bind(invoice_id)
    .fetch_optional(&pool)
    .await
    .expect("read token row");
    assert_eq!(used, Some(true));

    // audit log row with channel=email_reply should exist
    let audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log
         WHERE tenant_id = $1 AND resource_id = $2
           AND (changes->'metadata'->>'channel') = 'email_reply'",
    )
    .bind(tenant_id)
    .bind(invoice_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("query audit log");
    assert!(audit_count.0 >= 1, "expected at least one email_reply audit row");

    cleanup(&pool, tenant_id, invoice_id, approval_id).await;
}

#[tokio::test]
#[ignore]
async fn replay_attempt_does_not_double_approve() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();

    let (invoice_id, approval_id) = seed_pending_approval(&pool, tenant_id).await;
    let token = mint_approve_token(&pool, tenant_id, user_id, invoice_id).await;

    let msg = InboundEmailPayload {
        from: "approval-reply-test@example.com".to_string(),
        to: "ap@meridian.billforge.com".to_string(),
        subject: Some("Re: Approval Required".to_string()),
        message_id: Some("msg-replay".to_string()),
        text_body: Some("APPROVE".to_string()),
        html_body: None,
        reply_to: Some(format!("approvals+{}@billforge.com", token)),
        attachments: vec![],
    };
    let cmd = ReplyCommand::Approve;

    let first = handle_approval_reply(&pool, &pool, Uuid::new_v4(), &msg, &token, &cmd)
        .await
        .expect("first run ok");
    assert!(matches!(first, ApprovalReplyOutcome::Applied { .. }));

    let second = handle_approval_reply(&pool, &pool, Uuid::new_v4(), &msg, &token, &cmd)
        .await
        .expect("second run returns Triaged, not error");
    assert!(matches!(second, ApprovalReplyOutcome::Triaged { .. }));

    cleanup(&pool, tenant_id, invoice_id, approval_id).await;
}
