//! Workflow engine for invoice processing

use billforge_core::{
    domain::{
        ApprovalRequest, ApprovalStatus, ApprovalTarget, Invoice, ProcessingStatus,
        RuleCondition, WorkflowRule, WorkflowRuleType,
    },
    traits::{ApprovalRepository, InvoiceRepository, WorkflowRuleRepository},
    types::{TenantId, UserId},
    Result,
};
use std::sync::Arc;

/// Workflow engine for processing invoices
pub struct WorkflowEngine {
    invoice_repo: Arc<dyn InvoiceRepository>,
    rule_repo: Arc<dyn WorkflowRuleRepository>,
    approval_repo: Arc<dyn ApprovalRepository>,
}

impl WorkflowEngine {
    pub fn new(
        invoice_repo: Arc<dyn InvoiceRepository>,
        rule_repo: Arc<dyn WorkflowRuleRepository>,
        approval_repo: Arc<dyn ApprovalRepository>,
    ) -> Self {
        Self {
            invoice_repo,
            rule_repo,
            approval_repo,
        }
    }

    /// Process a submitted invoice through the workflow
    pub async fn process_invoice(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
    ) -> Result<ProcessingStatus> {
        // Get active routing rules
        let routing_rules = self
            .rule_repo
            .get_active_rules(tenant_id, WorkflowRuleType::Routing)
            .await?;

        // Get active approval rules
        let approval_rules = self
            .rule_repo
            .get_active_rules(tenant_id, WorkflowRuleType::Approval)
            .await?;

        // Check auto-approval rules first
        let auto_approval_rules = self
            .rule_repo
            .get_active_rules(tenant_id, WorkflowRuleType::AutoApproval)
            .await?;

        for rule in &auto_approval_rules {
            if self.evaluate_conditions(invoice, &rule.conditions) {
                // Auto-approve
                return Ok(ProcessingStatus::Approved);
            }
        }

        // Check if approval is required
        let mut approvals_needed = Vec::new();
        for rule in &approval_rules {
            if self.evaluate_conditions(invoice, &rule.conditions) {
                approvals_needed.push(rule.clone());
            }
        }

        if !approvals_needed.is_empty() {
            // Create approval requests
            for rule in approvals_needed {
                self.create_approval_request(tenant_id, invoice, &rule).await?;
            }
            return Ok(ProcessingStatus::PendingApproval);
        }

        // No approval needed
        Ok(ProcessingStatus::Approved)
    }

    /// Evaluate rule conditions against an invoice
    fn evaluate_conditions(&self, invoice: &Invoice, conditions: &[RuleCondition]) -> bool {
        conditions.iter().all(|condition| self.evaluate_condition(invoice, condition))
    }

    /// Evaluate a single condition
    fn evaluate_condition(&self, invoice: &Invoice, condition: &RuleCondition) -> bool {
        use billforge_core::domain::{ConditionField, ConditionOperator};

        match &condition.field {
            ConditionField::Amount => {
                let amount = invoice.total_amount.as_decimal();
                match &condition.operator {
                    ConditionOperator::GreaterThan => {
                        condition.value.as_f64().map_or(false, |v| amount > v)
                    }
                    ConditionOperator::LessThan => {
                        condition.value.as_f64().map_or(false, |v| amount < v)
                    }
                    ConditionOperator::GreaterThanOrEqual => {
                        condition.value.as_f64().map_or(false, |v| amount >= v)
                    }
                    ConditionOperator::LessThanOrEqual => {
                        condition.value.as_f64().map_or(false, |v| amount <= v)
                    }
                    ConditionOperator::Equals => {
                        condition.value.as_f64().map_or(false, |v| (amount - v).abs() < 0.01)
                    }
                    _ => false,
                }
            }
            ConditionField::VendorId => {
                match &condition.operator {
                    ConditionOperator::Equals => {
                        let vendor_id = invoice.vendor_id.map(|v| v.to_string());
                        condition.value.as_str().map_or(false, |v| {
                            vendor_id.as_deref() == Some(v)
                        })
                    }
                    _ => false,
                }
            }
            _ => {
                // TODO: Implement other conditions
                false
            }
        }
    }

    /// Create an approval request
    async fn create_approval_request(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        rule: &WorkflowRule,
    ) -> Result<()> {
        // Extract approval target from rule actions
        // This is a simplified implementation
        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4(),
            invoice_id: invoice.id.clone(),
            tenant_id: tenant_id.clone(),
            rule_id: rule.id.clone(),
            requested_from: ApprovalTarget::Role("approver".to_string()),
            status: ApprovalStatus::Pending,
            comments: None,
            responded_by: None,
            responded_at: None,
            created_at: chrono::Utc::now(),
            expires_at: None,
        };

        self.approval_repo.create(tenant_id, request).await?;
        Ok(())
    }

    /// Handle approval response
    pub async fn handle_approval(
        &self,
        tenant_id: &TenantId,
        approval_id: uuid::Uuid,
        approved: bool,
        comments: Option<String>,
        user_id: &UserId,
    ) -> Result<ProcessingStatus> {
        let status = if approved {
            ApprovalStatus::Approved
        } else {
            ApprovalStatus::Rejected
        };

        let approval = self
            .approval_repo
            .respond(tenant_id, approval_id, status, comments, user_id)
            .await?;

        // Get all approvals for this invoice
        let all_approvals = self
            .approval_repo
            .list_for_invoice(tenant_id, &approval.invoice_id)
            .await?;

        // Check if any are rejected
        if all_approvals.iter().any(|a| a.status == ApprovalStatus::Rejected) {
            return Ok(ProcessingStatus::Rejected);
        }

        // Check if all are approved
        if all_approvals.iter().all(|a| a.status == ApprovalStatus::Approved) {
            return Ok(ProcessingStatus::Approved);
        }

        // Still waiting for approvals
        Ok(ProcessingStatus::PendingApproval)
    }
}
