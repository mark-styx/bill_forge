//! Workday Financial Management data types
//!
//! Types map to Workday REST API and SOAP Financial Management objects.
//! Workday uses JSON for REST and XML for SOAP — these structs are used
//! for internal representation and JSON serialization via the REST API.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ──────────────────────────── Suppliers ────────────────────────────

/// Workday supplier record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdaySupplier {
    /// Workday internal ID
    pub id: String,
    /// Supplier ID / reference code
    pub supplier_id: String,
    /// Supplier name
    pub supplier_name: String,
    /// Supplier category
    pub supplier_category: Option<String>,
    /// Payment terms
    pub payment_terms: Option<String>,
    /// Status (Active / Inactive)
    pub status: String,
    /// Tax ID (EIN/SSN)
    pub tax_id: Option<String>,
    /// Primary email address
    pub primary_email: Option<String>,
    /// Primary phone number
    pub primary_phone: Option<String>,
    /// Address line 1
    pub address_line_1: Option<String>,
    /// Address line 2
    pub address_line_2: Option<String>,
    /// City
    pub city: Option<String>,
    /// State / region
    pub state: Option<String>,
    /// Postal code
    pub postal_code: Option<String>,
    /// Country code
    pub country: Option<String>,
    /// Currency code (e.g. "USD")
    pub currency: Option<String>,
    /// Default expense account reference
    pub default_expense_account: Option<String>,
}

// ──────────────────────────── Ledger Accounts ────────────────────────────

/// Workday ledger account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdayLedgerAccount {
    /// Ledger account ID
    pub ledger_account_id: String,
    /// Account name
    pub name: String,
    /// Account type (Asset, Liability, Revenue, Expense, Equity)
    pub account_type: String,
    /// Account number
    pub account_number: String,
    /// Status (Active / Inactive)
    pub status: String,
    /// Parent account reference (for hierarchical chart of accounts)
    pub parent_account: Option<String>,
}

// ──────────────────────────── Spend Categories ────────────────────────────

/// Workday spend category (for invoice line categorization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdaySpendCategory {
    /// Spend category ID
    pub id: String,
    /// Category name
    pub name: String,
    /// Category description
    pub description: Option<String>,
    /// Status (Active / Inactive)
    pub status: String,
}

// ──────────────────────────── Supplier Invoices ────────────────────────────

/// Workday supplier invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdaySupplierInvoice {
    /// Workday internal ID (assigned on create)
    pub id: Option<String>,
    /// Invoice number
    pub invoice_number: String,
    /// Supplier ID reference
    pub supplier_id: String,
    /// Invoice date
    pub invoice_date: NaiveDate,
    /// Due date
    pub due_date: Option<NaiveDate>,
    /// Total amount
    pub total_amount: f64,
    /// Currency code
    pub currency: Option<String>,
    /// Memo / description
    pub memo: Option<String>,
    /// Invoice line items
    pub lines: Vec<WorkdayInvoiceLine>,
    /// Status (Draft, InProgress, Approved, Paid)
    pub status: Option<String>,
    /// Company reference (for multi-company)
    pub company_reference: Option<String>,
}

/// Workday supplier invoice line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdayInvoiceLine {
    /// Line number
    pub line_number: i32,
    /// Line amount
    pub amount: f64,
    /// Line memo
    pub memo: Option<String>,
    /// Spend category reference
    pub spend_category: Option<String>,
    /// Ledger account reference
    pub ledger_account: Option<String>,
    /// Cost center reference
    pub cost_center: Option<String>,
    /// Project reference
    pub project: Option<String>,
}

// ──────────────────────────── Companies ────────────────────────────

/// Workday company (for multi-company support)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdayCompany {
    /// Company ID
    pub id: String,
    /// Company name
    pub name: String,
    /// Currency code
    pub currency: Option<String>,
    /// Status (Active / Inactive)
    pub status: String,
}

// ──────────────────────────── API Response Wrappers ────────────────────────────

/// Workday REST API query response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdayQueryResponse<T> {
    /// Total number of records matching the query
    pub total: i32,
    /// Records returned
    pub data: Vec<T>,
}

// ──────────────────────────── OAuth Tokens ────────────────────────────

/// Workday OAuth tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdayTokens {
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Token type (e.g. "Bearer")
    pub token_type: String,
    /// Expires in seconds
    pub expires_in: i64,
}

// ──────────────────────────── Connection Storage ────────────────────────────

/// Workday connection storage (persisted to database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdayConnectionStorage {
    /// BillForge tenant ID
    pub tenant_id: String,
    /// Workday tenant URL (e.g. "https://impl.workday.com")
    pub workday_tenant_url: String,
    /// Workday tenant name (e.g. "acme_corp")
    pub workday_tenant_name: String,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Access token expiry
    pub access_token_expires_at: DateTime<Utc>,
    /// Whether sync is enabled
    pub sync_enabled: bool,
    /// Last successful sync timestamp
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}
