//! Vendor domain model

use crate::types::TenantId;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// Unique identifier for a vendor
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VendorId(pub Uuid);

impl VendorId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for VendorId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for VendorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for VendorId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Vendor status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorStatus {
    /// Vendor is active and can receive payments
    Active,
    /// Vendor is temporarily on hold
    OnHold,
    /// Vendor is inactive/archived
    Inactive,
    /// Pending initial setup/verification
    Pending,
}

/// Type of vendor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorType {
    /// Regular business vendor
    Business,
    /// Individual contractor (1099)
    Contractor,
    /// Employee reimbursement
    Employee,
    /// Government entity
    Government,
    /// Non-profit organization
    NonProfit,
}

/// Core vendor entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vendor {
    pub id: VendorId,
    pub tenant_id: TenantId,
    
    // Basic information
    pub name: String,
    pub legal_name: Option<String>,
    pub vendor_type: VendorType,
    pub status: VendorStatus,
    
    // Contact information
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    
    // Address
    pub address: Option<VendorAddress>,
    
    // Tax information
    pub tax_id: Option<String>,
    pub tax_id_type: Option<TaxIdType>,
    pub w9_on_file: bool,
    pub w9_received_date: Option<NaiveDate>,
    
    // Payment information
    pub payment_terms: Option<String>,
    pub default_payment_method: Option<PaymentMethod>,
    pub bank_account: Option<BankAccount>,
    
    // Internal tracking
    pub vendor_code: Option<String>,
    pub default_gl_code: Option<String>,
    pub default_department: Option<String>,
    
    // Communication
    pub primary_contact: Option<VendorContact>,
    pub contacts: Vec<VendorContact>,
    
    // Notes and metadata
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub custom_fields: serde_json::Value,
    
    // Audit
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Vendor address
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VendorAddress {
    pub line1: String,
    pub line2: Option<String>,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

/// Tax ID types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxIdType {
    /// US Employer Identification Number
    Ein,
    /// US Social Security Number
    Ssn,
    /// Individual Taxpayer Identification Number
    Itin,
    /// Foreign tax ID
    Foreign,
    /// Other
    Other,
}

/// Payment methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Check,
    Ach,
    Wire,
    CreditCard,
    VirtualCard,
}

/// Bank account information (encrypted at rest)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankAccount {
    pub bank_name: String,
    pub account_type: AccountType,
    /// Last 4 digits only for display
    pub account_last_four: String,
    /// Encrypted full account number
    pub account_number_encrypted: String,
    /// Encrypted routing number
    pub routing_number_encrypted: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Checking,
    Savings,
}

/// Vendor contact person
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorContact {
    pub id: Uuid,
    pub name: String,
    pub title: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub is_primary: bool,
}

/// Tax document stored for a vendor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxDocument {
    pub id: Uuid,
    pub vendor_id: VendorId,
    pub tenant_id: TenantId,
    pub document_type: TaxDocumentType,
    pub tax_year: i32,
    pub file_id: Uuid,
    pub file_name: String,
    pub received_date: NaiveDate,
    pub expires_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxDocumentType {
    W9,
    W8Ben,
    W8BenE,
    Form1099,
    Other,
}

/// Input for creating a vendor
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateVendorInput {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub legal_name: Option<String>,
    pub vendor_type: VendorType,
    #[validate(email)]
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub address: Option<VendorAddress>,
    pub tax_id: Option<String>,
    pub tax_id_type: Option<TaxIdType>,
    pub payment_terms: Option<String>,
    pub default_payment_method: Option<PaymentMethod>,
    pub vendor_code: Option<String>,
    pub default_gl_code: Option<String>,
    pub default_department: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

/// Input for updating a vendor
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateVendorInput {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub legal_name: Option<String>,
    pub vendor_type: Option<VendorType>,
    pub status: Option<VendorStatus>,
    #[validate(email)]
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub address: Option<VendorAddress>,
    pub tax_id: Option<String>,
    pub tax_id_type: Option<TaxIdType>,
    pub payment_terms: Option<String>,
    pub default_payment_method: Option<PaymentMethod>,
    pub vendor_code: Option<String>,
    pub default_gl_code: Option<String>,
    pub default_department: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Filters for querying vendors
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VendorFilters {
    pub status: Option<VendorStatus>,
    pub vendor_type: Option<VendorType>,
    pub search: Option<String>,
    pub tags: Vec<String>,
    pub has_w9: Option<bool>,
}

/// Vendor communication message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorMessage {
    pub id: Uuid,
    pub vendor_id: VendorId,
    pub tenant_id: TenantId,
    pub subject: String,
    pub body: String,
    pub sender_type: MessageSender,
    pub sender_id: Option<Uuid>,
    pub sender_name: String,
    pub attachments: Vec<MessageAttachment>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageSender {
    Internal,
    Vendor,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachment {
    pub id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub mime_type: String,
}
