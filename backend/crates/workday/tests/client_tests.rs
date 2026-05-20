//! Baseline smoke tests for the Workday client.
//!
//! These verify client construction and that public methods are wired up.
//! Full HTTP-level testing with wiremock is a follow-up.

use billforge_workday::WorkdayClient;
use chrono::NaiveDate;

// ──────────────────────────── Helpers ────────────────────────────

fn make_client() -> WorkdayClient {
    WorkdayClient::new(
        "test-token".to_string(),
        "https://impl.workday.com".to_string(),
        "acme_corp".to_string(),
    )
}

// ──────────────────────────── Construction ────────────────────────────

#[test]
fn test_client_construction() {
    let _client = make_client();
}

// ──────────────────────────── Smoke tests (no server) ────────────────────────────

#[tokio::test]
async fn test_query_suppliers_without_server_errors() {
    let client = make_client();
    let result = client.query_suppliers(0, 10).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_supplier_without_server_errors() {
    let client = make_client();
    let result = client.get_supplier("supplier-123").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_supplier_invoice_without_server_errors() {
    use billforge_workday::{WorkdayInvoiceLine, WorkdaySupplierInvoice};

    let client = make_client();
    let invoice = WorkdaySupplierInvoice {
        id: None,
        invoice_number: "INV-001".to_string(),
        supplier_id: "supplier-123".to_string(),
        invoice_date: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
        due_date: None,
        total_amount: 100.0,
        currency: Some("USD".to_string()),
        memo: None,
        lines: vec![WorkdayInvoiceLine {
            line_number: 1,
            amount: 100.0,
            memo: None,
            spend_category: None,
            ledger_account: None,
            cost_center: None,
            project: None,
        }],
        status: None,
        company_reference: None,
    };
    let result = client.create_supplier_invoice(&invoice).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_worker_info_without_server_errors() {
    let client = make_client();
    let result = client.get_worker_info().await;
    assert!(result.is_err());
}
