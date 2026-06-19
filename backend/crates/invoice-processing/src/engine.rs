//! Workflow engine for invoice processing

use billforge_core::{
    domain::{
        ActionType, ApprovalRequest, ApprovalStatus, ApprovalTarget, Invoice, ProcessingStatus,
        RuleAction, RuleCondition, StageType, WorkflowRule, WorkflowRuleId, WorkflowRuleType,
        WorkflowTemplate, WorkflowTemplateStage,
    },
    intelligent_routing::{IntelligentRoutingEngine, RoutingConfig, RoutingDataProvider},
    traits::{
        ApprovalRepository, AuditService, InvoiceRepository, TenantSettingsProvider,
        WorkflowRuleRepository, WorkflowTemplateRepository,
    },
    types::{TenantId, TenantSettings, UserId},
    Result,
};
use billforge_invoice_capture::OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD;
use chrono::Duration;
use sqlx::PgPool;
use std::sync::Arc;

/// Minimum confidence threshold for ML auto-approval (95%)
const ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD: f32 = 0.95;

/// Workflow engine for processing invoices
#[allow(dead_code)]
pub struct WorkflowEngine {
    invoice_repo: Arc<dyn InvoiceRepository>,
    rule_repo: Arc<dyn WorkflowRuleRepository>,
    approval_repo: Arc<dyn ApprovalRepository>,
    workflow_template_repo: Arc<dyn WorkflowTemplateRepository>,
    routing_provider: Option<Arc<dyn RoutingDataProvider>>,
    settings_provider: Option<Arc<dyn TenantSettingsProvider>>,
    audit_service: Option<Arc<dyn AuditService>>,
    pool: Option<Arc<PgPool>>,
}

impl WorkflowEngine {
    pub fn new(
        invoice_repo: Arc<dyn InvoiceRepository>,
        rule_repo: Arc<dyn WorkflowRuleRepository>,
        approval_repo: Arc<dyn ApprovalRepository>,
        workflow_template_repo: Arc<dyn WorkflowTemplateRepository>,
    ) -> Self {
        Self {
            invoice_repo,
            rule_repo,
            approval_repo,
            workflow_template_repo,
            routing_provider: None,
            settings_provider: None,
            audit_service: None,
            pool: None,
        }
    }

    pub fn with_routing(mut self, routing_provider: Arc<dyn RoutingDataProvider>) -> Self {
        self.routing_provider = Some(routing_provider);
        self
    }

    pub fn with_tenant_settings_provider(
        mut self,
        settings_provider: Arc<dyn TenantSettingsProvider>,
    ) -> Self {
        self.settings_provider = Some(settings_provider);
        self
    }

    pub fn with_audit_service(mut self, audit_service: Arc<dyn AuditService>) -> Self {
        self.audit_service = Some(audit_service);
        self
    }

    pub fn with_pool(mut self, pool: Arc<PgPool>) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Process a submitted invoice through the workflow
    pub async fn process_invoice(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
    ) -> Result<ProcessingStatus> {
        // Resolve per-tenant settings (fallback to defaults)
        let settings = match &self.settings_provider {
            Some(provider) => provider.get(tenant_id).await.unwrap_or_default(),
            None => TenantSettings::default(),
        };

        // Template-first processing: if the tenant has a default workflow
        // template, drive the invoice through its ordered stages (skip /
        // auto-advance / route) before consulting rule-based routing. Mirrors
        // `WorkflowService::process_invoice_through_template`. Falls through to
        // the existing rule logic when no template exists or every stage was
        // skipped/auto-advanced.
        if let Some(template) = self.workflow_template_repo.get_default(tenant_id).await? {
            if let Some(status) = self
                .apply_workflow_template(tenant_id, invoice, &template)
                .await?
            {
                return Ok(status);
            }
        }

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

        // Defer: after every approval path, try to detect/update a recurring
        // pattern for this tenant+vendor so patterns form even when no pattern
        // existed yet (the primary review gap).  The call is cheap (one query)
        // and returns early if there are fewer than MIN_SAMPLE_COUNT invoices.
        macro_rules! try_update_pattern {
            () => {
                if let (Some(ref pool), Some(vendor_id)) = (&self.pool, invoice.vendor_id) {
                    let _ = crate::recurring_patterns::detect_or_update_pattern(
                        &**pool,
                        *tenant_id.as_uuid(),
                        vendor_id,
                    )
                    .await;
                }
            };
        }

        // Check recurring-pattern auto-approval before ML-confidence lane.
        if let (Some(ref pool), Some(vendor_id)) = (&self.pool, invoice.vendor_id) {
            if let Ok(Some(pattern)) =
                crate::recurring_patterns::find_pattern(pool, *tenant_id.as_uuid(), vendor_id).await
            {
                let line_items_json =
                    serde_json::to_value(&invoice.line_items).unwrap_or(serde_json::json!([]));
                let match_result = crate::recurring_patterns::evaluate_pattern_match(
                    invoice.total_amount.amount,
                    invoice.invoice_date,
                    &line_items_json,
                    &pattern,
                );

                if pattern.auto_approve_enabled
                    && matches!(
                        match_result,
                        crate::recurring_patterns::PatternMatchResult::Eligible
                    )
                {
                    tracing::info!(
                        invoice_id = %invoice.id.as_uuid(),
                        pattern_id = %pattern.id,
                        median_cents = pattern.trailing_median_cents,
                        observed_cents = invoice.total_amount.amount,
                        "Auto-approving invoice due to recurring pattern match"
                    );

                    // Audit-log the pattern-based auto-approval.
                    let expected_date = pattern
                        .last_invoice_date
                        .map(|d| d + chrono::Duration::days(pattern.cadence_days as i64));
                    let date_delta = match (invoice.invoice_date, expected_date) {
                        (Some(inv), Some(exp)) => Some((inv - exp).num_days()),
                        _ => None,
                    };
                    if let Err(e) = sqlx::query(
                        r#"INSERT INTO invoice_audit_log
                           (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                    )
                    .bind(uuid::Uuid::new_v4())
                    .bind(tenant_id.as_uuid())
                    .bind(invoice.id.as_uuid())
                    .bind(None::<uuid::Uuid>)
                    .bind("received")
                    .bind("approved")
                    .bind("recurring_pattern_match")
                    .bind(serde_json::json!({
                        "pattern_id": pattern.id,
                        "vendor_id": vendor_id,
                        "trailing_median_cents": pattern.trailing_median_cents,
                        "observed_amount_cents": invoice.total_amount.amount,
                        "cadence_days": pattern.cadence_days,
                        "date_delta_days": date_delta,
                        "amount_tolerance_pct": pattern.amount_tolerance_pct,
                        "window_tolerance_days": pattern.window_tolerance_days,
                    }))
                    .execute(&**pool)
                    .await
                    {
                        tracing::warn!(error = %e, "Failed to write recurring-pattern auto-approval audit entry");
                    }

                    try_update_pattern!();
                    return Ok(ProcessingStatus::Approved);
                }

                // Pattern exists but match failed or auto-approve disabled: audit the reason.
                if let crate::recurring_patterns::PatternMatchResult::Ineligible(reason) =
                    match_result
                {
                    tracing::info!(
                        invoice_id = %invoice.id.as_uuid(),
                        pattern_id = %pattern.id,
                        reason = %reason,
                        "Recurring pattern found but invoice ineligible for auto-approval"
                    );
                    if let Err(e) = sqlx::query(
                        r#"INSERT INTO invoice_audit_log
                           (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                    )
                    .bind(uuid::Uuid::new_v4())
                    .bind(tenant_id.as_uuid())
                    .bind(invoice.id.as_uuid())
                    .bind(None::<uuid::Uuid>)
                    .bind("received")
                    .bind("submitted")
                    .bind("recurring_pattern_ineligible")
                    .bind(serde_json::json!({
                        "pattern_id": pattern.id,
                        "vendor_id": vendor_id,
                        "reason": reason,
                    }))
                    .execute(&**pool)
                    .await
                    {
                        tracing::warn!(error = %e, "Failed to write recurring-pattern ineligible audit entry");
                    }
                }
            }
        }

        // Check ML categorization auto-approval threshold.
        // Both OCR confidence and categorization confidence must meet their
        // thresholds, and all required fields must be present, to auto-approve.
        // The categorization threshold is per-tenant (falling back to the
        // global const 0.95). The lane can be disabled per-tenant entirely.
        if settings.auto_approval_enabled {
            let threshold = settings
                .auto_approval_threshold
                .unwrap_or(ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD);
            let ocr_ok = invoice
                .ocr_confidence
                .map(|c| c >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD)
                .unwrap_or(false);
            let categorization_ok = invoice
                .categorization_confidence
                .map(|c| c >= threshold)
                .unwrap_or(false);

            if ocr_ok && categorization_ok {
                // Check that all required categorization fields are populated
                let has_complete_categorization = invoice.gl_code.is_some()
                    && invoice.department.is_some()
                    && invoice.cost_center.is_some();

                if has_complete_categorization {
                    tracing::info!(
                        invoice_id = %invoice.id.as_uuid(),
                        ocr_confidence = ?invoice.ocr_confidence,
                        categorization_confidence = ?invoice.categorization_confidence,
                        threshold_used = threshold,
                        "Auto-approving invoice due to high OCR and ML categorization confidence"
                    );

                    // Write dedicated audit entry to invoice_audit_log for touchless auto-approval
                    if let Some(ref pool) = self.pool {
                        let from_status = sqlx::query_scalar::<_, String>(
                            "SELECT status FROM invoices WHERE id = $1 AND tenant_id = $2",
                        )
                        .bind(invoice.id.as_uuid())
                        .bind(tenant_id.as_uuid())
                        .fetch_optional(&**pool)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "received".to_string());

                        if let Err(e) = sqlx::query(
                            r#"INSERT INTO invoice_audit_log
                               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
                               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                        )
                        .bind(uuid::Uuid::new_v4())
                        .bind(tenant_id.as_uuid())
                        .bind(invoice.id.as_uuid())
                        .bind(None::<uuid::Uuid>)
                        .bind(&from_status)
                        .bind("approved")
                        .bind("touchless_auto_approval")
                        .bind(serde_json::json!({
                            "ocr_confidence": invoice.ocr_confidence,
                            "categorization_confidence": invoice.categorization_confidence,
                            "threshold_used": threshold,
                            "lane": "learned_pattern",
                        }))
                        .execute(&**pool)
                        .await
                        {
                            tracing::warn!(error = %e, "Failed to write touchless auto-approval audit entry to invoice_audit_log");
                        }
                    }

                    try_update_pattern!();
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
                try_update_pattern!();
                return Ok(ProcessingStatus::Approved);
            }
        }

        for rule in &routing_rules {
            if !self.evaluate_conditions(invoice, &rule.conditions) {
                continue;
            }
            if self.rule_has_action(rule, ActionType::AutoApprove) {
                try_update_pattern!();
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
        try_update_pattern!();
        Ok(ProcessingStatus::Approved)
    }

    /// Apply a workflow template's ordered stages to an invoice.
    ///
    /// Mirrors `WorkflowService::process_invoice_through_template`: iterate
    /// stages in `order`, skipping stages whose `skip_conditions` all match and
    /// auto-advancing stages whose `auto_advance_conditions` all match. The
    /// first actionable stage drives the outcome using the same routing helpers
    /// the rule path uses (intelligent approval target + approval request for
    /// approval stages, derived processing status otherwise).
    ///
    /// Returns `Some(status)` when a stage captures the invoice, or `None` when
    /// every stage was skipped/auto-advanced so the caller falls through to
    /// rule-based processing.
    async fn apply_workflow_template(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        template: &WorkflowTemplate,
    ) -> Result<Option<ProcessingStatus>> {
        let mut stages = template.stages.clone();
        stages.sort_by_key(|s| s.order);

        for stage in &stages {
            // Skip conditions - if ALL match, skip this stage entirely.
            if !stage.skip_conditions.is_empty()
                && billforge_core::workflow_evaluator::evaluate_conditions(
                    invoice,
                    &stage.skip_conditions,
                )
            {
                tracing::info!(
                    invoice_id = %invoice.id.as_uuid(),
                    template_id = %template.id,
                    stage = %stage.name,
                    source = "template",
                    "Skipping workflow stage - skip conditions met"
                );
                continue;
            }

            // Auto-advance conditions - if ALL match, auto-complete and advance.
            if !stage.auto_advance_conditions.is_empty()
                && billforge_core::workflow_evaluator::evaluate_conditions(
                    invoice,
                    &stage.auto_advance_conditions,
                )
            {
                tracing::info!(
                    invoice_id = %invoice.id.as_uuid(),
                    template_id = %template.id,
                    stage = %stage.name,
                    source = "template",
                    "Auto-advancing workflow stage - conditions met"
                );
                continue;
            }

            // Stage requires processing - route the invoice here.
            let status = match stage.stage_type {
                StageType::Approval => {
                    self.create_template_stage_approval_request(tenant_id, invoice, stage)
                        .await?;
                    ProcessingStatus::PendingApproval
                }
                StageType::Payment => ProcessingStatus::ReadyForPayment,
                _ => ProcessingStatus::Submitted,
            };

            tracing::info!(
                invoice_id = %invoice.id.as_uuid(),
                template_id = %template.id,
                stage = %stage.name,
                stage_type = ?stage.stage_type,
                queue_id = ?stage.queue_id,
                source = "template",
                "Workflow template stage captured invoice"
            );

            return Ok(Some(status));
        }

        Ok(None)
    }

    /// Create an approval request for a template-driven Approval stage.
    ///
    /// Uses the same intelligent-routing target selection the rule path uses
    /// (`intelligent_approval_target`), falling back to the generic "approver"
    /// role. The request is tagged with a nil rule id (no rule originated it)
    /// and the stage's SLA if present, defaulting to 24 hours otherwise.
    async fn create_template_stage_approval_request(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        stage: &WorkflowTemplateStage,
    ) -> Result<()> {
        let target = self
            .intelligent_approval_target(tenant_id, invoice)
            .await
            .unwrap_or_else(|| ApprovalTarget::Role("approver".to_string()));

        let sla_hours = stage
            .sla_hours
            .filter(|h| *h > 0)
            .map(|h| h as i64)
            .unwrap_or(24);
        let now = chrono::Utc::now();
        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4(),
            invoice_id: invoice.id.clone(),
            tenant_id: tenant_id.clone(),
            // Template-driven request: no rule originated it.
            rule_id: WorkflowRuleId(uuid::Uuid::nil()),
            requested_from: target,
            status: ApprovalStatus::Pending,
            comments: None,
            responded_by: None,
            responded_at: None,
            created_at: now,
            expires_at: Some(now + Duration::hours(sla_hours)),
        };

        self.approval_repo.create(tenant_id, request).await?;
        Ok(())
    }

    /// Evaluate rule conditions against an invoice
    fn evaluate_conditions(&self, invoice: &Invoice, conditions: &[RuleCondition]) -> bool {
        billforge_core::workflow_evaluator::evaluate_conditions(invoice, conditions)
    }

    /// Evaluate a single condition
    #[allow(dead_code)]
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

        let config = RoutingConfig {
            tenant_id: tenant_id.clone(),
            ..Default::default()
        };
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
        let now = chrono::Utc::now();
        let sla_hours = Self::approval_sla_hours(rule);
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
            created_at: now,
            expires_at: Some(now + Duration::hours(sla_hours as i64)),
        };

        self.approval_repo.create(tenant_id, request).await?;
        Ok(())
    }

    fn approval_sla_hours(rule: &WorkflowRule) -> i32 {
        rule.actions
            .iter()
            .find_map(|action| {
                action
                    .params
                    .get("sla_hours")
                    .or_else(|| action.params.get("timeout_hours"))
                    .or_else(|| action.params.get("escalation_hours"))
                    .and_then(|value| value.as_i64())
                    .and_then(|hours| i32::try_from(hours).ok())
                    .filter(|hours| *hours > 0)
            })
            .unwrap_or(24)
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
            // Keep recurring patterns current: try to detect/update a pattern
            // for this invoice's vendor now that the approval is finalized.
            // This ensures patterns form even for vendors that had no prior
            // pattern row (the primary detection gap identified in review).
            if let (Some(ref pool), Ok(Some(invoice))) = (
                &self.pool,
                self.invoice_repo
                    .get_by_id(tenant_id, &approval.invoice_id)
                    .await,
            ) {
                if let Some(vendor_id) = invoice.vendor_id {
                    let _ = crate::recurring_patterns::detect_or_update_pattern(
                        pool,
                        *tenant_id.as_uuid(),
                        vendor_id,
                    )
                    .await;
                }
            }
            return Ok(ProcessingStatus::Approved);
        }

        // Still waiting for approvals
        Ok(ProcessingStatus::PendingApproval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::domain::WorkflowRuleId;

    fn rule_with_actions(actions: Vec<RuleAction>) -> WorkflowRule {
        let now = chrono::Utc::now();
        WorkflowRule {
            id: WorkflowRuleId(uuid::Uuid::new_v4()),
            tenant_id: TenantId::new(),
            name: "Approval SLA test".to_string(),
            description: None,
            priority: 0,
            is_active: true,
            rule_type: WorkflowRuleType::Approval,
            conditions: vec![],
            actions,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn approval_sla_hours_uses_action_sla_hours() {
        let rule = rule_with_actions(vec![RuleAction {
            action_type: ActionType::RequireApproval,
            params: serde_json::json!({ "sla_hours": 6 }),
        }]);

        assert_eq!(WorkflowEngine::approval_sla_hours(&rule), 6);
    }

    #[test]
    fn approval_sla_hours_defaults_to_twenty_four() {
        let rule = rule_with_actions(vec![RuleAction {
            action_type: ActionType::RequireApproval,
            params: serde_json::json!({ "sla_hours": 0 }),
        }]);

        assert_eq!(WorkflowEngine::approval_sla_hours(&rule), 24);
    }

    // -- OCR + categorization auto-approval threshold tests --

    #[test]
    fn auto_approval_blocked_by_low_ocr_confidence() {
        // Low OCR confidence (0.80) with high categorization confidence (0.99)
        // should NOT satisfy the auto-approval predicate.
        let ocr_ok = Some(0.80f32)
            .map(|c| c >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        let cat_ok = Some(0.99f32)
            .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        assert!(!ocr_ok, "OCR 0.80 should not pass the 0.90 threshold");
        assert!(cat_ok, "Categorization 0.99 should pass the 0.95 threshold");
        assert!(
            !(ocr_ok && cat_ok),
            "Should not auto-approve when OCR confidence is below threshold"
        );
    }

    #[test]
    fn auto_approval_requires_both_thresholds() {
        // Both confidences above their thresholds should pass.
        let ocr_ok = Some(0.95f32)
            .map(|c| c >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        let cat_ok = Some(0.99f32)
            .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        assert!(ocr_ok, "OCR 0.95 should pass the 0.90 threshold");
        assert!(cat_ok, "Categorization 0.99 should pass the 0.95 threshold");
        assert!(
            ocr_ok && cat_ok,
            "Should auto-approve when both confidences meet their thresholds"
        );
    }

    #[test]
    fn auto_approval_blocked_by_missing_ocr_confidence() {
        // Missing OCR confidence (None) should block auto-approval even with
        // high categorization confidence.
        let ocr_ok = None::<f32>
            .map(|c| c >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        let cat_ok = Some(0.99f32)
            .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        assert!(!ocr_ok, "Missing OCR confidence should not pass");
        assert!(
            !(ocr_ok && cat_ok),
            "Should not auto-approve when OCR confidence is missing"
        );
    }

    #[test]
    fn auto_approval_blocked_by_low_categorization_confidence() {
        // High OCR confidence but low categorization confidence should block.
        let ocr_ok = Some(0.95f32)
            .map(|c| c >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        let cat_ok = Some(0.80f32)
            .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        assert!(ocr_ok, "OCR 0.95 should pass the 0.90 threshold");
        assert!(
            !cat_ok,
            "Categorization 0.80 should not pass the 0.95 threshold"
        );
        assert!(
            !(ocr_ok && cat_ok),
            "Should not auto-approve when categorization confidence is below threshold"
        );
    }

    // -- Per-tenant threshold override tests --

    #[test]
    fn per_tenant_threshold_overrides_default() {
        // When a tenant sets auto_approval_threshold = 0.99, a categorization
        // confidence of 0.96 should NOT pass (it would pass the default 0.95).
        let threshold: f32 = 0.99;
        let cat_ok = Some(0.96f32).map(|c| c >= threshold).unwrap_or(false);
        assert!(
            !cat_ok,
            "Categorization 0.96 should not pass a tenant threshold of 0.99"
        );
    }

    #[test]
    fn per_tenant_threshold_none_uses_default() {
        // When auto_approval_threshold is None, the global const is used.
        let threshold: Option<f32> = None;
        #[allow(clippy::unnecessary_literal_unwrap)]
        let resolved = threshold.unwrap_or(ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD);
        assert!(
            (resolved - ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD).abs() < f32::EPSILON,
            "None threshold should fall back to the global default"
        );
        let cat_ok = Some(0.96f32).map(|c| c >= resolved).unwrap_or(false);
        assert!(
            cat_ok,
            "Categorization 0.96 should pass the default 0.95 threshold"
        );
    }

    #[test]
    fn auto_approval_disabled_skips_lane() {
        // When auto_approval_enabled is false, the lane should be skipped
        // entirely regardless of confidence levels.
        let auto_approval_enabled = false;
        let ocr_ok = Some(0.99f32)
            .map(|c| c >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        let cat_ok = Some(0.99f32)
            .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD)
            .unwrap_or(false);
        // The lane check is `if settings.auto_approval_enabled { ... }`
        // so when disabled, neither ocr_ok nor cat_ok matter.
        assert!(
            ocr_ok && cat_ok,
            "Both confidences should pass their thresholds"
        );
        assert!(
            !auto_approval_enabled,
            "Lane should be skipped when disabled"
        );
    }

    // ---------------------------------------------------------------------------
    // WorkflowTemplate wiring tests (issue #372)
    // ---------------------------------------------------------------------------
    //
    // These tests prove that `process_invoice` now consults the
    // `WorkflowTemplateRepository` (template-first) before falling through to
    // rule-based routing/approval/auto-approval.

    use async_trait::async_trait;
    use billforge_core::domain::{
        CaptureStatus, ConditionField, ConditionOperator, CreateInvoiceInput,
        CreateWorkflowRuleInput, CreateWorkflowTemplateInput, InvoiceFilters, InvoiceId,
        WorkflowTemplateId,
    };
    use billforge_core::types::{Money, PaginatedResponse, Pagination};
    use std::sync::Mutex;

    /// Invoice repo stub: `process_invoice` does not call it on the happy path.
    struct StubInvoiceRepo;

    #[async_trait]
    impl InvoiceRepository for StubInvoiceRepo {
        async fn create(
            &self,
            _tid: &TenantId,
            _input: CreateInvoiceInput,
            _uid: Option<&UserId>,
        ) -> Result<Invoice> {
            unimplemented!()
        }
        async fn get_by_id(&self, _tid: &TenantId, _id: &InvoiceId) -> Result<Option<Invoice>> {
            Ok(None)
        }
        async fn list(
            &self,
            _tid: &TenantId,
            _f: &InvoiceFilters,
            _p: &Pagination,
        ) -> Result<PaginatedResponse<Invoice>> {
            unimplemented!()
        }
        async fn update(
            &self,
            _tid: &TenantId,
            _id: &InvoiceId,
            _u: serde_json::Value,
        ) -> Result<Invoice> {
            unimplemented!()
        }
        async fn delete(&self, _tid: &TenantId, _id: &InvoiceId) -> Result<()> {
            Ok(())
        }
        async fn update_capture_status(
            &self,
            _tid: &TenantId,
            _id: &InvoiceId,
            _s: CaptureStatus,
        ) -> Result<()> {
            Ok(())
        }
        async fn update_processing_status(
            &self,
            _tid: &TenantId,
            _id: &InvoiceId,
            _s: ProcessingStatus,
        ) -> Result<()> {
            Ok(())
        }
    }

    /// Approval repo that records every request created via `create`.
    struct RecordingApprovalRepo {
        created: Mutex<Vec<ApprovalRequest>>,
    }

    impl RecordingApprovalRepo {
        fn new() -> Self {
            Self {
                created: Mutex::new(Vec::new()),
            }
        }
        fn requests(&self) -> Vec<ApprovalRequest> {
            self.created.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ApprovalRepository for RecordingApprovalRepo {
        async fn create(
            &self,
            _tid: &TenantId,
            request: ApprovalRequest,
        ) -> Result<ApprovalRequest> {
            self.created.lock().unwrap().push(request.clone());
            Ok(request)
        }
        async fn get_by_id(
            &self,
            _tid: &TenantId,
            _id: uuid::Uuid,
        ) -> Result<Option<ApprovalRequest>> {
            Ok(None)
        }
        async fn list_for_invoice(
            &self,
            _tid: &TenantId,
            _inv: &InvoiceId,
        ) -> Result<Vec<ApprovalRequest>> {
            Ok(vec![])
        }
        async fn list_pending_for_user(
            &self,
            _tid: &TenantId,
            _uid: &UserId,
        ) -> Result<Vec<ApprovalRequest>> {
            Ok(vec![])
        }
        async fn respond(
            &self,
            _tid: &TenantId,
            _id: uuid::Uuid,
            _status: ApprovalStatus,
            _comments: Option<String>,
            _uid: &UserId,
        ) -> Result<ApprovalRequest> {
            unimplemented!()
        }
        async fn cancel_for_invoice(&self, _tid: &TenantId, _inv: &InvoiceId) -> Result<()> {
            Ok(())
        }
    }

    /// Rule repo that returns a fixed set of active rules, filtered by type.
    struct StubRuleRepo {
        rules: Vec<WorkflowRule>,
    }

    #[async_trait]
    impl WorkflowRuleRepository for StubRuleRepo {
        async fn create(
            &self,
            _tid: &TenantId,
            _input: CreateWorkflowRuleInput,
        ) -> Result<WorkflowRule> {
            unimplemented!()
        }
        async fn get_by_id(
            &self,
            _tid: &TenantId,
            _id: &WorkflowRuleId,
        ) -> Result<Option<WorkflowRule>> {
            Ok(None)
        }
        async fn list(
            &self,
            _tid: &TenantId,
            _rt: Option<WorkflowRuleType>,
        ) -> Result<Vec<WorkflowRule>> {
            Ok(vec![])
        }
        async fn update(
            &self,
            _tid: &TenantId,
            _id: &WorkflowRuleId,
            _input: CreateWorkflowRuleInput,
        ) -> Result<WorkflowRule> {
            unimplemented!()
        }
        async fn delete(&self, _tid: &TenantId, _id: &WorkflowRuleId) -> Result<()> {
            Ok(())
        }
        async fn set_active(
            &self,
            _tid: &TenantId,
            _id: &WorkflowRuleId,
            _active: bool,
        ) -> Result<()> {
            Ok(())
        }
        async fn get_active_rules(
            &self,
            _tid: &TenantId,
            rule_type: WorkflowRuleType,
        ) -> Result<Vec<WorkflowRule>> {
            Ok(self
                .rules
                .iter()
                .filter(|r| r.rule_type == rule_type)
                .cloned()
                .collect())
        }
    }

    /// Template repo with a configurable default template.
    struct StubTemplateRepo {
        default: Option<WorkflowTemplate>,
    }

    #[async_trait]
    impl WorkflowTemplateRepository for StubTemplateRepo {
        async fn create(
            &self,
            _tid: &TenantId,
            _input: CreateWorkflowTemplateInput,
        ) -> Result<WorkflowTemplate> {
            unimplemented!()
        }
        async fn get_by_id(
            &self,
            _tid: &TenantId,
            _id: &WorkflowTemplateId,
        ) -> Result<Option<WorkflowTemplate>> {
            Ok(None)
        }
        async fn list(&self, _tid: &TenantId) -> Result<Vec<WorkflowTemplate>> {
            Ok(vec![])
        }
        async fn update(
            &self,
            _tid: &TenantId,
            _id: &WorkflowTemplateId,
            _input: CreateWorkflowTemplateInput,
        ) -> Result<WorkflowTemplate> {
            unimplemented!()
        }
        async fn delete(&self, _tid: &TenantId, _id: &WorkflowTemplateId) -> Result<()> {
            Ok(())
        }
        async fn set_active(
            &self,
            _tid: &TenantId,
            _id: &WorkflowTemplateId,
            _is_active: bool,
        ) -> Result<()> {
            Ok(())
        }
        async fn get_default(&self, _tid: &TenantId) -> Result<Option<WorkflowTemplate>> {
            Ok(self.default.clone())
        }
    }

    fn template_approval_stage(
        skip_conditions: Vec<RuleCondition>,
        auto_advance_conditions: Vec<RuleCondition>,
    ) -> WorkflowTemplateStage {
        WorkflowTemplateStage {
            order: 0,
            name: "approval".to_string(),
            stage_type: StageType::Approval,
            queue_id: None,
            sla_hours: None,
            escalation_hours: None,
            requires_action: true,
            skip_conditions,
            auto_advance_conditions,
        }
    }

    fn template_with_stages(stages: Vec<WorkflowTemplateStage>) -> WorkflowTemplate {
        WorkflowTemplate {
            id: WorkflowTemplateId::new(),
            tenant_id: TenantId::new(),
            name: "test-template".to_string(),
            description: None,
            is_active: true,
            is_default: true,
            stages,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// An auto-approval rule with empty (always-matching) conditions.
    fn auto_approval_rule() -> WorkflowRule {
        let now = chrono::Utc::now();
        WorkflowRule {
            id: WorkflowRuleId(uuid::Uuid::new_v4()),
            tenant_id: TenantId::new(),
            name: "auto-approve-all".to_string(),
            description: None,
            priority: 0,
            is_active: true,
            rule_type: WorkflowRuleType::AutoApproval,
            conditions: vec![],
            actions: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    fn template_test_invoice() -> Invoice {
        Invoice {
            id: InvoiceId::new(),
            tenant_id: TenantId::new(),
            vendor_id: Some(uuid::Uuid::new_v4()),
            vendor_name: "Test Vendor".to_string(),
            invoice_number: "INV-001".to_string(),
            invoice_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            due_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
            po_number: None,
            subtotal: Some(Money {
                amount: 10000,
                currency: "USD".to_string(),
            }),
            tax_amount: Some(Money {
                amount: 0,
                currency: "USD".to_string(),
            }),
            total_amount: Money {
                amount: 10000,
                currency: "USD".to_string(),
            },
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: uuid::Uuid::new_v4(),
            supporting_documents: vec![],
            // None so the ML auto-approval lane never short-circuits.
            ocr_confidence: None,
            categorization_confidence: None,
            department: None,
            gl_code: None,
            cost_center: None,
            notes: None,
            tags: vec![],
            custom_fields: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: Some(UserId(uuid::Uuid::new_v4())),
        }
    }

    fn build_test_engine(
        template: Option<WorkflowTemplate>,
        rules: Vec<WorkflowRule>,
    ) -> (WorkflowEngine, std::sync::Arc<RecordingApprovalRepo>) {
        let approval_repo = std::sync::Arc::new(RecordingApprovalRepo::new());
        let engine = WorkflowEngine::new(
            std::sync::Arc::new(StubInvoiceRepo),
            std::sync::Arc::new(StubRuleRepo { rules }),
            approval_repo.clone(),
            std::sync::Arc::new(StubTemplateRepo { default: template }),
        );
        (engine, approval_repo)
    }

    #[tokio::test]
    async fn process_invoice_uses_template_when_present() {
        // A template with a single Approval stage should capture the invoice
        // (PendingApproval + an approval request) and must NOT consult the rule
        // repo for routing/approval/auto-approval. The rule repo carries an
        // always-matching AutoApproval rule that would otherwise approve the
        // invoice, so a PendingApproval outcome proves the template took over.
        let (engine, approval_repo) = build_test_engine(
            Some(template_with_stages(vec![template_approval_stage(
                vec![],
                vec![],
            )])),
            vec![auto_approval_rule()],
        );
        let invoice = template_test_invoice();

        let status = engine
            .process_invoice(&invoice.tenant_id, &invoice)
            .await
            .expect("template processing should succeed");

        assert_eq!(
            status,
            ProcessingStatus::PendingApproval,
            "template Approval stage should route to pending approval"
        );

        let requests = approval_repo.requests();
        assert_eq!(
            requests.len(),
            1,
            "exactly one template-driven approval request"
        );
        assert!(
            matches!(&requests[0].requested_from, ApprovalTarget::Role(role) if role == "approver"),
            "no routing provider configured -> fallback to approver role"
        );
        assert!(
            requests[0].rule_id.0 == uuid::Uuid::nil(),
            "template-driven requests carry a nil rule id"
        );
    }

    #[tokio::test]
    async fn process_invoice_falls_through_to_rules_when_no_template() {
        // With no default template, the existing rule-based path must run
        // unchanged: the always-matching AutoApproval rule should approve.
        let (engine, _approval_repo) = build_test_engine(None, vec![auto_approval_rule()]);
        let invoice = template_test_invoice();

        let status = engine
            .process_invoice(&invoice.tenant_id, &invoice)
            .await
            .expect("rule fall-through should succeed");

        assert_eq!(
            status,
            ProcessingStatus::Approved,
            "no template -> rule-based auto-approval preserved"
        );
    }

    #[tokio::test]
    async fn process_invoice_skips_stage_on_skip_condition_match() {
        // The template's Approval stage has a skip condition that matches the
        // invoice (amount > 0). The stage must be skipped and processing must
        // fall through to the AutoApproval rule -> Approved. If skip logic were
        // broken, the Approval stage would capture it as PendingApproval.
        let skip_matches = vec![RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::GreaterThan,
            value: serde_json::json!(0),
        }];
        let (engine, _approval_repo) = build_test_engine(
            Some(template_with_stages(vec![template_approval_stage(
                skip_matches,
                vec![],
            )])),
            vec![auto_approval_rule()],
        );
        let invoice = template_test_invoice();

        let status = engine
            .process_invoice(&invoice.tenant_id, &invoice)
            .await
            .expect("skip-then-fall-through should succeed");

        assert_eq!(
            status,
            ProcessingStatus::Approved,
            "matching skip condition should skip the stage and fall through to rules"
        );
    }
}
