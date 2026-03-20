//! Bill.com data types
//!
//! Types map to Bill.com REST API objects (JSON with camelCase fields).
//! Bill.com uses camelCase in its API, so we use serde rename_all
//! to keep Rust fields in snake_case while serializing correctly.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ──────────────────────────── Vendors ────────────────────────────

/// Bill.com vendor record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComVendor {
    /// Bill.com vendor ID
    pub id: Option<String>,
    /// Vendor name
    pub name: String,
    /// Street address
    pub street: Option<String>,
    /// City
    pub city: Option<String>,
    /// State / region
    pub state: Option<String>,
    /// Zip / postal code
    pub zip: Option<String>,
    /// Country
    pub country: Option<String>,
    /// Contact email
    pub email: Option<String>,
    /// Contact phone
    pub phone: Option<String>,
    /// Preferred payment method (ACH, Check, VirtualCard)
    pub payment_method: Option<String>,
    /// Bank account number (for ACH payments)
    pub account_number: Option<String>,
    /// Bank routing number (for ACH payments)
    pub routing_number: Option<String>,
    /// Whether to combine multiple bills into one payment
    pub combine_payments: Option<bool>,
    /// Status (Active / Inactive)
    pub status: Option<String>,
}

// ──────────────────────────── Bills (AP Invoices) ────────────────────────────

/// Bill.com bill (AP invoice)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComBill {
    /// Bill.com bill ID (assigned on create)
    pub id: Option<String>,
    /// Vendor ID
    pub vendor_id: String,
    /// Invoice number
    pub invoice_number: Option<String>,
    /// Invoice date
    pub invoice_date: NaiveDate,
    /// Due date
    pub due_date: Option<NaiveDate>,
    /// Total amount
    pub amount: f64,
    /// Description / memo
    pub description: Option<String>,
    /// Line items
    pub line_items: Vec<BillComBillLine>,
    /// Status (Open, PartiallyPaid, Paid, Void, Scheduled)
    pub status: Option<String>,
    /// Created time
    pub created_time: Option<DateTime<Utc>>,
    /// Updated time
    pub updated_time: Option<DateTime<Utc>>,
}

/// Bill.com bill line item
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComBillLine {
    /// Line ID (assigned on create)
    pub id: Option<String>,
    /// Line amount
    pub amount: f64,
    /// Line description
    pub description: Option<String>,
    /// Chart of account ID
    pub chart_of_account_id: Option<String>,
    /// Department ID
    pub department_id: Option<String>,
    /// Location ID
    pub location_id: Option<String>,
    /// Class ID
    pub class_id: Option<String>,
}

// ──────────────────────────── Payments ────────────────────────────

/// Bill.com payment record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComPayment {
    /// Payment ID (assigned on create)
    pub id: Option<String>,
    /// Vendor ID
    pub vendor_id: String,
    /// Bill ID being paid
    pub bill_id: String,
    /// Payment amount
    pub amount: f64,
    /// Scheduled process date
    pub process_date: NaiveDate,
    /// Status (Scheduled, Sent, Paid, Voided, Failed)
    pub status: Option<String>,
    /// Disbursement type (ACH, Check, VirtualCard)
    pub disbursement_type: Option<String>,
    /// Funding account ID
    pub funding_account: Option<String>,
    /// Confirmation number (assigned after processing)
    pub confirmation_number: Option<String>,
    /// Transaction number (assigned after processing)
    pub transaction_number: Option<String>,
    /// Created time
    pub created_time: Option<DateTime<Utc>>,
}

// ──────────────────────────── Funding Accounts ────────────────────────────

/// Bill.com funding account (bank account for payments)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComFundingAccount {
    /// Funding account ID
    pub id: String,
    /// Account type (BankAccount, Wallet)
    pub account_type: String,
    /// Account display name
    pub name: String,
    /// Bank name
    pub bank_name: Option<String>,
    /// Last four digits of account number
    pub last_four: Option<String>,
}

// ──────────────────────────── Bulk Payment ────────────────────────────

/// Bill.com bulk payment request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComBulkPaymentRequest {
    /// Scheduled process date for all payments
    pub process_date: NaiveDate,
    /// Funding account ID
    pub funding_account: String,
    /// Individual payment items
    pub payments: Vec<BillComBulkPaymentItem>,
}

/// Individual item within a bulk payment request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComBulkPaymentItem {
    /// Bill ID to pay
    pub bill_id: String,
    /// Payment amount
    pub amount: f64,
}

// ──────────────────────────── API Response Wrappers ────────────────────────────

/// Bill.com list response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComListResponse<T> {
    /// Records returned
    pub data: Vec<T>,
    /// Total number of records matching the query
    pub total_count: i32,
    /// Whether more records are available
    pub has_more: bool,
}

// ──────────────────────────── Session ────────────────────────────

/// Bill.com session (returned from login)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillComSession {
    /// Session ID for API calls
    pub session_id: String,
    /// Organization ID
    pub org_id: String,
    /// User ID
    pub user_id: String,
}

// ──────────────────────────── Connection Storage ────────────────────────────

/// Bill.com connection storage (persisted to database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillComConnectionStorage {
    /// BillForge tenant ID
    pub tenant_id: String,
    /// Bill.com organization ID
    pub org_id: String,
    /// Developer key
    pub dev_key: String,
    /// User name (email)
    pub user_name: String,
    /// Encrypted password
    pub password_encrypted: String,
    /// Environment (production / sandbox)
    pub environment: String,
    /// Whether sync is enabled
    pub sync_enabled: bool,
    /// Last successful sync timestamp
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}
