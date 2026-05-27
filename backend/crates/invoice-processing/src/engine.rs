//! Workflow engine for invoice processing

use billforge_core::{
    domain::{
        ActionType, ApprovalRequest, ApprovalStatus, ApprovalTarget, Invoice, ProcessingStatus,
        RuleAction, RuleCondition, WorkflowRule, WorkflowRuleType,
    },
    intelligent_routing::{IntelligentRoutingEngine, RoutingConfig, RoutingDataProvider},
    traits::{ApprovalRepository, InvoiceRepository, WorkflowRuleRepository},
    types::{TenantId, UserId},
    Result,
};
use std::sync::Arc;

/// Minimum confidence threshold for ML auto-approval (95%)
const ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD: f32 = 0.95;

/// Workflow engine for processing invoices
pub struct WorkflowEngine {
    invoice_repo: Arc<dyn InvoiceRepository>,
    rule_repo: Arc<dyn WorkflowRuleRepository>,
    approval_repo: Arc<dyn ApprovalRepository>,
    routing_provider: Option<Arc<dyn RoutingDataProvider>>,
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
            routing_provider: None,
        }
    }

    pub fn with_routing(mut self, routing_provider: Arc<dyn RoutingDataProvider>) -> Self {
        self.routing_provider = Some(routing_provider);
        self
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

        // Check ML categorization auto-approval threshold
        // If categorization confidence >= threshold and all required fields are present, auto-approve
        if let Some(confidence) = invoice.categorization_confidence {
            if confidence >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD {
                // Check that all required categorization fields are populated
                let has_complete_categorization = invoice.gl_code.is_some()
                    && invoice.department.is_some()
                    && invoice.cost_center.is_some();

                if has_complete_categorization {
                    tracing::info!(
                        invoice_id = %invoice.id.as_uuid(),
                        confidence = confidence,
                        "Auto-approving invoice due to high ML categorization confidence"
                    );
                    return Ok(ProcessingStatus::Approved);
                }
            }
        }

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

        for rule in &routing_rules {
            if !self.evaluate_conditions(invoice, &rule.conditions) {
                continue;
            }
            if self.rule_has_action(rule, ActionType::AutoApprove) {
                return Ok(ProcessingStatus::Approved);
            }
            let approval_actions = self.approval_actions(rule);
            if !approval_actions.is_empty() {
                for action in approval_actions {
                    self.create_approval_request_for_action(tenant_id, invoice, rule, action)
                        .await?;
                }
                return Ok(ProcessingStatus::PendingApproval);
            }
            if self.rule_has_action(rule, ActionType::RouteToQueue) {
                tracing::info!(
                    invoice_id = %invoice.id.as_uuid(),
                    rule_id = %rule.id,
                    "Routing rule requested queue routing; invoice remains submitted for queue assignment"
                );
                return Ok(ProcessingStatus::Submitted);
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
                self.create_approval_request(tenant_id, invoice, &rule)
                    .await?;
            }
            return Ok(ProcessingStatus::PendingApproval);
        }

        // No approval needed
        Ok(ProcessingStatus::Approved)
    }

    /// Evaluate rule conditions against an invoice
    fn evaluate_conditions(&self, invoice: &Invoice, conditions: &[RuleCondition]) -> bool {
        billforge_core::workflow_evaluator::evaluate_conditions(invoice, conditions)
    }

    /// Evaluate a single condition
    fn evaluate_condition(&self, invoice: &Invoice, condition: &RuleCondition) -> bool {
        billforge_core::workflow_evaluator::evaluate_single_condition(invoice, condition)
    }

    fn rule_has_action(&self, rule: &WorkflowRule, action_type: ActionType) -> bool {
        rule.actions
            .iter()
            .any(|action| action.action_type == action_type)
    }

    fn approval_actions<'a>(&self, rule: &'a WorkflowRule) -> Vec<&'a RuleAction> {
        rule.actions
            .iter()
            .filter(|action| {
                matches!(
                    action.action_type,
                    ActionType::RequireApproval | ActionType::RequireRoleApproval
                )
            })
            .collect()
    }

    /// Create an approval request
    async fn create_approval_request(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        rule: &WorkflowRule,
    ) -> Result<()> {
        if !self.approval_actions(rule).is_empty() {
            for action in self.approval_actions(rule) {
                self.create_approval_request_for_action(tenant_id, invoice, rule, action)
                    .await?;
            }
            return Ok(());
        }

        self.create_approval_request_with_target(
            tenant_id,
            invoice,
            rule,
            self.intelligent_approval_target(tenant_id, invoice)
                .await
                .unwrap_or_else(|| ApprovalTarget::Role("approver".to_string())),
        )
        .await
    }

    async fn create_approval_request_for_action(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        rule: &WorkflowRule,
        action: &RuleAction,
    ) -> Result<()> {
        let target = match action.action_type {
            ActionType::RequireApproval => action
                .params
                .get("user_id")
                .and_then(|value| value.as_str())
                .and_then(|user_id| uuid::Uuid::parse_str(user_id).ok())
                .map(|user_id| ApprovalTarget::User(UserId::from_uuid(user_id)))
                .unwrap_or_else(|| {
                    // Async routing is handled below for the no-explicit-user case.
                    ApprovalTarget::Role("approver".to_string())
                }),
            ActionType::RequireRoleApproval => action
                .params
                .get("role")
                .and_then(|value| value.as_str())
                .map(|role| ApprovalTarget::Role(role.to_string()))
                .unwrap_or_else(|| ApprovalTarget::Role("approver".to_string())),
            _ => ApprovalTarget::Role("approver".to_string()),
        };

        let target = if matches!(target, ApprovalTarget::Role(ref role) if role == "approver") {
            self.intelligent_approval_target(tenant_id, invoice)
                .await
                .unwrap_or(target)
        } else {
            target
        };

        self.create_approval_request_with_target(tenant_id, invoice, rule, target)
            .await
    }

    async fn intelligent_approval_target(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
    ) -> Option<ApprovalTarget> {
        let provider = self.routing_provider.as_ref()?;
        let context = match provider.get_routing_context(tenant_id).await {
            Ok(context) => context,
            Err(error) => {
                tracing::warn!(
                    tenant_id = %tenant_id,
                    invoice_id = %invoice.id.as_uuid(),
                    error = %error,
                    "Intelligent routing context failed; falling back to role approval"
                );
                return None;
            }
        };

        let mut config = RoutingConfig::default();
        config.tenant_id = tenant_id.clone();
        let engine = IntelligentRoutingEngine::new(config);
        let decision = context.route(&engine, invoice);
        let approver_id = decision.approver_id?;

        tracing::info!(
            tenant_id = %tenant_id,
            invoice_id = %invoice.id.as_uuid(),
            approver_id = %approver_id,
            score = decision.score,
            strategy = ?decision.strategy,
            "Selected approval target through intelligent routing"
        );

        Some(ApprovalTarget::User(approver_id))
    }

    async fn create_approval_request_with_target(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        rule: &WorkflowRule,
        target: ApprovalTarget,
    ) -> Result<()> {
        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4(),
            invoice_id: invoice.id.clone(),
            tenant_id: tenant_id.clone(),
            rule_id: rule.id.clone(),
            requested_from: target,
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
        if all_approvals
            .iter()
            .any(|a| a.status == ApprovalStatus::Rejected)
        {
            return Ok(ProcessingStatus::Rejected);
        }

        // Check if all are approved
        if all_approvals
            .iter()
            .all(|a| a.status == ApprovalStatus::Approved)
        {
            return Ok(ProcessingStatus::Approved);
        }

        // Still waiting for approvals
        Ok(ProcessingStatus::PendingApproval)
    }
}
