//! Sage Intacct data types
//!
//! Types map to Sage Intacct Web Services API objects.
//! Sage uses XML, so these structs are used for internal representation
//! and are converted to/from XML in the client layer.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ──────────────────────────── Vendors ────────────────────────────

/// Sage Intacct vendor record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageVendor {
    /// Vendor ID (RECORDNO in Sage)
    pub record_no: String,
    /// Vendor ID / code
    pub vendor_id: String,
    /// Vendor name
    pub name: String,
    /// Display contact name
    pub display_contact: Option<SageContact>,
    /// Status (active / inactive)
    pub status: String,
    /// Vendor type ID
    pub vendor_type_id: Option<String>,
    /// Default expense GL account
    pub default_expense_gl_account: Option<String>,
    /// Tax ID (EIN/SSN)
    pub tax_id: Option<String>,
    /// 1099 eligible
    pub form_1099_eligible: bool,
    /// Payment term
    pub payment_term: Option<String>,
    /// Currency code (e.g. "USD")
    pub currency: Option<String>,
}

/// Sage Intacct contact information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageContact {
    /// Contact name
    pub contact_name: Option<String>,
    /// Primary email
    pub email: Option<String>,
    /// Primary phone
    pub phone: Option<String>,
    /// Mailing address
    pub address: Option<SageAddress>,
}

/// Sage Intacct address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageAddress {
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub zip: Option<String>,
    pub country: Option<String>,
}

// ──────────────────────────── GL Accounts ────────────────────────────

/// Sage Intacct GL account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageGLAccount {
    /// Account number
    pub account_no: String,
    /// Account title
    pub title: String,
    /// Account type (e.g. "incomestatement", "balancesheet")
    pub account_type: String,
    /// Normal balance (debit / credit)
    pub normal_balance: String,
    /// Category / classification
    pub category: Option<String>,
    /// Status (active / inactive)
    pub status: String,
    /// Department restrictions (if dimensional)
    pub department: Option<String>,
    /// Location restrictions (if multi-entity)
    pub location: Option<String>,
}

// ──────────────────────────── AP Bills (Purchasing Transactions) ────────────────────────────

/// Sage Intacct AP bill (purchasing transaction)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageAPBill {
    /// Record number (assigned by Sage on create)
    pub record_no: Option<String>,
    /// Transaction type — "Vendor Invoice" for AP bills
    pub transaction_type: String,
    /// Vendor ID
    pub vendor_id: String,
    /// Transaction date
    pub date_created: NaiveDate,
    /// Due date
    pub date_due: Option<NaiveDate>,
    /// Document number (invoice number)
    pub document_number: Option<String>,
    /// Reference number (PO number)
    pub reference_number: Option<String>,
    /// Description / memo
    pub description: Option<String>,
    /// Currency code
    pub currency: Option<String>,
    /// Exchange rate type
    pub exchange_rate_type: Option<String>,
    /// Line items
    pub lines: Vec<SageAPBillLine>,
    /// State (Draft, Pending, Posted)
    pub state: Option<String>,
    /// Total amount (calculated from lines)
    pub total_amount: Option<f64>,
    /// Location ID (for multi-entity)
    pub location_id: Option<String>,
    /// Department ID
    pub department_id: Option<String>,
}

/// Sage Intacct AP bill line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageAPBillLine {
    /// GL account number
    pub gl_account_no: String,
    /// Transaction amount
    pub amount: f64,
    /// Line memo
    pub memo: Option<String>,
    /// Department ID
    pub department_id: Option<String>,
    /// Location ID
    pub location_id: Option<String>,
    /// Project ID (for project tracking)
    pub project_id: Option<String>,
    /// Class ID (custom dimension)
    pub class_id: Option<String>,
}

// ──────────────────────────── API Response Wrappers ────────────────────────────

/// Result from a readByQuery operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageQueryResult<T> {
    /// Number of records returned
    pub count: i32,
    /// Total records matching query
    pub total_count: i32,
    /// Number of remaining records
    pub num_remaining: i32,
    /// Result ID for pagination
    pub result_id: Option<String>,
    /// Records
    pub records: Vec<T>,
}

/// Sage Intacct operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageOperationResult {
    /// Status (success / failure)
    pub status: String,
    /// Function name
    pub function: String,
    /// Control ID
    pub control_id: String,
    /// Record key (for create operations)
    pub key: Option<String>,
    /// Error messages (if failed)
    pub errors: Vec<SageError>,
}

/// Sage Intacct error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageError {
    /// Error number
    pub error_no: String,
    /// Error description
    pub description: String,
    /// Error description2 (additional detail)
    pub description2: Option<String>,
    /// Correction suggestion
    pub correction: Option<String>,
}

// ──────────────────────────── Connection Storage ────────────────────────────

/// Sage Intacct connection storage (persisted to database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageIntacctConnection {
    /// Tenant ID
    pub tenant_id: String,
    /// Company ID in Sage Intacct
    pub company_id: String,
    /// Entity ID (for multi-entity)
    pub entity_id: Option<String>,
    /// Sender ID
    pub sender_id: String,
    /// Encrypted sender password
    pub sender_password_encrypted: String,
    /// User ID
    pub user_id: String,
    /// Encrypted user password
    pub user_password_encrypted: String,
    /// Whether sync is enabled
    pub sync_enabled: bool,
    /// Last successful sync timestamp
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
}
