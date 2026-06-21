//! Integration tests for worker-side auto-approval state-machine sync
//! (issue #426 regression in the OCR straight-through path).
//!
//! The OCR straight-through path (`run_straight_through_processing`) used to
//! call `engine.process_invoice(...).status` and go straight to
//! `update_processing_status` without routing touchless auto-approval lanes
//! (recurring-pattern / ML-confidence) through the state machine. That left
//! `invoices.status='received'`, `processing_status='approved'`, and zero
//! audit-log rows. These tests assert the worker-side bridge now transitions
//! `invoices.status` and writes the single canonical audit row, matching the
//! API caller behaviour.
//!
//! `#[ignore]` because they require a migrated PostgreSQL instance. Run with:
//! `cargo test -p billforge-worker --test auto_approval_state_sync -- --ignored`

#![allow(warnings)]

use billforge_core::domain::ProcessingStatus;
use billforge_core::TenantId;
use billforge_invoice_processing::{AutoApprovalLane, ProcessInvoiceOutcome};
use billforge_worker::jobs::ocr_processing::{
    sync_auto_approval_with_state_machine, transition_invoice_to_approved,
};
use std::sync::Arc;
use uuid::Uuid;

async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("worker-auto-approve-{}@example.com", user_id))
    .bind("hash_not_used")
    .bind("Worker Auto-Approve Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Insert an invoice in the `received` lifecycle status, the entry point for
/// the touchless auto-approval transition.
async fn insert_received_invoice(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) -> Uuid {
    let invoice_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number,
               total_amount_cents, currency, capture_status, processing_status,
               document_id, created_by, status)
           VALUES ($1, $2, 'AutoApprove Vendor', $3, 10000, 'USD',
                   'reviewed', 'submitted', $4, $5, 'received')"#,
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("AA-{}", invoice_id))
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert received invoice");
    invoice_id
}

async fn read_status(pool: &sqlx::PgPool, invoice_id: Uuid) -> String {
    let row: (String,) = sqlx::query_as("SELECT status FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_one(pool)
        .await
        .expect("read invoice.status");
    row.0
}

async fn audit_rows(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
) -> Vec<(Option<Uuid>, Option<String>, String, String)> {
    sqlx::query_as::<_, (Option<Uuid>, Option<String>, String, String)>(
        r#"SELECT actor_id, from_status, to_status, event_type
           FROM invoice_audit_log
           WHERE tenant_id = $1 AND invoice_id = $2
           ORDER BY created_at ASC"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .fetch_all(pool)
    .await
    .expect("read audit rows")
}

fn lane_outcome(event_type: &'static str) -> ProcessInvoiceOutcome {
    ProcessInvoiceOutcome {
        status: ProcessingStatus::Approved,
        auto_approval_lane: Some(AutoApprovalLane {
            event_type,
            metadata: serde_json::json!({"lane": event_type}),
        }),
    }
}

// ============================================================================
// Direct canonical transition writer
// ============================================================================

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with invoices/invoice_audit_log tables"]
async fn transition_invoice_to_approved_writes_status_and_audit_row(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    transition_invoice_to_approved(
        &pool,
        &tenant_id,
        &invoice_id,
        &Uuid::nil(),
        "recurring_pattern_match",
        serde_json::json!({"pattern_id": "p-1"}),
    )
    .await
    .expect("transition should succeed for received -> approved");

    assert_eq!(
        read_status(&pool, invoice_id).await,
        "approved",
        "invoices.status must be updated by the transition"
    );

    let rows = audit_rows(&pool, &tenant_id, invoice_id).await;
    assert_eq!(rows.len(), 1, "exactly one canonical audit-log row");
    let (actor, from_status, to_status, event_type) = &rows[0];
    assert!(
        actor.map(|id| id == Uuid::nil()).unwrap_or(false),
        "system actor (nil UUID) recorded for touchless approval"
    );
    assert_eq!(from_status.as_deref(), Some("received"));
    assert_eq!(to_status, "approved");
    assert_eq!(event_type, "recurring_pattern_match");
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with invoices/invoice_audit_log tables"]
async fn transition_invoice_to_approved_rejects_non_received_status(pool: sqlx::PgPool) {
    // An invoice already approved cannot be auto-approved again: the canonical
    // state machine rejects the transition rather than double-writing.
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    // Move it to approved first.
    transition_invoice_to_approved(
        &pool,
        &tenant_id,
        &invoice_id,
        &Uuid::nil(),
        "recurring_pattern_match",
        serde_json::json!({}),
    )
    .await
    .expect("first transition should succeed");

    // Second transition must be rejected.
    let err = transition_invoice_to_approved(
        &pool,
        &tenant_id,
        &invoice_id,
        &Uuid::nil(),
        "touchless_auto_approval",
        serde_json::json!({}),
    )
    .await
    .expect_err("second transition should be rejected");

    assert!(
        matches!(err, billforge_core::Error::Validation(_)),
        "expected Validation error for invalid transition, got {:?}",
        err
    );

    // Still exactly one audit row from the first transition.
    assert_eq!(audit_rows(&pool, &tenant_id, invoice_id).await.len(), 1);
}

// ============================================================================
// Bridge (sync_auto_approval_with_state_machine)
// ============================================================================

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with invoices/invoice_audit_log tables"]
async fn bridge_recurring_pattern_lane_keeps_columns_in_sync(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    let proceed = sync_auto_approval_with_state_machine(
        &pool,
        &tenant_id,
        &invoice_id,
        &lane_outcome("recurring_pattern_match"),
    )
    .await
    .expect("bridge should not hard-fail");

    assert!(
        proceed,
        "proceed=true signals the caller writes processing_status"
    );

    assert_eq!(read_status(&pool, invoice_id).await, "approved");

    let rows = audit_rows(&pool, &tenant_id, invoice_id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, "approved");
    assert_eq!(rows[0].3, "recurring_pattern_match");
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with invoices/invoice_audit_log tables"]
async fn bridge_touchless_ml_lane_keeps_columns_in_sync(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    let proceed = sync_auto_approval_with_state_machine(
        &pool,
        &tenant_id,
        &invoice_id,
        &lane_outcome("touchless_auto_approval"),
    )
    .await
    .expect("bridge should not hard-fail");

    assert!(proceed);
    assert_eq!(read_status(&pool, invoice_id).await, "approved");

    let rows = audit_rows(&pool, &tenant_id, invoice_id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].3, "touchless_auto_approval");
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with invoices/invoice_audit_log tables"]
async fn bridge_no_lane_returns_proceed_without_writing(pool: sqlx::PgPool) {
    // When no auto-approval lane fired (e.g. a rule-based auto-approve or
    // pending-approval outcome), the bridge is a no-op that tells the caller
    // to proceed. It must NOT touch invoices.status or the audit log.
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    let outcome = ProcessInvoiceOutcome::new(ProcessingStatus::Approved);
    let proceed = sync_auto_approval_with_state_machine(&pool, &tenant_id, &invoice_id, &outcome)
        .await
        .expect("bridge should not hard-fail");

    assert!(proceed, "no lane -> caller proceeds as before");
    assert_eq!(
        read_status(&pool, invoice_id).await,
        "received",
        "no lane must not transition invoices.status"
    );
    assert_eq!(
        audit_rows(&pool, &tenant_id, invoice_id).await.len(),
        0,
        "no lane must not write an audit row"
    );
}
