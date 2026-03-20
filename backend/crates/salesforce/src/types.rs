//! Salesforce data types
//!
//! Maps to Salesforce REST API objects (v59.0+).
//! Primary objects for AP workflow integration:
//! - Account → Vendor master data
//! - Contact → Vendor contacts
//! - Opportunity → PO linkage
//! - Custom objects for payment tracking

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ──────────────────────────── Auth Tokens ────────────────────────────

/// Salesforce OAuth tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesforceTokens {
    /// Access token
    pub access_token: String,
    /// Refresh token (only on initial auth, not on refresh)
    pub refresh_token: Option<String>,
    /// Instance URL (e.g. "https://na1.salesforce.com")
    pub instance_url: String,
    /// Token type
    pub token_type: Option<String>,
    /// Issued at timestamp
    pub issued_at: Option<String>,
    /// ID URL
    pub id: Option<String>,
    /// Signature
    pub signature: Option<String>,
}

// ──────────────────────────── Accounts (Vendor Master) ────────────────────────────

/// Salesforce Account (maps to BillForge vendor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesforceAccount {
    /// Salesforce Account ID (18-char)
    #[serde(rename = "Id")]
    pub id: String,
    /// Account name (company name)
    #[serde(rename = "Name")]
    pub name: String,
    /// Account type (e.g. "Vendor", "Partner", "Customer")
    #[serde(rename = "Type")]
    pub account_type: Option<String>,
    /// Industry
    #[serde(rename = "Industry")]
    pub industry: Option<String>,
    /// Website
    #[serde(rename = "Website")]
    pub website: Option<String>,
    /// Phone
    #[serde(rename = "Phone")]
    pub phone: Option<String>,
    /// Billing street
    #[serde(rename = "BillingStreet")]
    pub billing_street: Option<String>,
    /// Billing city
    #[serde(rename = "BillingCity")]
    pub billing_city: Option<String>,
    /// Billing state
    #[serde(rename = "BillingState")]
    pub billing_state: Option<String>,
    /// Billing postal code
    #[serde(rename = "BillingPostalCode")]
    pub billing_postal_code: Option<String>,
    /// Billing country
    #[serde(rename = "BillingCountry")]
    pub billing_country: Option<String>,
    /// Annual revenue
    #[serde(rename = "AnnualRevenue")]
    pub annual_revenue: Option<f64>,
    /// Number of employees
    #[serde(rename = "NumberOfEmployees")]
    pub number_of_employees: Option<i32>,
    /// Description
    #[serde(rename = "Description")]
    pub description: Option<String>,
    /// Owner ID
    #[serde(rename = "OwnerId")]
    pub owner_id: Option<String>,
    /// Active status (custom field — may not exist in all orgs)
    #[serde(rename = "Active__c")]
    pub active: Option<String>,
    /// Vendor ID in ERP (custom field)
    #[serde(rename = "Vendor_ID__c")]
    pub vendor_id_custom: Option<String>,
    /// Payment terms (custom field)
    #[serde(rename = "Payment_Terms__c")]
    pub payment_terms: Option<String>,
    /// Last modified date
    #[serde(rename = "LastModifiedDate")]
    pub last_modified_date: Option<String>,
    /// Created date
    #[serde(rename = "CreatedDate")]
    pub created_date: Option<String>,
}

// ──────────────────────────── Contacts ────────────────────────────

/// Salesforce Contact (vendor contact person)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesforceContact {
    /// Salesforce Contact ID
    #[serde(rename = "Id")]
    pub id: String,
    /// First name
    #[serde(rename = "FirstName")]
    pub first_name: Option<String>,
    /// Last name
    #[serde(rename = "LastName")]
    pub last_name: String,
    /// Email
    #[serde(rename = "Email")]
    pub email: Option<String>,
    /// Phone
    #[serde(rename = "Phone")]
    pub phone: Option<String>,
    /// Title
    #[serde(rename = "Title")]
    pub title: Option<String>,
    /// Account ID (parent account)
    #[serde(rename = "AccountId")]
    pub account_id: Option<String>,
    /// Department
    #[serde(rename = "Department")]
    pub department: Option<String>,
}

// ──────────────────────────── Opportunities (PO Linkage) ────────────────────────────

/// Salesforce Opportunity (for PO number linkage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesforceOpportunity {
    /// Salesforce Opportunity ID
    #[serde(rename = "Id")]
    pub id: String,
    /// Opportunity name
    #[serde(rename = "Name")]
    pub name: String,
    /// Account ID
    #[serde(rename = "AccountId")]
    pub account_id: Option<String>,
    /// Stage name
    #[serde(rename = "StageName")]
    pub stage_name: String,
    /// Amount
    #[serde(rename = "Amount")]
    pub amount: Option<f64>,
    /// Close date
    #[serde(rename = "CloseDate")]
    pub close_date: Option<String>,
    /// PO Number (custom field)
    #[serde(rename = "PO_Number__c")]
    pub po_number: Option<String>,
    /// Is Won
    #[serde(rename = "IsWon")]
    pub is_won: Option<bool>,
    /// Is Closed
    #[serde(rename = "IsClosed")]
    pub is_closed: Option<bool>,
}

// ──────────────────────────── SOQL Query Response ────────────────────────────

/// Salesforce SOQL query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesforceQueryResponse<T> {
    /// Total number of records
    #[serde(rename = "totalSize")]
    pub total_size: i32,
    /// Whether the query is done (or has more pages)
    pub done: bool,
    /// Next records URL (for pagination)
    #[serde(rename = "nextRecordsUrl")]
    pub next_records_url: Option<String>,
    /// Records
    pub records: Vec<T>,
}

// ──────────────────────────── Connection Storage ────────────────────────────

/// Salesforce connection storage (persisted to database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesforceConnectionStorage {
    /// Tenant ID
    pub tenant_id: String,
    /// Salesforce instance URL
    pub instance_url: String,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Access token expiry (Salesforce tokens last ~2 hours)
    pub access_token_expires_at: DateTime<Utc>,
    /// Salesforce org ID
    pub org_id: Option<String>,
    /// Whether sync is enabled
    pub sync_enabled: bool,
    /// Last successful sync timestamp
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}

// ──────────────────────────── Sync Result ────────────────────────────

/// Result of a sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// Number of records imported (new)
    pub imported: u64,
    /// Number of records updated
    pub updated: u64,
    /// Number of records skipped
    pub skipped: u64,
    /// Errors encountered
    pub errors: Vec<String>,
}
