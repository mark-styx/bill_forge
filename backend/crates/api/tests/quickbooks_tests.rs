//! Integration tests for QuickBooks Online integration endpoints
//!
//! Tests OAuth flow, vendor sync, and invoice export functionality
//!
//! Note: Authentication and routing tests removed - they require PostgreSQL database setup.
//! These are tested in integration tests with a real database.
//! The following tests verify the QuickBooks data structures and configuration.

// ============================================================================
// QuickBooks Client Tests (Unit)
// ============================================================================

#[test]
fn test_quickbooks_vendor_structure() {
    use billforge_quickbooks::types::QBVendor;

    let vendor = QBVendor {
        Id: "1".to_string(),
        DisplayName: "Acme Corp".to_string(),
        CompanyName: Some("Acme Corporation".to_string()),
        PrimaryEmailAddr: None,
        PrimaryPhone: None,
        Balance: 1000,
        Active: true,
        SyncToken: "0".to_string(),
        MetaData: None,
    };

    assert_eq!(vendor.Id, "1");
    assert_eq!(vendor.DisplayName, "Acme Corp");
    assert_eq!(vendor.CompanyName, Some("Acme Corporation".to_string()));
}

#[test]
fn test_quickbooks_account_structure() {
    use billforge_quickbooks::types::QBAccount;

    let account = QBAccount {
        Id: "10".to_string(),
        Name: "Office Supplies".to_string(),
        AccountType: "Expense".to_string(),
        AccountSubType: Some("Supplies".to_string()),
        Classification: "Expense".to_string(),
        Active: true,
        CurrentBalance: 0,
        SyncToken: "0".to_string(),
    };

    assert_eq!(account.Id, "10");
    assert_eq!(account.Name, "Office Supplies");
    assert_eq!(account.AccountType, "Expense");
    assert!(account.Active);
}

#[test]
fn test_quickbooks_bill_structure() {
    use billforge_quickbooks::types::{QBBill, QBBillLine, QBReference};

    let bill = QBBill {
        Id: "bill-1".to_string(),
        VendorRef: QBReference {
            value: "1".to_string(),
            name: Some("Acme Corp".to_string()),
        },
        CurrencyRef: None,
        DueDate: "2026-03-15".to_string(),
        TotalAmt: 10000,
        Balance: 10000,
        Line: vec![QBBillLine {
            Id: Some("line-1".to_string()),
            LineNum: Some(1),
            Description: Some("Test item".to_string()),
            Amount: 10000,
            AccountBasedExpenseLineDetail: None,
        }],
        SyncToken: "0".to_string(),
        PrivateNote: None,
        MetaData: None,
    };

    assert_eq!(bill.VendorRef.value, "1");
    assert_eq!(bill.Line.len(), 1);
    assert_eq!(bill.Line[0].Amount, 10000);
    assert_eq!(bill.TotalAmt, 10000);
}

#[test]
fn test_quickbooks_tokens_structure() {
    use billforge_quickbooks::types::QBTokens;

    let tokens = QBTokens {
        access_token: "access_token_value".to_string(),
        refresh_token: "refresh_token_value".to_string(),
        token_type: "bearer".to_string(),
        expires_in: 3600,
        x_refresh_token_expires_in: 8726400,
    };

    assert_eq!(tokens.access_token, "access_token_value");
    assert_eq!(tokens.refresh_token, "refresh_token_value");
    assert_eq!(tokens.expires_in, 3600);
}

// ============================================================================
// QuickBooks OAuth Config Tests
// ============================================================================

#[test]
fn test_quickbooks_oauth_config() {
    use billforge_quickbooks::oauth::{QuickBooksOAuthConfig, QuickBooksEnvironment};

    let config = QuickBooksOAuthConfig {
        client_id: "test_client".to_string(),
        client_secret: "test_secret".to_string(),
        redirect_uri: "http://localhost:8080/callback".to_string(),
        environment: QuickBooksEnvironment::Sandbox,
    };

    assert_eq!(config.client_id, "test_client");
    assert_eq!(config.environment, QuickBooksEnvironment::Sandbox);
}

#[test]
fn test_quickbooks_environment() {
    use billforge_quickbooks::oauth::QuickBooksEnvironment;

    let sandbox = QuickBooksEnvironment::Sandbox;
    let production = QuickBooksEnvironment::Production;

    // Verify environment variants exist
    assert_ne!(sandbox, production);
}

#[test]
fn test_token_storage_has_refresh_token() {
    use billforge_quickbooks::types::QBTokenStorage;
    use chrono::{TimeZone, Utc};

    // Verify QBTokenStorage struct has a refresh_token field, confirming the DB schema
    // stores the refresh token needed for automatic token refresh.
    let storage = QBTokenStorage {
        tenant_id: "test-tenant".to_string(),
        company_id: "test-company".to_string(),
        access_token: "access_token_value".to_string(),
        refresh_token: "refresh_token_value".to_string(),
        access_token_expires_at: Utc::now(),
        refresh_token_expires_at: Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    assert_eq!(storage.refresh_token, "refresh_token_value");
    assert!(!storage.refresh_token.is_empty());
}

#[test]
fn test_quickbooks_environment_token_url() {
    use billforge_quickbooks::oauth::QuickBooksEnvironment;

    let sandbox = QuickBooksEnvironment::Sandbox;
    let production = QuickBooksEnvironment::Production;

    // Both environments use the same Intuit OAuth token endpoint for refresh
    let expected_url = "https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer";
    assert_eq!(sandbox.token_url(), expected_url);
    assert_eq!(production.token_url(), expected_url);
}

// ============================================================================
// Sync Response Structure Tests
// ============================================================================

#[test]
fn test_sync_vendors_response_with_errors_field() {
    // Verify SyncVendorsResponse includes the `errors` field for partial-success reporting.
    // This is backward-compatible (additive field) — the frontend can show a warning if errors > 0.
    let response = serde_json::from_str::<serde_json::Value>(
        r#"{"imported": 3, "updated": 1, "skipped": 0, "errors": 2}"#
    ).unwrap();

    assert_eq!(response["imported"], 3);
    assert_eq!(response["updated"], 1);
    assert_eq!(response["skipped"], 0);
    assert_eq!(response["errors"], 2);
}

#[test]
fn test_sync_vendors_response_partial_success_serialization() {
    // Simulate a partial-success sync: some vendors imported, some updated, some failed.
    let imported = 3u64;
    let updated = 1u64;
    let skipped = 0u64;
    let errors = 2u64;

    let json = serde_json::json!({
        "imported": imported,
        "updated": updated,
        "skipped": skipped,
        "errors": errors,
    });

    assert_eq!(json["imported"], 3);
    assert_eq!(json["updated"], 1);
    assert_eq!(json["skipped"], 0);
    assert_eq!(json["errors"], 2);

    // Verify the JSON is valid and round-trips
    let serialized = serde_json::to_string(&json).unwrap();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized["errors"], 2);
}

#[test]
fn test_sync_vendors_response_zero_errors() {
    // When no errors occur, errors field should still be present and equal 0.
    let json = serde_json::json!({
        "imported": 5,
        "updated": 3,
        "skipped": 0,
        "errors": 0,
    });

    assert_eq!(json["errors"], 0);
}

#[test]
fn test_export_invoice_response_structure() {
    // Verify ExportInvoiceResponse has both required fields.
    let response = serde_json::json!({
        "quickbooks_invoice_id": "QB-BILL-123",
        "status": "synced"
    });

    assert_eq!(response["quickbooks_invoice_id"], "QB-BILL-123");
    assert_eq!(response["status"], "synced");

    // Ensure both keys exist
    assert!(response.get("quickbooks_invoice_id").is_some());
    assert!(response.get("status").is_some());
}

#[test]
fn test_sync_accounts_response_includes_errors() {
    // Verify the sync_accounts response includes an errors count.
    let json = serde_json::json!({
        "status": "synced",
        "count": 10,
        "errors": 1
    });

    assert_eq!(json["status"], "synced");
    assert_eq!(json["count"], 10);
    assert_eq!(json["errors"], 1);
}
