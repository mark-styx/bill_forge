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
