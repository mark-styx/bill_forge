//! Xero data types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Xero contact (vendor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroContact {
    /// Xero contact ID
    pub ContactID: String,
    /// Contact name
    pub Name: String,
    /// Contact status
    #[serde(rename = "ContactStatus")]
    pub ContactStatus: String,
    /// Email address
    pub EmailAddress: Option<String>,
    /// Phone number
    #[serde(rename = "Phones")]
    pub Phones: Option<Vec<XeroPhone>>,
    /// Addresses
    #[serde(rename = "Addresses")]
    pub Addresses: Option<Vec<XeroAddress>>,
    /// Is supplier
    pub IsSupplier: Option<bool>,
    /// Is customer
    pub IsCustomer: Option<bool>,
    /// Default currency
    pub DefaultCurrency: Option<String>,
    /// Updated date
    pub UpdatedDateUTC: Option<String>,
}

/// Xero phone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroPhone {
    #[serde(rename = "PhoneType")]
    pub PhoneType: String,
    #[serde(rename = "PhoneNumber")]
    pub PhoneNumber: Option<String>,
    #[serde(rename = "PhoneAreaCode")]
    pub PhoneAreaCode: Option<String>,
    #[serde(rename = "PhoneCountryCode")]
    pub PhoneCountryCode: Option<String>,
}

/// Xero address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroAddress {
    #[serde(rename = "AddressType")]
    pub AddressType: String,
    #[serde(rename = "AddressLine1")]
    pub AddressLine1: Option<String>,
    #[serde(rename = "AddressLine2")]
    pub AddressLine2: Option<String>,
    pub City: Option<String>,
    pub Region: Option<String>,
    #[serde(rename = "PostalCode")]
    pub PostalCode: Option<String>,
    pub Country: Option<String>,
}

/// Xero account (chart of accounts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroAccount {
    /// Account ID
    #[serde(rename = "AccountID")]
    pub AccountID: String,
    /// Account code
    pub Code: String,
    /// Account name
    pub Name: String,
    /// Account type
    #[serde(rename = "Type")]
    pub AccountType: String,
    /// Tax type
    #[serde(rename = "TaxType")]
    pub TaxType: Option<String>,
    /// Enable payments to account
    pub EnablePaymentsToAccount: Option<bool>,
    /// Show in expense claims
    pub ShowInExpenseClaims: Option<bool>,
    /// Account class (Asset, Liability, Equity, Revenue, Expense)
    pub Class: String,
    /// System account
    #[serde(rename = "SystemAccount")]
    pub SystemAccount: Option<bool>,
    /// Status
    pub Status: String,
    /// Bank account number
    #[serde(rename = "BankAccountNumber")]
    pub BankAccountNumber: Option<String>,
    /// Currency code
    pub CurrencyCode: Option<String>,
    /// Reporting code
    #[serde(rename = "ReportingCode")]
    pub ReportingCode: Option<String>,
    /// Updated date
    pub UpdatedDateUTC: Option<String>,
}

/// Xero invoice (bill)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroInvoice {
    /// Invoice ID
    #[serde(rename = "InvoiceID")]
    pub InvoiceID: Option<String>,
    /// Invoice number
    #[serde(rename = "InvoiceNumber")]
    pub InvoiceNumber: Option<String>,
    /// Reference
    pub Reference: Option<String>,
    /// Contact
    pub Contact: XeroContact,
    /// Invoice type (ACCPAY for bills)
    #[serde(rename = "Type")]
    pub InvoiceType: String,
    /// Status
    pub Status: Option<String>,
    /// Line items
    #[serde(rename = "LineItems")]
    pub LineItems: Vec<XeroLineItem>,
    /// Date
    pub Date: String,
    /// Due date
    #[serde(rename = "DueDate")]
    pub DueDate: String,
    /// Currency code
    #[serde(rename = "CurrencyCode")]
    pub CurrencyCode: String,
    /// Sub total
    #[serde(rename = "SubTotal")]
    pub SubTotal: f64,
    /// Total tax
    #[serde(rename = "TotalTax")]
    pub TotalTax: f64,
    /// Total
    pub Total: f64,
    /// Amount due
    #[serde(rename = "AmountDue")]
    pub AmountDue: Option<f64>,
    /// Amount paid
    #[serde(rename = "AmountPaid")]
    pub AmountPaid: Option<f64>,
    /// Updated date
    pub UpdatedDateUTC: Option<String>,
}

/// Xero line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroLineItem {
    /// Line item ID
    #[serde(rename = "LineItemID")]
    pub LineItemID: Option<String>,
    /// Description
    pub Description: Option<String>,
    /// Quantity
    pub Quantity: Option<f64>,
    /// Unit amount
    #[serde(rename = "UnitAmount")]
    pub UnitAmount: Option<f64>,
    /// Account code
    #[serde(rename = "AccountCode")]
    pub AccountCode: Option<String>,
    /// Tax type
    #[serde(rename = "TaxType")]
    pub TaxType: Option<String>,
    /// Tax amount
    #[serde(rename = "TaxAmount")]
    pub TaxAmount: Option<f64>,
    /// Line amount
    #[serde(rename = "LineAmount")]
    pub LineAmount: Option<f64>,
    /// Tracking
    pub Tracking: Option<Vec<XeroTrackingCategory>>,
}

/// Xero tracking category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroTrackingCategory {
    #[serde(rename = "TrackingCategoryID")]
    pub TrackingCategoryID: Option<String>,
    pub Name: Option<String>,
    pub Option: Option<String>,
    #[serde(rename = "TrackingOptionID")]
    pub TrackingOptionID: Option<String>,
}

/// Xero tenant (organization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroTenant {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
    #[serde(rename = "tenantName")]
    pub tenant_name: String,
    #[serde(rename = "tenantType")]
    pub tenant_type: String,
}

/// Xero API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroResponse<T> {
    pub Id: Option<String>,
    pub Status: Option<String>,
    #[serde(rename = "ProviderName")]
    pub ProviderName: Option<String>,
    #[serde(rename = "DateTimeUTC")]
    pub DateTimeUTC: Option<String>,
    #[serde(rename = "HttpStatusCode")]
    pub HttpStatusCode: Option<i32>,
    #[serde(rename = "Items")]
    pub Items: Option<Vec<T>>,
    #[serde(rename = "Page")]
    pub Page: Option<i32>,
    #[serde(rename = "PageSize")]
    pub PageSize: Option<i32>,
    #[serde(rename = "TotalCount")]
    pub TotalCount: Option<i32>,
}

/// Xero OAuth tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroTokens {
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Token type
    pub token_type: String,
    /// Expires in seconds
    pub expires_in: i64,
    /// Scope
    pub scope: Option<String>,
}

/// Xero token storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XeroTokenStorage {
    /// Tenant ID
    pub tenant_id: String,
    /// Xero tenant ID
    pub xero_tenant_id: String,
    /// Organization name
    pub organization_name: String,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Access token expiry
    pub access_token_expires_at: DateTime<Utc>,
    /// Refresh token expiry
    pub refresh_token_expires_at: Option<DateTime<Utc>>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}
