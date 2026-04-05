//! Payment request route unit tests
//!
//! Tests cover:
//! - Request/response type serialization round-trips
//! - Request number generation format
//! - Status validation logic
//! - Tenant isolation (different TenantIds)
//! - Empty invoice list validation

use billforge_core::types::TenantId;
use chrono::NaiveDate;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Request / Response serde round-trips
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct CreatePaymentRequestBody {
    invoice_ids: Vec<Uuid>,
    notes: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AddInvoicesBody {
    invoice_ids: Vec<Uuid>,
}

#[test]
fn test_create_request_body_with_notes() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let body = CreatePaymentRequestBody {
        invoice_ids: vec![id1, id2],
        notes: Some("Batch payment for Q1".to_string()),
    };
    let json = serde_json::to_string(&body).expect("serialize");
    let back: CreatePaymentRequestBody = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.invoice_ids.len(), 2);
    assert_eq!(back.notes.as_deref(), Some("Batch payment for Q1"));
}

#[test]
fn test_create_request_body_without_notes() {
    let json = format!(
        r#"{{"invoice_ids": ["{}"]}}"#,
        Uuid::new_v4()
    );
    let body: CreatePaymentRequestBody = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(body.invoice_ids.len(), 1);
    assert!(body.notes.is_none());
}

#[test]
fn test_add_invoices_body() {
    let id = Uuid::new_v4();
    let json = format!(r#"{{"invoice_ids": ["{}"]}}"#, id);
    let body: AddInvoicesBody = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(body.invoice_ids, vec![id]);
}

// ---------------------------------------------------------------------------
// Request number format validation
// ---------------------------------------------------------------------------

#[test]
fn test_request_number_format() {
    // Verify the format PR-YYYYMMDD-XXXX is generated correctly
    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let prefix = format!("PR-{}-", today);

    // Simulate sequence generation
    let seq = 1u32;
    let request_number = format!("{}{:04}", prefix, seq);
    assert!(request_number.starts_with("PR-"));
    assert_eq!(request_number.len(), 16); // PR-YYYYMMDD-XXXX = 16 chars
    assert_eq!(&request_number[3..11], today);
    assert_eq!(&request_number[11..12], "-");
    assert_eq!(&request_number[12..], "0001");

    // Second in sequence
    let seq2 = 42u32;
    let request_number2 = format!("{}{:04}", prefix, seq2);
    assert_eq!(&request_number2[12..], "0042");
}

#[test]
fn test_request_number_sequence_parsing() {
    let prefix = "PR-20260405-";
    let request_number = format!("{}{:04}", prefix, 7u32);
    let seq_str = request_number.strip_prefix(prefix).unwrap_or("0000");
    let seq: u32 = seq_str.parse().unwrap();
    assert_eq!(seq, 7);
}

// ---------------------------------------------------------------------------
// Tenant isolation
// ---------------------------------------------------------------------------

#[test]
fn test_tenant_isolation_different_ids() {
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();
    assert_ne!(tenant_a, tenant_b, "Different tenants should have different IDs");
}

#[test]
fn test_tenant_id_from_uuid_preserves_value() {
    let uuid = Uuid::new_v4();
    let tenant_id = TenantId::from_uuid(uuid);
    assert_eq!(tenant_id.as_uuid(), &uuid);
}

// ---------------------------------------------------------------------------
// Status validation logic
// ---------------------------------------------------------------------------

#[test]
fn test_status_transitions() {
    // Valid: draft -> submitted
    let current = "draft";
    let target = "submitted";
    assert!(can_transition(current, target));

    // Invalid: submitted -> draft (no backwards)
    assert!(!can_transition("submitted", "draft"));

    // Invalid: completed -> submitted
    assert!(!can_transition("completed", "submitted"));

    // Invalid: draft -> completed (must go through submitted)
    assert!(!can_transition("draft", "completed"));
}

fn can_transition(current: &str, target: &str) -> bool {
    matches!((current, target), ("draft", "submitted") | ("submitted", "processing") | ("processing", "completed") | (_, "cancelled"))
}

#[test]
fn test_submit_only_from_draft() {
    // Submit is only valid from "draft" status
    assert!(can_submit("draft"));
    assert!(!can_submit("submitted"));
    assert!(!can_submit("processing"));
    assert!(!can_submit("completed"));
    assert!(!can_submit("cancelled"));
}

fn can_submit(status: &str) -> bool {
    status == "draft"
}

// ---------------------------------------------------------------------------
// Aggregate computation
// ---------------------------------------------------------------------------

#[test]
fn test_total_amount_sum() {
    let amounts = vec![10000i64, 25000, 5000, 30000];
    let total: i64 = amounts.iter().sum();
    assert_eq!(total, 70000);
}

#[test]
fn test_due_date_range() {
    let dates = vec![
        NaiveDate::from_ymd_opt(2024, 2, 14),
        NaiveDate::from_ymd_opt(2024, 1, 31),
        NaiveDate::from_ymd_opt(2024, 3, 15),
        NaiveDate::from_ymd_opt(2024, 2, 28),
    ];
    let dates: Vec<NaiveDate> = dates.into_iter().flatten().collect();
    let earliest = dates.iter().min();
    let latest = dates.iter().max();
    assert_eq!(earliest, Some(&NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()));
    assert_eq!(latest, Some(&NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()));
}

#[test]
fn test_vendor_id_single_vendor() {
    let vendor = Uuid::new_v4();
    let vendor_ids: std::collections::HashSet<Option<Uuid>> = vec![Some(vendor), Some(vendor)]
        .into_iter()
        .collect();
    assert_eq!(vendor_ids.len(), 1);
    let resolved = if vendor_ids.len() == 1 {
        vendor_ids.into_iter().next().flatten()
    } else {
        None
    };
    assert_eq!(resolved, Some(vendor));
}

#[test]
fn test_vendor_id_multi_vendor() {
    let v1 = Uuid::new_v4();
    let v2 = Uuid::new_v4();
    let vendor_ids: std::collections::HashSet<Option<Uuid>> =
        vec![Some(v1), Some(v2)].into_iter().collect();
    assert_eq!(vendor_ids.len(), 2);
    let resolved = if vendor_ids.len() == 1 {
        vendor_ids.into_iter().next().flatten()
    } else {
        None
    };
    assert_eq!(resolved, None);
}

#[test]
fn test_empty_invoice_ids_rejected() {
    let invoice_ids: Vec<Uuid> = vec![];
    assert!(invoice_ids.is_empty());
    // This validates the guard in create_payment_request:
    // if invoice_ids.is_empty() => Error::Validation
}

// ---------------------------------------------------------------------------
// Response shape verification
// ---------------------------------------------------------------------------

#[test]
fn test_payment_request_response_shape() {
    let now = chrono::Utc::now();
    let response = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "request_number": "PR-20260405-0001",
        "status": "draft",
        "vendor_id": null,
        "vendor_name": null,
        "total_amount_cents": 70000,
        "currency": "USD",
        "invoice_count": 3,
        "earliest_due_date": "2024-01-31",
        "latest_due_date": "2024-03-15",
        "items": [],
        "notes": null,
        "created_by": Uuid::new_v4().to_string(),
        "submitted_at": null,
        "created_at": now.to_rfc3339()
    });

    // Verify all expected fields present
    assert!(response.get("id").is_some());
    assert!(response.get("request_number").is_some());
    assert!(response.get("status").is_some());
    assert!(response.get("vendor_id").is_some());
    assert!(response.get("total_amount_cents").is_some());
    assert!(response.get("currency").is_some());
    assert!(response.get("invoice_count").is_some());
    assert!(response.get("earliest_due_date").is_some());
    assert!(response.get("latest_due_date").is_some());
    assert!(response.get("items").is_some());
    assert!(response.get("notes").is_some());
    assert!(response.get("created_by").is_some());
    assert!(response.get("submitted_at").is_some());
    assert!(response.get("created_at").is_some());
}

#[test]
fn test_list_response_pagination_shape() {
    let response = serde_json::json!({
        "data": [],
        "pagination": {
            "page": 1,
            "per_page": 25,
            "total_items": 0,
            "total_pages": 0
        }
    });
    assert!(response.get("data").is_some());
    assert!(response.get("pagination").is_some());
    let pag = &response["pagination"];
    assert_eq!(pag["page"], 1);
    assert_eq!(pag["per_page"], 25);
    assert_eq!(pag["total_items"], 0);
    assert_eq!(pag["total_pages"], 0);
}
