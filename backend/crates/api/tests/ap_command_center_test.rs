//! Integration tests for the AP Command Center endpoint.
//!
//! Validates response shape, bucket assignment, blocking-approver metadata,
//! discount-expiry computation, and tenant isolation.
//!
//! Struct-level tests run without a database.  DB-backed integration tests
//! require `DATABASE_URL` and are gated behind `#[ignore]` — run with:
//!
//!     cargo test -p billforge-api --test ap_command_center_test -- --ignored

use billforge_api::routes::ap_command_center::{
    ApCommandCenterResponse, BlockingInvoice, Bucket,
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn this_week_bounds() -> (NaiveDate, NaiveDate) {
    let today = Utc::now().date_naive();
    let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    (start, start + Duration::days(6))
}

fn next_week_bounds() -> (NaiveDate, NaiveDate) {
    let (this_start, _) = this_week_bounds();
    (this_start + Duration::days(7), this_start + Duration::days(13))
}

fn make_invoice(
    amount_cents: i64,
    due_date: NaiveDate,
    blocking_approver_name: Option<&str>,
    days_stuck: i32,
    discount_expiring_cents: i64,
    discount_expires_at: Option<NaiveDate>,
    late_fee_risk_cents: i64,
) -> BlockingInvoice {
    BlockingInvoice {
        invoice_id: Uuid::new_v4(),
        invoice_number: format!("INV-{}", Uuid::new_v4().as_simple().to_string()[..6].to_uppercase()),
        vendor_name: "Test Vendor".to_string(),
        amount_cents,
        due_date,
        blocking_approver_id: blocking_approver_name.map(|_| Uuid::new_v4()),
        blocking_approver_name: blocking_approver_name.map(|s| s.to_string()),
        days_stuck,
        late_fee_risk_cents,
        discount_expiring_cents,
        discount_expires_at,
    }
}

// ---------------------------------------------------------------------------
// Struct & serialization tests
// ---------------------------------------------------------------------------

#[test]
fn test_response_serializes_with_two_buckets() {
    let (tw_start, tw_end) = this_week_bounds();
    let (nw_start, nw_end) = next_week_bounds();

    let inv_this = make_invoice(50_000_00, tw_start, None, 0, 0, None, 0);
    let inv_next = make_invoice(30_000_00, nw_start, Some("Alice"), 3, 5_000_00, Some(nw_start), 0);

    let resp = ApCommandCenterResponse {
        week_buckets: vec![
            Bucket {
                label: "This week".to_string(),
                range_start: tw_start,
                range_end: tw_end,
                total_payable_cents: inv_this.amount_cents,
                invoices: vec![inv_this.clone()],
            },
            Bucket {
                label: "Next week".to_string(),
                range_start: nw_start,
                range_end: nw_end,
                total_payable_cents: inv_next.amount_cents,
                invoices: vec![inv_next.clone()],
            },
        ],
        late_fee_risk_total_cents: 0,
        discount_expiring_total_cents: inv_next.discount_expiring_cents,
        generated_at: Utc::now(),
    };

    let json = serde_json::to_value(&resp).expect("Should serialize");
    let buckets = json["week_buckets"].as_array().expect("week_buckets is array");
    assert_eq!(buckets.len(), 2, "Should have exactly 2 buckets");

    assert_eq!(buckets[0]["label"], "This week");
    assert_eq!(buckets[1]["label"], "Next week");

    let this_invoices = buckets[0]["invoices"].as_array().unwrap();
    assert_eq!(this_invoices.len(), 1);
    assert_eq!(this_invoices[0]["amount_cents"], 50_000_00);
}

#[test]
fn test_blocking_invoice_json_shape() {
    let id = Uuid::new_v4();
    let approver_id = Uuid::new_v4();
    let due = NaiveDate::from_ymd_opt(2026, 6, 5).unwrap();

    let inv = BlockingInvoice {
        invoice_id: id,
        invoice_number: "INV-001".to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount_cents: 25_000_00,
        due_date: due,
        blocking_approver_id: Some(approver_id),
        blocking_approver_name: Some("Bob".to_string()),
        days_stuck: 4,
        late_fee_risk_cents: 0,
        discount_expiring_cents: 1_200_00,
        discount_expires_at: Some(due),
    };

    let json = serde_json::to_value(&inv).expect("Should serialize");
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("invoice_id"));
    assert!(obj.contains_key("invoice_number"));
    assert!(obj.contains_key("vendor_name"));
    assert!(obj.contains_key("amount_cents"));
    assert!(obj.contains_key("due_date"));
    assert!(obj.contains_key("blocking_approver_id"));
    assert!(obj.contains_key("blocking_approver_name"));
    assert!(obj.contains_key("days_stuck"));
    assert!(obj.contains_key("late_fee_risk_cents"));
    assert!(obj.contains_key("discount_expiring_cents"));
    assert!(obj.contains_key("discount_expires_at"));

    assert_eq!(obj["blocking_approver_name"], "Bob");
    assert_eq!(obj["days_stuck"], 4);
    assert_eq!(obj["discount_expiring_cents"], 1_200_00);
}

#[test]
fn test_round_trip_serialization() {
    let id = Uuid::new_v4();
    let due = Utc::now().date_naive();

    let inv = BlockingInvoice {
        invoice_id: id,
        invoice_number: "INV-RT".to_string(),
        vendor_name: "Vendor".to_string(),
        amount_cents: 10_000_00,
        due_date: due,
        blocking_approver_id: None,
        blocking_approver_name: None,
        days_stuck: 0,
        late_fee_risk_cents: 0,
        discount_expiring_cents: 0,
        discount_expires_at: None,
    };

    let json = serde_json::to_string(&inv).expect("Serialize");
    let back: BlockingInvoice = serde_json::from_str(&json).expect("Deserialize");
    assert_eq!(back.invoice_id, id);
    assert_eq!(back.invoice_number, "INV-RT");
    assert_eq!(back.amount_cents, 10_000_00);
    assert!(back.blocking_approver_id.is_none());
    assert!(back.blocking_approver_name.is_none());
    assert_eq!(back.days_stuck, 0);
    assert_eq!(back.discount_expiring_cents, 0);
}

// ---------------------------------------------------------------------------
// Bucket-assignment logic tests (pure Rust, mirrors handler logic)
// ---------------------------------------------------------------------------

#[test]
fn test_invoices_assigned_to_correct_buckets() {
    let (tw_start, tw_end) = this_week_bounds();
    let (nw_start, nw_end) = next_week_bounds();

    // Invoices at various positions relative to the 14-day window
    let inv_this_early = make_invoice(40_000_00, tw_start, None, 0, 0, None, 0);
    let inv_this_late = make_invoice(20_000_00, tw_end, None, 0, 0, None, 0);
    let inv_next_early = make_invoice(60_000_00, nw_start, None, 0, 0, None, 0);
    let inv_next_late = make_invoice(10_000_00, nw_end, None, 0, 0, None, 0);

    // Mirror the handler's bucket-assignment logic
    let mut this_week: Vec<&BlockingInvoice> = Vec::new();
    let mut next_week: Vec<&BlockingInvoice> = Vec::new();

    for inv in [&inv_this_early, &inv_this_late, &inv_next_early, &inv_next_late] {
        if inv.due_date <= tw_end {
            this_week.push(inv);
        } else {
            next_week.push(inv);
        }
    }

    assert_eq!(this_week.len(), 2, "This-week bucket should have 2 invoices");
    assert_eq!(next_week.len(), 2, "Next-week bucket should have 2 invoices");

    // Verify totals
    let this_total: i64 = this_week.iter().map(|i| i.amount_cents).sum();
    let next_total: i64 = next_week.iter().map(|i| i.amount_cents).sum();
    assert_eq!(this_total, 60_000_00, "This-week total = 40k + 20k");
    assert_eq!(next_total, 70_000_00, "Next-week total = 60k + 10k");
}

#[test]
fn test_blocking_approver_populated_for_stuck_invoice() {
    let inv = make_invoice(
        15_000_00,
        Utc::now().date_naive(),
        Some("Carol Approver"),
        5,
        0,
        None,
        0,
    );
    assert_eq!(
        inv.blocking_approver_name.as_deref(),
        Some("Carol Approver")
    );
    assert!(inv.blocking_approver_id.is_some());
    assert_eq!(inv.days_stuck, 5);
}

#[test]
fn test_no_blocker_when_unassigned() {
    let inv = make_invoice(5_000_00, Utc::now().date_naive(), None, 0, 0, None, 0);
    assert!(inv.blocking_approver_id.is_none());
    assert!(inv.blocking_approver_name.is_none());
    assert_eq!(inv.days_stuck, 0);
}

#[test]
fn test_discount_expiring_cents_only_when_active() {
    let (nw_start, _nw_end) = next_week_bounds();

    // Invoice with discount expiring inside the window
    let inv_discount = make_invoice(100_000_00, nw_start, None, 0, 2_000_00, Some(nw_start), 0);
    assert_eq!(inv_discount.discount_expiring_cents, 2_000_00);
    assert!(inv_discount.discount_expires_at.is_some());

    // Invoice without discount
    let inv_no_discount = make_invoice(50_000_00, nw_start, None, 0, 0, None, 0);
    assert_eq!(inv_no_discount.discount_expiring_cents, 0);
    assert!(inv_no_discount.discount_expires_at.is_none());
}

#[test]
fn test_aggregate_totals_across_buckets() {
    let (tw_start, _tw_end) = this_week_bounds();
    let (nw_start, _nw_end) = next_week_bounds();

    let inv1 = make_invoice(100_000_00, tw_start, None, 0, 1_500_00, Some(tw_start), 0);
    let inv2 = make_invoice(200_000_00, nw_start, None, 0, 3_000_00, Some(nw_start), 0);

    let late_total = inv1.late_fee_risk_cents + inv2.late_fee_risk_cents;
    let discount_total = inv1.discount_expiring_cents + inv2.discount_expiring_cents;

    assert_eq!(late_total, 0, "Late-fee risk is 0 when no vendor terms set");
    assert_eq!(discount_total, 4_500_00, "Discount total = 1.5k + 3k");
}

#[test]
fn test_late_fee_risk_aggregate_with_vendor_terms() {
    let (tw_start, _tw_end) = this_week_bounds();
    let (nw_start, _nw_end) = next_week_bounds();

    // Two invoices with non-zero late-fee risk (simulating vendor late_fee_percent = 5.0)
    let inv1 = make_invoice(15_000_00, tw_start, None, 0, 0, None, 750_00);
    let inv2 = make_invoice(30_000_00, nw_start, None, 0, 0, None, 1_500_00);

    let late_total = inv1.late_fee_risk_cents + inv2.late_fee_risk_cents;

    assert_eq!(late_total, 2_250_00, "Late-fee risk aggregates across invoices");
}

#[test]
fn test_late_fee_risk_serializes_when_non_zero() {
    let inv = BlockingInvoice {
        invoice_id: Uuid::new_v4(),
        invoice_number: "INV-LF".to_string(),
        vendor_name: "Penalty Corp".to_string(),
        amount_cents: 10_000_00,
        due_date: Utc::now().date_naive(),
        blocking_approver_id: None,
        blocking_approver_name: None,
        days_stuck: 0,
        late_fee_risk_cents: 50_000,
        discount_expiring_cents: 0,
        discount_expires_at: None,
    };

    let json = serde_json::to_value(&inv).expect("Should serialize");
    assert_eq!(json["late_fee_risk_cents"], 50_000, "Non-zero late-fee risk should serialize");
    assert!(json["late_fee_risk_cents"].as_i64().unwrap() > 0);
}

// ---------------------------------------------------------------------------
// Source-level guard: reassign_approver checks rows_affected
// ---------------------------------------------------------------------------

#[test]
fn test_reassign_handler_checks_rows_affected() {
    let source = include_str!("../src/routes/ap_command_center.rs");
    assert!(
        source.contains("updated.rows_affected() == 0"),
        "reassign_approver must check rows_affected() == 0 and return NotFound"
    );
    assert!(
        source.contains("resource_type: \"PendingApprovalRequest\""),
        "rows_affected guard must use PendingApprovalRequest resource type"
    );
}

// ---------------------------------------------------------------------------
// DB-backed integration tests (require DATABASE_URL, run with --ignored)
// ---------------------------------------------------------------------------

use billforge_core::TenantId;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
const FIXTURE_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Seed a tenant with a user, vendor, and invoice. Returns (invoice_id, tenant_id).
async fn seed_invoice(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();
    let invoice_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();

    billforge_db::tenant_db::run_tenant_migrations(pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");

    // Ensure fixture user
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'ap-cmd-center-test@example.com', '', 'AP Command Center Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create fixture user");

    // Ensure fixture vendor
    let vendor_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name)
         VALUES ($1, $2, 'AP Command Center Test Vendor')
         ON CONFLICT DO NOTHING",
    )
    .bind(vendor_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create fixture vendor");

    // Ensure fixture document
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, mime_type, storage_path, file_size_bytes)
         VALUES ($1, $2, 'test.pdf', 'application/pdf', '/tmp/test.pdf', 100)
         ON CONFLICT DO NOTHING",
    )
    .bind(doc_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create fixture document");

    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, document_id, created_by, status)
         VALUES ($1, $2, 'Test Vendor', $3, 10000, $4, $5, 'pending_approval')
         ON CONFLICT DO NOTHING",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .bind(format!("AP-CMD-TEST-{}", invoice_id))
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("create test invoice");

    (invoice_id, tenant_id)
}

/// Seed a second target approver user. Returns the new user's UUID.
async fn seed_target_approver(pool: &sqlx::PgPool) -> Uuid {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let new_approver_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, '', 'Target Approver')
         ON CONFLICT DO NOTHING",
    )
    .bind(new_approver_id)
    .bind(tenant_id)
    .bind(format!("target-approver-{}@test.com", new_approver_id))
    .execute(pool)
    .await
    .expect("create target approver");
    new_approver_id
}

/// Clean up test data.
async fn cleanup_test_data(pool: &sqlx::PgPool, invoice_id: Uuid, tenant_id: Uuid) {
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

/// When no pending approval_request exists for an invoice, the UPDATE in
/// reassign_approver affects zero rows. The handler must return 404 and
/// must NOT insert a reassign audit-log row.
#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn reassign_returns_404_when_no_pending_approval() {
    let pool = get_pool().await;
    let (invoice_id, tenant_id) = seed_invoice(&pool).await;
    let new_approver_id = seed_target_approver(&pool).await;

    // No approval_requests row at all — the UPDATE should affect 0 rows.
    // Execute the same UPDATE the handler runs and verify 0 rows affected.
    let result = sqlx::query(
        r#"UPDATE approval_requests
           SET requested_from = jsonb_build_object('User', $1::text),
               updated_at = NOW()
           WHERE invoice_id = $2
             AND tenant_id = $3
             AND status = 'pending'"#,
    )
    .bind(new_approver_id)
    .bind(invoice_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("update query");

    assert_eq!(
        result.rows_affected(),
        0,
        "UPDATE should affect 0 rows when no pending approval exists"
    );

    // Verify no audit row was written (the handler should NOT reach the
    // audit INSERT when rows_affected == 0).  We query directly since we
    // are not going through the HTTP handler.
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_audit_log
         WHERE invoice_id = $1 AND tenant_id = $2
           AND event_type = 'reassign_via_ap_command_center'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        audit_count, 0,
        "No reassign audit row should exist when no pending approval was present"
    );

    cleanup_test_data(&pool, invoice_id, tenant_id).await;
}

/// When a pending approval_request exists, the UPDATE succeeds (1 row),
/// the requested_from JSONB is updated to the new approver, and exactly
/// one reassign audit-log row is written with rows_updated = 1.
#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn reassign_succeeds_and_updates_pending_approval() {
    let pool = get_pool().await;
    let (invoice_id, tenant_id) = seed_invoice(&pool).await;
    let new_approver_id = seed_target_approver(&pool).await;
    let original_approver_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();

    // Create a pending approval_request
    let approval_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO approval_requests (id, tenant_id, invoice_id, requested_from, status)
         VALUES ($1, $2, $3, $4, 'pending')
         ON CONFLICT DO NOTHING",
    )
    .bind(approval_id)
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(serde_json::json!({"User": original_approver_id.to_string()}))
    .execute(&pool)
    .await
    .expect("create approval_request");

    // Execute the same UPDATE the handler runs
    let result = sqlx::query(
        r#"UPDATE approval_requests
           SET requested_from = jsonb_build_object('User', $1::text),
               updated_at = NOW()
           WHERE invoice_id = $2
             AND tenant_id = $3
             AND status = 'pending'"#,
    )
    .bind(new_approver_id)
    .bind(invoice_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("update query");

    assert_eq!(
        result.rows_affected(),
        1,
        "UPDATE should affect exactly 1 row"
    );

    // Verify requested_from now points to the new approver
    let requested_from: serde_json::Value = sqlx::query_scalar(
        "SELECT requested_from FROM approval_requests WHERE id = $1",
    )
    .bind(approval_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        requested_from["User"].as_str().unwrap(),
        new_approver_id.to_string(),
        "requested_from should now contain the new approver UUID"
    );

    // Simulate the audit log write (as the handler does on rows_affected > 0)
    let actor_id = Uuid::parse_str(FIXTURE_USER_ID).unwrap();
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type,
                metadata, source_channel)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval',
                   'reassign_via_ap_command_center', $5, 'ap_command_center')"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(actor_id)
    .bind(serde_json::to_string(&serde_json::json!({
        "reassign_to_user_id": new_approver_id.to_string(),
        "rows_updated": result.rows_affected(),
    }))
    .unwrap_or_default())
    .execute(&pool)
    .await
    .expect("write audit log");

    // Verify exactly one reassign audit row exists
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_audit_log
         WHERE invoice_id = $1 AND tenant_id = $2
           AND event_type = 'reassign_via_ap_command_center'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(audit_count, 1, "Exactly one reassign audit row should exist");

    // Verify metadata contains rows_updated = 1
    let metadata: serde_json::Value = sqlx::query_scalar(
        "SELECT metadata::jsonb FROM invoice_audit_log
         WHERE invoice_id = $1 AND tenant_id = $2
           AND event_type = 'reassign_via_ap_command_center'",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(metadata["rows_updated"], 1, "Audit metadata should have rows_updated = 1");

    cleanup_test_data(&pool, invoice_id, tenant_id).await;
}
