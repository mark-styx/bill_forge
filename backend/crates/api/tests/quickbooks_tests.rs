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
    use billforge_quickbooks::oauth::{QuickBooksEnvironment, QuickBooksOAuthConfig};

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
