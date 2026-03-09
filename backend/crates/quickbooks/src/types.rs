//! QuickBooks data types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// QuickBooks vendor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBVendor {
    /// QuickBooks vendor ID
    pub Id: String,
    /// Vendor name
    pub DisplayName: String,
    /// Company name
    pub CompanyName: Option<String>,
    /// Contact email
    pub PrimaryEmailAddr: Option<QBEmailAddress>,
    /// Contact phone
    pub PrimaryPhone: Option<QBPhone>,
    /// Balance
    pub Balance: i64,
    /// Active status
    pub Active: bool,
    /// Sync token for updates
    pub SyncToken: String,
    /// Metadata
    pub MetaData: Option<QBMetaData>,
}

/// QuickBooks email address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBEmailAddress {
    pub Address: String,
}

/// QuickBooks phone number
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBPhone {
    pub FreeFormNumber: String,
}

/// QuickBooks account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBAccount {
    /// QuickBooks account ID
    pub Id: String,
    /// Account name
    pub Name: String,
    /// Account type
    pub AccountType: String,
    /// Account sub-type
    pub AccountSubType: Option<String>,
    /// Classification (Asset, Liability, Equity, Income, Expense)
    pub Classification: String,
    /// Active status
    pub Active: bool,
    /// Current balance
    pub CurrentBalance: i64,
    /// Sync token
    pub SyncToken: String,
}

/// QuickBooks bill (invoice)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBBill {
    /// QuickBooks bill ID
    pub Id: String,
    /// Vendor reference
    pub VendorRef: QBReference,
    /// Currency reference
    pub CurrencyRef: Option<QBReference>,
    /// Due date
    pub DueDate: String,
    /// Total amount
    pub TotalAmt: i64,
    /// Balance due
    pub Balance: i64,
    /// Line items
    pub Line: Vec<QBBillLine>,
    /// Sync token
    pub SyncToken: String,
    /// Private note
    pub PrivateNote: Option<String>,
    /// Metadata
    pub MetaData: Option<QBMetaData>,
}

/// QuickBooks bill line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBBillLine {
    /// Line ID
    pub Id: Option<String>,
    /// Line number
    pub LineNum: Option<i32>,
    /// Description
    pub Description: Option<String>,
    /// Amount
    pub Amount: i64,
    /// Account reference
    pub AccountBasedExpenseLineDetail: Option<QBAccountBasedExpenseLineDetail>,
}

/// Account-based expense line detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBAccountBasedExpenseLineDetail {
    /// Account reference
    pub AccountRef: QBReference,
    /// Billable status
    pub BillableStatus: Option<String>,
    /// Tax code reference
    pub TaxCodeRef: Option<QBReference>,
}

/// QuickBooks reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBReference {
    /// Reference value (ID)
    pub value: String,
    /// Reference name
    pub name: Option<String>,
}

/// QuickBooks metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBMetaData {
    /// Creation time
    pub CreateTime: DateTime<Utc>,
    /// Last updated time
    pub LastUpdatedTime: DateTime<Utc>,
}

/// QuickBooks API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBQueryResponse<T> {
    /// Query response
    pub QueryResponse: Option<QBQueryData<T>>,
}

/// QuickBooks query data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBQueryData<T> {
    /// Results
    #[serde(rename = "T")]
    pub results: Vec<T>,
    /// Start position
    pub startPosition: i32,
    /// Max results
    pub maxResults: i32,
    /// Total count
    pub totalCount: Option<i32>,
}

/// QuickBooks OAuth tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBTokens {
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Token type
    pub token_type: String,
    /// Expires in seconds
    pub expires_in: i64,
    /// Refresh token expires in seconds
    pub x_refresh_token_expires_in: i64,
}

/// QuickBooks token storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QBTokenStorage {
    /// Tenant ID
    pub tenant_id: String,
    /// Company ID
    pub company_id: String,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Access token expiry
    pub access_token_expires_at: DateTime<Utc>,
    /// Refresh token expiry
    pub refresh_token_expires_at: DateTime<Utc>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}
