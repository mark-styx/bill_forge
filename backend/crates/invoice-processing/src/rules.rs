//! Workflow rule definitions and helpers

use billforge_core::domain::{
    ActionType, ConditionField, ConditionOperator, RuleAction, RuleCondition, WorkflowRuleType,
};
use serde_json::json;

/// Helper to create common rule conditions
pub struct RuleBuilder {
    conditions: Vec<RuleCondition>,
    actions: Vec<RuleAction>,
}

impl RuleBuilder {
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Add amount threshold condition
    pub fn amount_greater_than(mut self, amount: f64) -> Self {
        self.conditions.push(RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::GreaterThan,
            value: json!(amount),
        });
        self
    }

    /// Add amount less than condition
    pub fn amount_less_than(mut self, amount: f64) -> Self {
        self.conditions.push(RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::LessThan,
            value: json!(amount),
        });
        self
    }

    /// Add vendor condition
    pub fn for_vendor(mut self, vendor_id: &str) -> Self {
        self.conditions.push(RuleCondition {
            field: ConditionField::VendorId,
            operator: ConditionOperator::Equals,
            value: json!(vendor_id),
        });
        self
    }

    /// Add department condition
    pub fn for_department(mut self, department: &str) -> Self {
        self.conditions.push(RuleCondition {
            field: ConditionField::Department,
            operator: ConditionOperator::Equals,
            value: json!(department),
        });
        self
    }

    /// Add require approval action
    pub fn require_approval_from_role(mut self, role: &str) -> Self {
        self.actions.push(RuleAction {
            action_type: ActionType::RequireRoleApproval,
            params: json!({ "role": role }),
        });
        self
    }

    /// Add require approval from specific user action
    pub fn require_approval_from_user(mut self, user_id: &str) -> Self {
        self.actions.push(RuleAction {
            action_type: ActionType::RequireApproval,
            params: json!({ "user_id": user_id }),
        });
        self
    }

    /// Add auto-approve action
    pub fn auto_approve(mut self) -> Self {
        self.actions.push(RuleAction {
            action_type: ActionType::AutoApprove,
            params: json!({}),
        });
        self
    }

    /// Add route to queue action
    pub fn route_to_queue(mut self, queue_id: &str) -> Self {
        self.actions.push(RuleAction {
            action_type: ActionType::RouteToQueue,
            params: json!({ "queue_id": queue_id }),
        });
        self
    }

    /// Build the conditions and actions
    pub fn build(self) -> (Vec<RuleCondition>, Vec<RuleAction>) {
        (self.conditions, self.actions)
    }
}

impl Default for RuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a standard "small invoice auto-approval" rule
pub fn small_invoice_auto_approval(threshold: f64) -> (Vec<RuleCondition>, Vec<RuleAction>) {
    RuleBuilder::new()
        .amount_less_than(threshold)
        .auto_approve()
        .build()
}

/// Create a standard "large invoice requires manager approval" rule
pub fn large_invoice_manager_approval(threshold: f64) -> (Vec<RuleCondition>, Vec<RuleAction>) {
    RuleBuilder::new()
        .amount_greater_than(threshold)
        .require_approval_from_role("tenant_admin")
        .build()
}
