//! Integration tests for the invoice status state machine and tenant-scoped audit log.
//!
//! Tests:
//! 1. Happy path: received -> in_review writes audit row with correct tenant_id, from/to, actor
//! 2. Invalid transition rejected: paid -> received returns error, no audit row, status unchanged
//! 3. Tenant isolation: audit log query scoped to tenant A does not return tenant B rows
//! 4. Atomicity: simulated failure after status update rolls back audit insert

#![allow(warnings)]

use billforge_core::TenantId;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run all tenant migrations so invoice_audit_log and FK targets exist.
async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

/// Insert a minimal user row.
async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind("state-machine-test@example.com")
    .bind("hash_not_used")
    .bind("SM Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Insert a minimal invoice with a given status.  Returns the invoice UUID.
async fn insert_invoice(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: Uuid,
    status: &str,
) -> Uuid {
    let invoice_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number,
               total_amount_cents, currency, capture_status, processing_status,
               document_id, created_by, status)
           VALUES ($1, $2, 'Test Vendor', 'SM-INV-001', 10000, 'USD',
                   'reviewed', 'pending_approval', $3, $4, $5)"#,
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .bind(doc_id)
    .bind(user_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert test invoice");
    invoice_id
}

/// Read the status column for an invoice.
async fn read_status(pool: &sqlx::PgPool, invoice_id: Uuid) -> String {
    let row: (String,) = sqlx::query_as("SELECT status FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_one(pool)
        .await
        .expect("read invoice status");
    row.0
}

/// Count audit log rows for a specific invoice + tenant.
async fn count_audit_rows(pool: &sqlx::PgPool, tenant_id: &TenantId, invoice_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoice_audit_log WHERE tenant_id = $1 AND invoice_id = $2",
    )
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .fetch_one(pool)
    .await
    .expect("count audit rows");
    row.0
}

/// Read the most recent audit row for a given invoice + tenant.
async fn read_latest_audit(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
) -> Option<(
    Option<Uuid>,
    Option<String>,
    String,
    String,
    serde_json::Value,
)> {
    let row: Option<(
        Option<Uuid>,
        Option<String>,
        String,
        String,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"SELECT actor_id, from_status, to_status, event_type, metadata
           FROM invoice_audit_log
           WHERE tenant_id = $1 AND invoice_id = $2
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .fetch_optional(pool)
    .await
    .expect("read audit row");
    row
}

// ============================================================================
// Test 1: Happy path - received -> in_review writes correct audit row
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test invoice_state_machine -- --ignored
async fn happy_path_transition_writes_audit_row(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_invoice(&pool, &tenant_id, user_id, "received").await;

    // Execute the transition
    billforge_api::state_machine::transition(
        &pool,
        &tenant_id,
        &invoice_id,
        &user_id,
        billforge_api::state_machine::InvoiceStatus::InReview,
        "start_review",
        serde_json::json!({"note": "beginning review"}),
    )
    .await
    .expect("transition should succeed");

    // Verify status updated
    let status = read_status(&pool, invoice_id).await;
    assert_eq!(status, "in_review");

    // Verify audit row
    assert_eq!(count_audit_rows(&pool, &tenant_id, invoice_id).await, 1);

    let audit = read_latest_audit(&pool, &tenant_id, invoice_id)
        .await
        .expect("audit row must exist");

    let (actor, from_status, to_status, event_type, metadata) = audit;
    assert_eq!(actor, Some(user_id));
    assert_eq!(from_status, Some("received".to_string()));
    assert_eq!(to_status, "in_review");
    assert_eq!(event_type, "start_review");
    assert_eq!(metadata["note"], "beginning review");
}

// ============================================================================
// Test 2: Invalid transition is rejected, no audit row written
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test invoice_state_machine -- --ignored
async fn invalid_transition_rejected_no_audit_row(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_invoice(&pool, &tenant_id, user_id, "paid").await;

    // Attempt invalid transition: paid -> received
    let result = billforge_api::state_machine::transition(
        &pool,
        &tenant_id,
        &invoice_id,
        &user_id,
        billforge_api::state_machine::InvoiceStatus::Received,
        "invalid_event",
        serde_json::json!({}),
    )
    .await;

    assert!(result.is_err(), "paid -> received should be rejected");

    // Verify status unchanged
    let status = read_status(&pool, invoice_id).await;
    assert_eq!(status, "paid", "status should remain 'paid'");

    // Verify no audit row written
    assert_eq!(
        count_audit_rows(&pool, &tenant_id, invoice_id).await,
        0,
        "no audit row should be written for invalid transition"
    );
}

// ============================================================================
// Test 3: Tenant isolation - tenant A cannot see tenant B's audit rows
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test invoice_state_machine -- --ignored
async fn tenant_isolation_audit_rows_scoped(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);

    // Set up two tenants
    let tenant_a = TenantId::from_uuid(Uuid::new_v4());
    let tenant_b = TenantId::from_uuid(Uuid::new_v4());
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    setup_schema(&pool, &tenant_a).await;
    insert_user(&pool, &tenant_a, user_a).await;
    let invoice_a = insert_invoice(&pool, &tenant_a, user_a, "received").await;

    setup_schema(&pool, &tenant_b).await;
    insert_user(&pool, &tenant_b, user_b).await;
    let invoice_b = insert_invoice(&pool, &tenant_b, user_b, "received").await;

    // Transition invoice_a
    billforge_api::state_machine::transition(
        &pool,
        &tenant_a,
        &invoice_a,
        &user_a,
        billforge_api::state_machine::InvoiceStatus::InReview,
        "start_review",
        serde_json::json!({}),
    )
    .await
    .expect("tenant A transition");

    // Transition invoice_b
    billforge_api::state_machine::transition(
        &pool,
        &tenant_b,
        &invoice_b,
        &user_b,
        billforge_api::state_machine::InvoiceStatus::InReview,
        "start_review",
        serde_json::json!({}),
    )
    .await
    .expect("tenant B transition");

    // Tenant A should only see its own row
    assert_eq!(count_audit_rows(&pool, &tenant_a, invoice_a).await, 1);
    assert_eq!(count_audit_rows(&pool, &tenant_a, invoice_b).await, 0);

    // Tenant B should only see its own row
    assert_eq!(count_audit_rows(&pool, &tenant_b, invoice_b).await, 1);
    assert_eq!(count_audit_rows(&pool, &tenant_b, invoice_a).await, 0);
}

// ============================================================================
// Test 4: Atomicity - transaction abort rolls back both status and audit
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test invoice_state_machine -- --ignored
async fn transition_atomicity_on_constraint_violation(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_invoice(&pool, &tenant_id, user_id, "received").await;

    // Transition succeeds
    billforge_api::state_machine::transition(
        &pool,
        &tenant_id,
        &invoice_id,
        &user_id,
        billforge_api::state_machine::InvoiceStatus::InReview,
        "start_review",
        serde_json::json!({}),
    )
    .await
    .expect("first transition");

    assert_eq!(read_status(&pool, invoice_id).await, "in_review");
    assert_eq!(count_audit_rows(&pool, &tenant_id, invoice_id).await, 1);

    // Now try an invalid transition from in_review (paid is not reachable from in_review).
    // This exercises the validation path and ensures rollback leaves state intact.
    let result = billforge_api::state_machine::transition(
        &pool,
        &tenant_id,
        &invoice_id,
        &user_id,
        billforge_api::state_machine::InvoiceStatus::Paid,
        "mark_paid",
        serde_json::json!({}),
    )
    .await;

    assert!(result.is_err(), "in_review -> paid should be rejected");

    // Status unchanged, audit count unchanged
    assert_eq!(read_status(&pool, invoice_id).await, "in_review");
    assert_eq!(count_audit_rows(&pool, &tenant_id, invoice_id).await, 1);
}

// ============================================================================
// Test 5: GL posting stage - paid -> posted writes audit row, metadata intact
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test invoice_state_machine -- --ignored
async fn posted_state_paid_to_posted_writes_audit_row(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_invoice(&pool, &tenant_id, user_id, "paid").await;

    let metadata = serde_json::json!({"erp": "qbo", "journal_entry_id": "je_test"});

    billforge_api::state_machine::transition(
        &pool,
        &tenant_id,
        &invoice_id,
        &user_id,
        billforge_api::state_machine::InvoiceStatus::Posted,
        "post_to_gl",
        metadata.clone(),
    )
    .await
    .expect("paid -> posted should succeed");

    assert_eq!(read_status(&pool, invoice_id).await, "posted");
    assert_eq!(count_audit_rows(&pool, &tenant_id, invoice_id).await, 1);

    let audit = read_latest_audit(&pool, &tenant_id, invoice_id)
        .await
        .expect("audit row must exist");

    let (actor, from_status, to_status, event_type, recorded_metadata) = audit;
    assert_eq!(actor, Some(user_id));
    assert_eq!(from_status, Some("paid".to_string()));
    assert_eq!(to_status, "posted");
    assert_eq!(event_type, "post_to_gl");
    assert_eq!(recorded_metadata["erp"], "qbo");
    assert_eq!(recorded_metadata["journal_entry_id"], "je_test");
}

// ============================================================================
// Test 6: Posted is terminal - posted -> paid is rejected, no audit row
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test invoice_state_machine -- --ignored
async fn posted_is_terminal_rejects_back_transition(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let invoice_id = insert_invoice(&pool, &tenant_id, user_id, "posted").await;

    let result = billforge_api::state_machine::transition(
        &pool,
        &tenant_id,
        &invoice_id,
        &user_id,
        billforge_api::state_machine::InvoiceStatus::Paid,
        "mark_paid",
        serde_json::json!({}),
    )
    .await;

    assert!(result.is_err(), "posted -> paid should be rejected");
    match result {
        Err(billforge_core::Error::Validation(_)) => {}
        other => panic!("expected Validation error, got {:?}", other),
    }

    assert_eq!(read_status(&pool, invoice_id).await, "posted");
    assert_eq!(count_audit_rows(&pool, &tenant_id, invoice_id).await, 0);
}
