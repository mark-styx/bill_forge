//! Workflow domain model for invoice processing
//!
//! This module implements a sophisticated workflow engine with:
//! - Customizable queue pipelines (AP → Pending Approval → Ready for Payment → Submitted)
//! - Per-queue assignment rules (vendor-based, department-based, amount-based)
//! - Multi-level approval support
//! - Automation rules (auto-approve, auto-submit)

use crate::types::{Money, TenantId, UserId};
use crate::domain::invoice::InvoiceId;
use crate::domain::vendor::VendorId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a workflow rule
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkflowRuleId(pub Uuid);

impl WorkflowRuleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkflowRuleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkflowRuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for WorkflowRuleId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a work queue
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkQueueId(pub Uuid);

impl WorkQueueId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkQueueId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkQueueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for WorkQueueId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Workflow rule for routing and approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRule {
    pub id: WorkflowRuleId,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub priority: i32,
    pub is_active: bool,
    pub rule_type: WorkflowRuleType,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Types of workflow rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRuleType {
    /// Routes invoice to a specific queue
    Routing,
    /// Requires approval before proceeding
    Approval,
    /// Auto-approves based on conditions
    AutoApproval,
    /// Escalation after timeout
    Escalation,
    /// Notification triggers
    Notification,
}

/// Condition for a workflow rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCondition {
    pub field: ConditionField,
    pub operator: ConditionOperator,
    pub value: serde_json::Value,
}

/// Fields that can be used in conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionField {
    Amount,
    VendorId,
    VendorName,
    Department,
    GlCode,
    InvoiceDate,
    DueDate,
    Tag,
    CustomField,
}

/// Operators for conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    Equals,
    NotEquals,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
    In,
    NotIn,
    Between,
    IsNull,
    IsNotNull,
}

/// Action to take when rule matches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleAction {
    pub action_type: ActionType,
    pub params: serde_json::Value,
}

/// Types of actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Route to a specific queue
    RouteToQueue,
    /// Require approval from user(s)
    RequireApproval,
    /// Require approval from role
    RequireRoleApproval,
    /// Auto-approve the invoice
    AutoApprove,
    /// Send notification
    SendNotification,
    /// Set a field value
    SetField,
    /// Add a tag
    AddTag,
    /// Escalate to user
    Escalate,
}

/// Work queue for organizing invoices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkQueue {
    pub id: WorkQueueId,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    pub queue_type: QueueType,
    pub assigned_users: Vec<UserId>,
    pub assigned_roles: Vec<String>,
    pub is_default: bool,
    pub is_active: bool,
    pub settings: QueueSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Type of work queue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueType {
    /// Initial review queue
    Review,
    /// Approval queue
    Approval,
    /// Exception/problem invoices
    Exception,
    /// Ready for payment
    Payment,
    /// Custom queue
    Custom,
}

/// Settings for a work queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSettings {
    /// Sort order for items
    pub default_sort: String,
    /// SLA hours for items in queue
    pub sla_hours: Option<i32>,
    /// Auto-escalate after hours
    pub escalation_hours: Option<i32>,
    /// Escalate to user
    pub escalation_user_id: Option<UserId>,
}

/// An invoice assigned to a queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: Uuid,
    pub queue_id: WorkQueueId,
    pub invoice_id: InvoiceId,
    pub tenant_id: TenantId,
    pub assigned_to: Option<UserId>,
    pub priority: i32,
    pub entered_at: DateTime<Utc>,
    pub due_at: Option<DateTime<Utc>>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Approval request for an invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub invoice_id: InvoiceId,
    pub tenant_id: TenantId,
    pub rule_id: WorkflowRuleId,
    pub requested_from: ApprovalTarget,
    pub status: ApprovalStatus,
    pub comments: Option<String>,
    pub responded_by: Option<UserId>,
    pub responded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Who the approval is requested from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalTarget {
    User(UserId),
    Role(String),
    AnyOf(Vec<UserId>),
    AllOf(Vec<UserId>),
}

/// Status of an approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
}

/// Input for creating a workflow rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkflowRuleInput {
    pub name: String,
    pub description: Option<String>,
    pub priority: i32,
    pub rule_type: WorkflowRuleType,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
}

/// Input for creating a work queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkQueueInput {
    pub name: String,
    pub description: Option<String>,
    pub queue_type: QueueType,
    pub assigned_users: Vec<UserId>,
    pub assigned_roles: Vec<String>,
    pub settings: QueueSettings,
}

/// Approval delegation rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDelegation {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub delegator_id: UserId,
    pub delegate_id: UserId,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub is_active: bool,
    pub conditions: Option<Vec<RuleCondition>>,
    pub created_at: DateTime<Utc>,
}

/// Approval limit for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalLimit {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub max_amount: Money,
    pub vendor_restrictions: Option<Vec<Uuid>>,
    pub department_restrictions: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Queue Flow Configuration
// ============================================================================

/// Defines the queue pipeline for a tenant
/// Default: OCR Error Queue (for unmapped) → AP Queue → Pending Approval → Ready for Payment → Submitted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueFlowConfig {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    /// Ordered list of queue stages in the flow
    pub stages: Vec<QueueStage>,
    /// Whether this is the default flow for the tenant
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A stage in the queue flow pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStage {
    /// Position in the pipeline (0 = first)
    pub order: i32,
    /// The queue at this stage
    pub queue_id: WorkQueueId,
    /// Optional: automatically transition to next stage under certain conditions
    pub auto_transition_rules: Vec<AutoTransitionRule>,
    /// Whether this stage requires explicit action to proceed
    pub requires_action: bool,
}

/// Rule for automatically transitioning between queue stages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTransitionRule {
    pub conditions: Vec<RuleCondition>,
    /// If true, skip this stage entirely when conditions match
    pub skip_stage: bool,
    /// If true, auto-complete the stage when conditions match
    pub auto_complete: bool,
}

/// Default queue stages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultQueueStage {
    /// Error queue for invoices that couldn't be processed/mapped by OCR
    OcrError,
    /// Initial AP review queue
    AccountsPayable,
    /// Pending approval from designated approvers
    PendingApproval,
    /// Ready to be submitted for payment
    ReadyForPayment,
    /// Payment has been submitted
    Submitted,
    /// Payment confirmed/completed
    Paid,
}

// ============================================================================
// Assignment Rules
// ============================================================================

/// Unique identifier for an assignment rule
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssignmentRuleId(pub Uuid);

impl AssignmentRuleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AssignmentRuleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AssignmentRuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for AssignmentRuleId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Rule for automatically assigning invoices to users within a queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentRule {
    pub id: AssignmentRuleId,
    pub tenant_id: TenantId,
    /// The queue this rule applies to
    pub queue_id: WorkQueueId,
    pub name: String,
    pub description: Option<String>,
    /// Priority (higher = evaluated first)
    pub priority: i32,
    pub is_active: bool,
    /// Conditions that must match for this rule to apply
    pub conditions: Vec<AssignmentCondition>,
    /// Who to assign to when conditions match
    pub assign_to: AssignmentTarget,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Condition for assignment rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentCondition {
    pub field: AssignmentField,
    pub operator: ConditionOperator,
    pub value: serde_json::Value,
}

/// Fields that can be used in assignment conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentField {
    /// Match by vendor
    VendorId,
    VendorName,
    /// Match by department (from line items or custom field)
    Department,
    /// Match by invoice amount
    Amount,
    /// Match by GL code
    GlCode,
    /// Match by custom field
    CustomField,
    /// Match by tag
    Tag,
}

/// Who to assign invoices to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssignmentTarget {
    /// Assign to a specific user
    User(UserId),
    /// Assign to anyone with a specific role
    Role(String),
    /// Assign to the registered approver for the matched vendor
    VendorApprover,
    /// Assign to the registered approver for the matched department
    DepartmentApprover,
    /// Round-robin assignment among specified users
    RoundRobin(Vec<UserId>),
    /// Assign to the user with the lowest current workload
    LeastLoaded(Vec<UserId>),
}

/// Input for creating an assignment rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAssignmentRuleInput {
    pub queue_id: WorkQueueId,
    pub name: String,
    pub description: Option<String>,
    pub priority: i32,
    pub conditions: Vec<AssignmentCondition>,
    pub assign_to: AssignmentTarget,
}

// ============================================================================
// Multi-Level Approval Configuration
// ============================================================================

/// Configuration for approval chains
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalChainConfig {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub description: Option<String>,
    /// Conditions that activate this approval chain
    pub conditions: Vec<RuleCondition>,
    /// Ordered list of approval steps
    pub steps: Vec<ApprovalStep>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A step in an approval chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalStep {
    /// Step order (0 = first)
    pub order: i32,
    /// Name of this step
    pub name: String,
    /// Who can approve at this step
    pub approvers: ApprovalTarget,
    /// How many approvals are needed (for AllOf targets)
    pub required_approvals: u32,
    /// Can this step be skipped if certain conditions are met?
    pub skip_conditions: Option<Vec<RuleCondition>>,
    /// Timeout in hours before escalation
    pub timeout_hours: Option<i32>,
    /// Who to escalate to on timeout
    pub escalate_to: Option<UserId>,
}

// ============================================================================
// Vendor/Department Approver Registration
// ============================================================================

/// Registration of a user as an approver for a specific vendor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorApproverRegistration {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub vendor_id: VendorId,
    pub user_id: UserId,
    /// Approval limit for this vendor (None = unlimited)
    pub max_amount: Option<Money>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Registration of a user as an approver for a specific department
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentApproverRegistration {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub department: String,
    pub user_id: UserId,
    /// Approval limit for this department (None = unlimited)
    pub max_amount: Option<Money>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Bulk Operations
// ============================================================================

/// Input for bulk operations on invoices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOperationInput {
    /// Invoice IDs to operate on
    pub invoice_ids: Vec<InvoiceId>,
    /// The operation to perform
    pub operation: BulkOperationType,
    /// Optional comment for the operation
    pub comment: Option<String>,
}

/// Types of bulk operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkOperationType {
    /// Submit multiple invoices for payment
    SubmitForPayment,
    /// Approve multiple invoices
    Approve,
    /// Reject multiple invoices
    Reject,
    /// Move to a specific queue
    MoveToQueue,
    /// Assign to a specific user
    AssignTo,
}

/// Result of a bulk operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOperationResult {
    pub total: usize,
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<BulkOperationError>,
}

/// Error for a single item in a bulk operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOperationError {
    pub invoice_id: InvoiceId,
    pub error: String,
}

// ============================================================================
// Enhanced Queue Item
// ============================================================================

/// Extended queue item with assignment tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItemExtended {
    pub id: Uuid,
    pub queue_id: WorkQueueId,
    pub invoice_id: InvoiceId,
    pub tenant_id: TenantId,
    /// User the item is assigned to
    pub assigned_to: Option<UserId>,
    /// Rule that assigned this item (if auto-assigned)
    pub assigned_by_rule: Option<AssignmentRuleId>,
    /// Priority (higher = more urgent)
    pub priority: i32,
    /// When the item entered this queue
    pub entered_at: DateTime<Utc>,
    /// SLA deadline
    pub due_at: Option<DateTime<Utc>>,
    /// When the item was claimed by a user
    pub claimed_at: Option<DateTime<Utc>>,
    /// When the item was completed
    pub completed_at: Option<DateTime<Utc>>,
    /// The action taken (if completed)
    pub completion_action: Option<String>,
    /// Comments/notes on this queue item
    pub notes: Option<String>,
}

// ============================================================================
// Document Reference
// ============================================================================

/// Reference to a stored document (PDF, image, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRef {
    pub id: Uuid,
    pub tenant_id: TenantId,
    /// Original filename
    pub filename: String,
    /// MIME type
    pub mime_type: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Storage path/key
    pub storage_key: String,
    /// Associated invoice (if any)
    pub invoice_id: Option<InvoiceId>,
    /// Document type
    pub doc_type: DocumentType,
    pub created_at: DateTime<Utc>,
}

/// Type of document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    /// Original invoice PDF/image
    InvoiceOriginal,
    /// Supporting documentation
    Supporting,
    /// Tax document (W9, 1099, etc.)
    TaxDocument,
    /// Contract or agreement
    Contract,
    /// Other
    Other,
}
