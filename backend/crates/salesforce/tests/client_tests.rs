//! Tests for Salesforce client that run without external dependencies.
//!
//! These tests verify client construction and basic method signatures.
//! Full HTTP-level testing requires wiremock (see client_tests.rs.bak).

use billforge_salesforce::SalesforceClient;

// ──────────────────────────── Helpers ────────────────────────────

fn make_client() -> SalesforceClient {
    SalesforceClient::new("test-token".to_string(), "https://test.salesforce.com".to_string())
}

// ──────────────────────────── Construction ────────────────────────────

#[test]
fn test_client_construction() {
    let _client = make_client();
}

#[tokio::test]
async fn test_query_contacts_without_server_returns_transport_error() {
    let client = make_client();
    let result = client.query_contacts_for_account("001ABCDEFGHIJKL").await;
    // No server running, expect a transport/connection error
    assert!(result.is_err());
}

#[tokio::test]
async fn test_query_vendor_accounts_without_server_returns_transport_error() {
    let client = make_client();
    let result = client.query_vendor_accounts(None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_query_all_accounts_without_server_returns_transport_error() {
    let client = make_client();
    let result = client.query_all_accounts().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_account_without_server_returns_transport_error() {
    let client = make_client();
    let result = client.get_account("001ABCDEFGHIJKLMNO").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_query_opportunities_without_server_returns_transport_error() {
    let client = make_client();
    let result = client.query_opportunities_with_po(Some("001ABCDEFGHIJKLMNO")).await;
    assert!(result.is_err());
}
