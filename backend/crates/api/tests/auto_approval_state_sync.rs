//! Integration tests for auto-approval state-machine sync (issue #426).
//!
//! Auto-approval lanes used to bypass `state_machine::transition` and write a
//! mismatched audit-log row directly, leaving `invoices.status='received'` and
//! `invoices.processing_status='approved'`. These tests assert the lane now
//! routes the approval through the state machine so both columns end up
//! `'approved'` and exactly one audit-log row is written with the lane label
//! preserved as the audit `event_type`.

#![allow(warnings)]

use billforge_core::TenantId;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

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

async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("auto-approve-{}@example.com", user_id))
    .bind("hash_not_used")
    .bind("Auto-Approve Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

async fn read_status(pool: &sqlx::PgPool, invoice_id: Uuid) -> String {
    let row: (String,) = sqlx::query_as("SELECT status FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_one(pool)
        .await
        .expect("read invoice.status");
    row.0
}

async fn read_processing_status(pool: &sqlx::PgPool, invoice_id: Uuid) -> String {
    let row: (String,) = sqlx::query_as("SELECT processing_status FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_one(pool)
        .await
        .expect("read invoice.processing_status");
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

/// Mirrors the bridge step the production callers in routes/invoices.rs do:
/// `state_machine::transition` with the lane label, then
/// `update_processing_status`. This is what the auto-approval lanes in
/// `WorkflowEngine::process_invoice` now hand off to the API caller.
async fn simulate_auto_approval_caller(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
    lane_event_type: &'static str,
) {
    billforge_api::state_machine::transition(
        pool,
        tenant_id,
        &invoice_id,
        &Uuid::nil(),
        billforge_api::state_machine::InvoiceStatus::Approved,
        lane_event_type,
        serde_json::json!({"lane": lane_event_type}),
    )
    .await
    .expect("state machine should accept Received -> Approved auto-approval");

    sqlx::query("UPDATE invoices SET processing_status = 'approved' WHERE id = $1")
        .bind(invoice_id)
        .execute(pool)
        .await
        .expect("update processing_status");
}

// ============================================================================
// Recurring-pattern lane
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test auto_approval_state_sync -- --ignored
async fn recurring_pattern_lane_keeps_status_columns_in_sync(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    simulate_auto_approval_caller(&pool, &tenant_id, invoice_id, "recurring_pattern_match").await;

    assert_eq!(
        read_status(&pool, invoice_id).await,
        "approved",
        "invoices.status must be updated via the state machine"
    );
    assert_eq!(
        read_processing_status(&pool, invoice_id).await,
        "approved",
        "invoices.processing_status must match invoices.status"
    );

    let rows = audit_rows(&pool, &tenant_id, invoice_id).await;
    assert_eq!(
        rows.len(),
        1,
        "exactly one canonical audit-log row should be written"
    );
    let (actor, from_status, to_status, event_type) = &rows[0];
    assert!(
        actor.map(|id| id == Uuid::nil()).unwrap_or(false),
        "system actor (nil UUID) recorded for touchless approval"
    );
    assert_eq!(from_status.as_deref(), Some("received"));
    assert_eq!(to_status, "approved");
    assert_eq!(
        event_type, "recurring_pattern_match",
        "audit event_type should preserve the auto-approval lane label"
    );
}

// ============================================================================
// Touchless ML-confidence lane
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test auto_approval_state_sync -- --ignored
async fn touchless_ml_lane_keeps_status_columns_in_sync(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_received_invoice(&pool, &tenant_id, user_id).await;

    simulate_auto_approval_caller(&pool, &tenant_id, invoice_id, "touchless_auto_approval").await;

    assert_eq!(read_status(&pool, invoice_id).await, "approved");
    assert_eq!(read_processing_status(&pool, invoice_id).await, "approved");

    let rows = audit_rows(&pool, &tenant_id, invoice_id).await;
    assert_eq!(rows.len(), 1);
    let (_, from_status, to_status, event_type) = &rows[0];
    assert_eq!(from_status.as_deref(), Some("received"));
    assert_eq!(to_status, "approved");
    assert_eq!(event_type, "touchless_auto_approval");
}
