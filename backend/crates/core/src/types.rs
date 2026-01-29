//! Core types used across all BillForge modules

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a tenant (company/organization)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(pub Uuid);

impl TenantId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TenantId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a user
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Available modules in BillForge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Module {
    InvoiceCapture,
    InvoiceProcessing,
    VendorManagement,
    Reporting,
}

impl Module {
    pub fn as_str(&self) -> &'static str {
        match self {
            Module::InvoiceCapture => "invoice_capture",
            Module::InvoiceProcessing => "invoice_processing",
            Module::VendorManagement => "vendor_management",
            Module::Reporting => "reporting",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Module::InvoiceCapture => "Invoice Capture",
            Module::InvoiceProcessing => "Invoice Processing",
            Module::VendorManagement => "Vendor Management",
            Module::Reporting => "Reporting & Analytics",
        }
    }
}

/// User roles within the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Full system access for the tenant
    TenantAdmin,
    /// Accounts Payable staff - can process invoices
    ApUser,
    /// Can approve invoices based on rules
    Approver,
    /// Can manage vendors
    VendorManager,
    /// Read-only access to reports
    ReportViewer,
    /// Custom role defined by tenant
    Custom(u32),
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Role::TenantAdmin => "tenant_admin",
            Role::ApUser => "ap_user",
            Role::Approver => "approver",
            Role::VendorManager => "vendor_manager",
            Role::ReportViewer => "report_viewer",
            Role::Custom(_) => "custom",
        }
    }
}

/// Tenant context for request processing
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub tenant_name: String,
    pub enabled_modules: Vec<Module>,
    pub settings: TenantSettings,
}

impl TenantContext {
    /// Check if a module is enabled for this tenant
    pub fn has_module(&self, module: Module) -> bool {
        self.enabled_modules.contains(&module)
    }
}

/// Tenant-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSettings {
    /// Custom branding
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub company_name: String,

    /// Timezone for date displays
    pub timezone: String,

    /// Currency settings
    pub default_currency: String,

    /// Feature flags
    pub features: TenantFeatures,
}

impl Default for TenantSettings {
    fn default() -> Self {
        Self {
            logo_url: None,
            primary_color: None,
            company_name: String::new(),
            timezone: "UTC".to_string(),
            default_currency: "USD".to_string(),
            features: TenantFeatures::default(),
        }
    }
}

/// Feature flags per tenant
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantFeatures {
    pub advanced_ocr: bool,
    pub api_access: bool,
    pub custom_workflows: bool,
    pub audit_logs: bool,
    pub sso_enabled: bool,
}

/// Authenticated user context
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub name: String,
    pub roles: Vec<Role>,
}

impl UserContext {
    /// Check if user has a specific role
    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role) || self.roles.contains(&Role::TenantAdmin)
    }

    /// Check if user is a tenant admin
    pub fn is_admin(&self) -> bool {
        self.roles.contains(&Role::TenantAdmin)
    }
}

/// Pagination parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 25,
        }
    }
}

impl Pagination {
    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.per_page
    }
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub per_page: u32,
    pub total_items: u64,
    pub total_pages: u32,
}

/// Money amount with currency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Money {
    /// Amount in smallest currency unit (cents for USD)
    pub amount: i64,
    /// ISO 4217 currency code
    pub currency: String,
}

impl Money {
    pub fn new(amount: i64, currency: impl Into<String>) -> Self {
        Self {
            amount,
            currency: currency.into(),
        }
    }

    pub fn usd(dollars: f64) -> Self {
        Self {
            amount: (dollars * 100.0).round() as i64,
            currency: "USD".to_string(),
        }
    }

    pub fn as_decimal(&self) -> f64 {
        self.amount as f64 / 100.0
    }
}

// Note: AuditEntry is now defined in domain/audit.rs with comprehensive fields
