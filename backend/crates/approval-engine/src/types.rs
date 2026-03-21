//! Approval engine data types
//!
//! Types matching the migration 065 schema: approval_policies, approval_chain_levels,
//! active_approval_chains, approval_chain_steps, approval_activity_log.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ══════════════════════════════════════════════════════════════════════
// Approval Policy
// ══════════════════════════════════════════════════════════════════════

/// An approval policy that defines the rules for routing invoices through
/// approval chains based on amount thresholds, department, vendor, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub is_default: bool,
    /// JSON criteria for matching invoices (amount ranges, departments, vendors)
    pub match_criteria: serde_json::Value,
    /// Higher priority policies are evaluated first
    pub priority: i32,
    /// Whether approval levels must be completed in order
    pub require_sequential: bool,
    /// Whether all levels must approve (vs. any single level)
    pub require_all_levels: bool,
    /// Whether the submitter can also approve
    pub allow_self_approval: bool,
    /// Invoices below this amount are auto-approved (None = no auto-approve)
    pub auto_approve_below_cents: Option<i64>,
    /// Whether to escalate overdue approvals
    pub escalation_enabled: bool,
    /// Hours before escalation triggers
    pub escalation_timeout_hours: Option<i32>,
    /// Final escalation target user
    pub final_escalation_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a new approval policy with its chain levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicyInput {
    pub name: String,
    pub description: Option<String>,
    pub match_criteria: Option<serde_json::Value>,
    pub priority: Option<i32>,
    pub require_sequential: Option<bool>,
    pub require_all_levels: Option<bool>,
    pub allow_self_approval: Option<bool>,
    pub auto_approve_below_cents: Option<i64>,
    pub escalation_enabled: Option<bool>,
    pub escalation_timeout_hours: Option<i32>,
    pub final_escalation_user_id: Option<Uuid>,
    pub levels: Vec<CreateChainLevelInput>,
}

// ══════════════════════════════════════════════════════════════════════
// Chain Level (template for a level in the approval chain)
// ══════════════════════════════════════════════════════════════════════

/// A level in an approval policy's chain template.
/// Defines who approves at a given level and the amount thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalChainLevel {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub tenant_id: Uuid,
    /// Order of this level in the chain (1-based)
    pub level_order: i32,
    pub name: String,
    /// Type of approver: "user", "role", "department_head", "amount_authority"
    pub approver_type: String,
    /// Specific user IDs (for type="user")
    pub approver_user_ids: serde_json::Value,
    /// Role name (for type="role")
    pub approver_role: Option<String>,
    /// Minimum invoice amount (cents) for this level to apply
    pub min_amount_cents: i64,
    /// Maximum invoice amount (cents) — None means no upper limit
    pub max_amount_cents: Option<i64>,
    /// How many approvers at this level must approve
    pub required_approver_count: i32,
    /// Timeout before escalation (overrides policy default)
    pub timeout_hours: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a chain level within a policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChainLevelInput {
    pub name: String,
    /// "user", "role", "department_head", "amount_authority"
    pub approver_type: String,
    pub approver_user_ids: Option<Vec<Uuid>>,
    pub approver_role: Option<String>,
    pub min_amount_cents: Option<i64>,
    pub max_amount_cents: Option<i64>,
    pub required_approver_count: Option<i32>,
    pub timeout_hours: Option<i32>,
}

// ══════════════════════════════════════════════════════════════════════
// Active Approval Chain (runtime instance for a specific invoice)
// ══════════════════════════════════════════════════════════════════════

/// A live approval chain created for a specific invoice.
/// Tracks progress through the approval levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveApprovalChain {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub invoice_id: Uuid,
    pub policy_id: Uuid,
    /// "pending", "in_progress", "approved", "rejected", "cancelled"
    pub status: String,
    /// Current level being processed (1-based)
    pub current_level: i32,
    /// Total number of applicable levels
    pub total_levels: i32,
    /// Final decision: "approved" or "rejected"
    pub final_decision: Option<String>,
    pub final_decided_by: Option<Uuid>,
    pub final_decided_at: Option<DateTime<Utc>>,
    pub escalation_count: i32,
    pub last_escalated_at: Option<DateTime<Utc>>,
    pub initiated_by: Uuid,
    pub initiated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ══════════════════════════════════════════════════════════════════════
// Approval Chain Step (individual approver action within a chain)
// ══════════════════════════════════════════════════════════════════════

/// A single approval step assigned to an approver within an active chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalChainStep {
    pub id: Uuid,
    pub chain_id: Uuid,
    pub tenant_id: Uuid,
    pub level_id: Uuid,
    pub level_order: i32,
    pub assigned_to: Uuid,
    /// "pending", "approved", "rejected", "delegated", "escalated", "cancelled"
    pub status: String,
    /// "approved" or "rejected"
    pub decision: Option<String>,
    pub comments: Option<String>,
    pub delegated_to: Option<Uuid>,
    pub delegated_at: Option<DateTime<Utc>>,
    pub delegation_reason: Option<String>,
    pub assigned_at: DateTime<Utc>,
    pub due_at: Option<DateTime<Utc>>,
    pub responded_at: Option<DateTime<Utc>>,
    pub escalated_at: Option<DateTime<Utc>>,
    pub escalated_to: Option<Uuid>,
    pub escalation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ══════════════════════════════════════════════════════════════════════
// Activity Log
// ══════════════════════════════════════════════════════════════════════

/// An entry in the approval activity audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalActivity {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub chain_id: Uuid,
    pub step_id: Option<Uuid>,
    pub invoice_id: Uuid,
    /// Action type: "submitted", "approved", "rejected", "delegated",
    /// "escalated", "recalled", "auto_approved"
    pub action: String,
    pub actor_id: Uuid,
    pub actor_role: Option<String>,
    pub comments: Option<String>,
    pub metadata: serde_json::Value,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ══════════════════════════════════════════════════════════════════════
// Input Types
// ══════════════════════════════════════════════════════════════════════

/// Input for an approve/reject decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionInput {
    /// "approve" or "reject"
    pub decision: String,
    pub comments: Option<String>,
}

/// Input for delegating an approval step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateInput {
    pub delegate_to: Uuid,
    pub reason: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════
// Response Types
// ══════════════════════════════════════════════════════════════════════

/// Full detail view of an approval chain (for API responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalChainDetail {
    pub chain: ActiveApprovalChain,
    pub policy: ApprovalPolicy,
    pub steps: Vec<ApprovalChainStep>,
    pub activity: Vec<ApprovalActivity>,
}

/// Summary of a pending approval for a user's inbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApprovalSummary {
    pub step: ApprovalChainStep,
    pub chain: ActiveApprovalChain,
    pub invoice_id: Uuid,
    pub invoice_number: Option<String>,
    pub vendor_name: Option<String>,
    pub total_amount_cents: Option<i64>,
    pub policy_name: String,
    pub level_name: String,
}
