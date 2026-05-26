//! Tests for Salesforce client that run without external dependencies.
//!
//! These tests verify input validation (SOQL injection prevention),
//! escape helpers, and basic client construction.

use billforge_salesforce::{escape_soql_literal, validate_sf_id, SalesforceClient};

// ──────────────────────────── Helpers ────────────────────────────

fn make_client() -> SalesforceClient {
    SalesforceClient::new(
        "test-token".to_string(),
        "https://test.salesforce.com".to_string(),
    )
}

// ──────────────────────────── validate_sf_id ────────────────────────────

#[test]
fn test_validate_sf_id_accepts_15_and_18_char_ids() {
    assert!(validate_sf_id("001ABCDEFGHIJKL").is_ok()); // 15 chars
    assert!(validate_sf_id("001ABCDEFGHIJKLMNO").is_ok()); // 18 chars
}

#[test]
fn test_validate_sf_id_rejects_injection() {
    let bad_inputs = [
        "001' OR Id != null--",
        "'; DROP",
        "abc",                 // too short
        "001ABCDEFGHIJKL; x",  // semicolon
        "",                    // empty
        "001ABC DEF123456",    // whitespace
        "001ABCDEFGHIJKL\x00", // null byte (not alphanumeric)
    ];
    for bad in &bad_inputs {
        assert!(
            validate_sf_id(bad).is_err(),
            "expected rejection for {:?}",
            bad
        );
    }
}

// ──────────────────────────── escape_soql_literal ────────────────────────────

#[test]
fn test_escape_soql_literal_escapes_quotes_and_backslashes() {
    assert_eq!(escape_soql_literal("O'Brien"), "O\\'Brien");
    assert_eq!(escape_soql_literal("back\\slash"), "back\\\\slash");
    assert_eq!(escape_soql_literal("has\"quote"), "has\\\"quote");
    assert_eq!(escape_soql_literal("line\nbreak"), "line\\nbreak");
    assert_eq!(escape_soql_literal("carriage\rreturn"), "carriage\\rreturn");
}

#[test]
fn test_escape_soql_literal_is_noop_for_plain_text() {
    assert_eq!(escape_soql_literal("HelloWorld123"), "HelloWorld123");
    assert_eq!(escape_soql_literal(""), "");
}

// ──────────────────────────── Construction ────────────────────────────

#[test]
fn test_client_construction() {
    let _client = make_client();
}

// ──────────────────────────── Integration: injection blocked before HTTP ────────────────────────────

#[tokio::test]
async fn test_query_contacts_for_account_rejects_injection() {
    let client = make_client();
    let result = client.query_contacts_for_account("' OR 1=1--").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid Salesforce ID"),
        "expected validation error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_query_opportunities_with_po_rejects_injection() {
    let client = make_client();
    let result = client.query_opportunities_with_po(Some("' OR 1=1--")).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid Salesforce ID"),
        "expected validation error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_get_account_rejects_injection() {
    let client = make_client();
    let result = client.get_account("'; DROP TABLE Account--").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid Salesforce ID"),
        "expected validation error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_query_vendor_accounts_rejects_injection_filter() {
    let client = make_client();
    let result = client
        .query_vendor_accounts(Some("Name != null'; DROP TABLE Account--"))
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("rejected"),
        "expected validation error, got: {}",
        err
    );
}
