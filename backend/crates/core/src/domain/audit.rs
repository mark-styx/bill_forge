//! Audit log domain models
//!
//! Provides audit trail functionality for compliance and security.

use crate::types::{TenantId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An audit log entry recording a system action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier for this audit entry
    pub id: Uuid,
    /// Tenant where the action occurred
    pub tenant_id: TenantId,
    /// User who performed the action (None for system actions)
    pub user_id: Option<UserId>,
    /// User's email for display purposes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
    /// The action that was performed
    pub action: AuditAction,
    /// Type of resource affected
    pub resource_type: ResourceType,
    /// ID of the affected resource
    pub resource_id: String,
    /// Human-readable description of what happened
    pub description: String,
    /// Previous state (for updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<serde_json::Value>,
    /// New state (for creates and updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<serde_json::Value>,
    /// Additional context/metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// IP address of the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// User agent string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Request ID for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// When the action occurred
    pub created_at: DateTime<Utc>,
}

impl AuditEntry {
    /// Create a new audit entry
    pub fn new(
        tenant_id: TenantId,
        user_id: Option<UserId>,
        action: AuditAction,
        resource_type: ResourceType,
        resource_id: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            user_email: None,
            action,
            resource_type,
            resource_id: resource_id.into(),
            description: description.into(),
            old_value: None,
            new_value: None,
            metadata: None,
            ip_address: None,
            user_agent: None,
            request_id: None,
            created_at: Utc::now(),
        }
    }

    /// Add user email for display
    pub fn with_user_email(mut self, email: impl Into<String>) -> Self {
        self.user_email = Some(email.into());
        self
    }

    /// Add old value for update actions
    pub fn with_old_value(mut self, value: serde_json::Value) -> Self {
        self.old_value = Some(value);
        self
    }

    /// Add new value
    pub fn with_new_value(mut self, value: serde_json::Value) -> Self {
        self.new_value = Some(value);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Add request context
    pub fn with_request_context(
        mut self,
        ip_address: Option<String>,
        user_agent: Option<String>,
        request_id: Option<String>,
    ) -> Self {
        self.ip_address = ip_address;
        self.user_agent = user_agent;
        self.request_id = request_id;
        self
    }
}

/// Types of auditable actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // CRUD operations
    Create,
    Read,
    Update,
    Delete,

    // Authentication
    Login,
    Logout,
    LoginFailed,
    PasswordChanged,
    TokenRefreshed,

    // Invoice workflow
    InvoiceSubmitted,
    InvoiceApproved,
    InvoiceRejected,
    InvoicePutOnHold,
    InvoiceReleasedFromHold,
    InvoiceVoided,
    InvoiceMarkedForPayment,
    InvoicePaid,

    // Queue operations
    MovedToQueue,
    AssignedToUser,
    Claimed,

    // OCR operations
    OcrStarted,
    OcrCompleted,
    OcrFailed,
    OcrRerun,

    // Vendor operations
    VendorActivated,
    VendorDeactivated,

    // Export operations
    DataExported,

    // Admin operations
    SettingsChanged,
    ModulesChanged,
    UserInvited,
    UserDeactivated,
    RoleChanged,
}

impl AuditAction {
    /// Get a human-readable label for the action
    pub fn label(&self) -> &'static str {
        match self {
            AuditAction::Create => "Created",
            AuditAction::Read => "Viewed",
            AuditAction::Update => "Updated",
            AuditAction::Delete => "Deleted",
            AuditAction::Login => "Logged in",
            AuditAction::Logout => "Logged out",
            AuditAction::LoginFailed => "Failed login attempt",
            AuditAction::PasswordChanged => "Changed password",
            AuditAction::TokenRefreshed => "Refreshed session",
            AuditAction::InvoiceSubmitted => "Submitted invoice",
            AuditAction::InvoiceApproved => "Approved invoice",
            AuditAction::InvoiceRejected => "Rejected invoice",
            AuditAction::InvoicePutOnHold => "Put invoice on hold",
            AuditAction::InvoiceReleasedFromHold => "Released invoice from hold",
            AuditAction::InvoiceVoided => "Voided invoice",
            AuditAction::InvoiceMarkedForPayment => "Marked invoice for payment",
            AuditAction::InvoicePaid => "Marked invoice as paid",
            AuditAction::MovedToQueue => "Moved to queue",
            AuditAction::AssignedToUser => "Assigned to user",
            AuditAction::Claimed => "Claimed work item",
            AuditAction::OcrStarted => "Started OCR processing",
            AuditAction::OcrCompleted => "Completed OCR processing",
            AuditAction::OcrFailed => "OCR processing failed",
            AuditAction::OcrRerun => "Reran OCR processing",
            AuditAction::VendorActivated => "Activated vendor",
            AuditAction::VendorDeactivated => "Deactivated vendor",
            AuditAction::DataExported => "Exported data",
            AuditAction::SettingsChanged => "Changed settings",
            AuditAction::ModulesChanged => "Changed modules",
            AuditAction::UserInvited => "Invited user",
            AuditAction::UserDeactivated => "Deactivated user",
            AuditAction::RoleChanged => "Changed user role",
        }
    }
}

/// Types of resources that can be audited
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Invoice,
    Vendor,
    User,
    Tenant,
    WorkQueue,
    WorkflowRule,
    AssignmentRule,
    Document,
    ApprovalRequest,
    Export,
    Settings,
    Session,
}

impl ResourceType {
    /// Get the display name for this resource type
    pub fn display_name(&self) -> &'static str {
        match self {
            ResourceType::Invoice => "Invoice",
            ResourceType::Vendor => "Vendor",
            ResourceType::User => "User",
            ResourceType::Tenant => "Tenant",
            ResourceType::WorkQueue => "Work Queue",
            ResourceType::WorkflowRule => "Workflow Rule",
            ResourceType::AssignmentRule => "Assignment Rule",
            ResourceType::Document => "Document",
            ResourceType::ApprovalRequest => "Approval Request",
            ResourceType::Export => "Export",
            ResourceType::Settings => "Settings",
            ResourceType::Session => "Session",
        }
    }
}
