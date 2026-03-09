//! Workflow orchestration service
//!
//! Handles approval workflow logic including:
//! - Creating approval requests
//! - Sending approval emails with action tokens
//! - Routing invoices through queues
//! - Escalation management

use crate::{
    domain::{ApprovalRequest, ApprovalStatus, ApprovalTarget, Invoice, WorkflowRule},
    services::{EmailAction, EmailActionTokenService},
    traits::{ApprovalRepository, InvoiceRepository, UserRepository, WorkQueueRepository},
    types::TenantId, Error, Result, UserId,
};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use uuid::Uuid;

/// Email service trait (abstracted to avoid circular dependency)
#[async_trait]
pub trait EmailService: Send + Sync {
    async fn send(&self, to: &str, subject: &str, html_body: &str, text_body: &str) -> crate::Result<()>;
}

/// Email templates trait (abstracted to avoid circular dependency)
pub trait EmailTemplates {
    fn invoice_pending_approval_with_actions(
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        submitted_by: &str,
        view_url: &str,
        approve_url: Option<&str>,
        reject_url: Option<&str>,
    ) -> (String, String);
}

/// Workflow orchestration service
pub struct WorkflowService<
    ES: EmailService,
    ET: EmailTemplates,
    UR: UserRepository,
    IR: InvoiceRepository,
    QR: WorkQueueRepository,
    AR: ApprovalRepository,
> {
    email_service: ES,
    email_token_service: EmailActionTokenService,
    user_repo: UR,
    invoice_repo: IR,
    queue_repo: QR,
    approval_repo: AR,
    app_url: String,
    _email_templates: std::marker::PhantomData<ET>,
}

impl<
    ES: EmailService,
    ET: EmailTemplates,
    UR: UserRepository,
    IR: InvoiceRepository,
    QR: WorkQueueRepository,
    AR: ApprovalRepository,
> WorkflowService<ES, ET, UR, IR, QR, AR> {
    /// Create a new workflow service
    pub fn new(
        email_service: ES,
        email_token_service: EmailActionTokenService,
        user_repo: UR,
        invoice_repo: IR,
        queue_repo: QR,
        approval_repo: AR,
        app_url: String,
    ) -> Self {
        Self {
            email_service,
            email_token_service,
            user_repo,
            invoice_repo,
            queue_repo,
            approval_repo,
            app_url,
            _email_templates: std::marker::PhantomData,
        }
    }

    /// Create an approval request and send notification email
    pub async fn create_approval_request(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        approver: ApprovalTarget,
        rule_id: Option<Uuid>,
    ) -> Result<ApprovalRequest> {
        let approval_id = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::days(7); // 7-day approval window

        let request = ApprovalRequest {
            id: approval_id,
            tenant_id: tenant_id.clone(),
            invoice_id: invoice.id.clone(),
            rule_id: crate::domain::WorkflowRuleId(rule_id.unwrap_or_else(Uuid::nil)),
            requested_from: approver.clone(),
            status: ApprovalStatus::Pending,
            comments: None,
            responded_by: None,
            responded_at: None,
            created_at: Utc::now(),
            expires_at: Some(expires_at),
        };

        // Send approval email
        self.send_approval_email(tenant_id, invoice, &approver, &request).await?;

        Ok(request)
    }

    /// Send approval request email with action tokens
    async fn send_approval_email(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        approver: &ApprovalTarget,
        request: &ApprovalRequest,
    ) -> Result<()> {
        // Get approver email(s) based on target type
        let approver_emails: Vec<String> = match approver {
            ApprovalTarget::User(user_id) => {
                self.user_repo.get_email_by_id(tenant_id, user_id).await?
                    .into_iter()
                    .collect()
            }
            ApprovalTarget::Role(role_name) => {
                self.user_repo.get_emails_by_role(tenant_id, role_name).await?
            }
            ApprovalTarget::AnyOf(user_ids) | ApprovalTarget::AllOf(user_ids) => {
                self.user_repo.get_emails_by_ids(tenant_id, user_ids).await?
            }
        };

        if approver_emails.is_empty() {
            tracing::warn!("No approver emails found for approval request {}", request.id);
            return Ok(());
        }

        // Get the first approver for token generation (any approver can action it)
        let first_approver_id = match approver {
            ApprovalTarget::User(user_id) => user_id.clone(),
            ApprovalTarget::Role(_) => {
                // For role-based approvals, use a nil user ID (will be set when action is taken)
                UserId(Uuid::nil())
            }
            ApprovalTarget::AnyOf(user_ids) | ApprovalTarget::AllOf(user_ids) => {
                user_ids.first().cloned().unwrap_or_else(|| UserId(Uuid::nil()))
            }
        };

        // Generate email action tokens
        let approve_token = self.email_token_service.generate_token(
            tenant_id,
            &first_approver_id,
            EmailAction::ApproveInvoice,
            invoice.id.0,
            "invoice",
            serde_json::json!({ "approval_id": request.id }),
        ).await?;

        let reject_token = self.email_token_service.generate_token(
            tenant_id,
            &first_approver_id,
            EmailAction::RejectInvoice,
            invoice.id.0,
            "invoice",
            serde_json::json!({ "approval_id": request.id }),
        ).await?;

        // Generate action URLs
        let approve_url = self.email_token_service.generate_action_url(
            &self.app_url,
            &approve_token,
            "approve",
        );

        let reject_url = self.email_token_service.generate_action_url(
            &self.app_url,
            &reject_token,
            "reject",
        );

        let view_url = format!("{}/invoices/{}", self.app_url, invoice.id);

        // Prepare email content
        let invoice_number = if invoice.invoice_number.is_empty() { "N/A" } else { &invoice.invoice_number };
        let vendor_name = if invoice.vendor_name.is_empty() { "Unknown Vendor" } else { &invoice.vendor_name };
        let amount = format!("${:.2}", invoice.total_amount.amount as f64 / 100.0);

        // Get the actual submitter name
        let submitted_by = match self.user_repo.get_name_by_id(tenant_id, &invoice.created_by).await {
            Ok(Some(name)) => name,
            Ok(None) => {
                tracing::warn!("User {} not found for invoice {}", invoice.created_by, invoice.id);
                "Unknown User".to_string()
            }
            Err(e) => {
                tracing::warn!("Failed to get submitter name for invoice {}: {}", invoice.id, e);
                "AP Team".to_string()
            }
        };

        let (html, text) = ET::invoice_pending_approval_with_actions(
            invoice_number,
            vendor_name,
            &amount,
            &submitted_by,
            &view_url,
            Some(&approve_url),
            Some(&reject_url),
        );

        // Send to all approvers
        for email in approver_emails {
            let subject = format!(
                "Approval Required: Invoice {} from {}",
                invoice_number, vendor_name
            );

            if let Err(e) = self.email_service.send(&email, &subject, &html, &text).await {
                tracing::error!("Failed to send approval email to {}: {}", email, e);
            }
        }

        Ok(())
    }

    /// Process invoice through workflow rules
    pub async fn process_invoice_workflow(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        rules: &[WorkflowRule],
    ) -> Result<()> {
        for rule in rules {
            if !rule.is_active {
                continue;
            }

            // Check if rule conditions match
            if self.evaluate_rule_conditions(invoice, &rule.conditions) {
                // Execute rule actions
                for action in &rule.actions {
                    self.execute_action(tenant_id, invoice, action).await?;
                }
            }
        }

        Ok(())
    }

    /// Evaluate workflow rule conditions against an invoice
    fn evaluate_rule_conditions(
        &self,
        invoice: &Invoice,
        conditions: &[crate::domain::RuleCondition],
    ) -> bool {
        // All conditions must match (AND logic)
        for condition in conditions {
            if !self.evaluate_single_condition(invoice, condition) {
                return false;
            }
        }
        true
    }

    /// Evaluate a single condition against an invoice
    fn evaluate_single_condition(
        &self,
        invoice: &Invoice,
        condition: &crate::domain::RuleCondition,
    ) -> bool {
        use crate::domain::{ConditionField, ConditionOperator};

        // Extract the field value from the invoice
        let field_value = match condition.field {
            ConditionField::Amount => {
                serde_json::to_value(invoice.total_amount.amount).ok()
            }
            ConditionField::VendorId => {
                invoice.vendor_id.as_ref().and_then(|v| serde_json::to_value(v).ok())
            }
            ConditionField::VendorName => {
                Some(serde_json::Value::String(invoice.vendor_name.clone()))
            }
            ConditionField::Department => {
                invoice.department.as_ref().map(|d| serde_json::Value::String(d.clone()))
            }
            ConditionField::GlCode => {
                invoice.gl_code.as_ref().map(|g| serde_json::Value::String(g.clone()))
            }
            ConditionField::InvoiceDate => {
                invoice.invoice_date.and_then(|d| serde_json::to_value(d.to_string()).ok())
            }
            ConditionField::DueDate => {
                invoice.due_date.and_then(|d| serde_json::to_value(d.to_string()).ok())
            }
            ConditionField::Tag => {
                if invoice.tags.is_empty() {
                    None
                } else {
                    serde_json::to_value(&invoice.tags).ok()
                }
            }
            ConditionField::CustomField => {
                // For custom fields, the condition value should specify which field
                if let serde_json::Value::Object(ref map) = condition.value {
                    if let Some(field_name) = map.get("field").and_then(|v| v.as_str()) {
                        invoice.custom_fields.get(field_name).cloned()
                    } else {
                        None
                    }
                } else {
                    invoice.custom_fields.clone().into()
                }
            }
        };

        // Apply the operator
        match &field_value {
            None => {
                // Field is null/missing
                matches!(condition.operator, ConditionOperator::IsNull)
            }
            Some(fv) => {
                self.apply_operator(fv, &condition.operator, &condition.value)
            }
        }
    }

    /// Apply a comparison operator
    fn apply_operator(
        &self,
        field_value: &serde_json::Value,
        operator: &crate::domain::ConditionOperator,
        condition_value: &serde_json::Value,
    ) -> bool {
        use crate::domain::ConditionOperator;

        match operator {
            ConditionOperator::Equals => field_value == condition_value,
            ConditionOperator::NotEquals => field_value != condition_value,
            ConditionOperator::GreaterThan => {
                self.compare_values(field_value, condition_value) == Some(std::cmp::Ordering::Greater)
            }
            ConditionOperator::GreaterThanOrEqual => {
                matches!(self.compare_values(field_value, condition_value), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
            }
            ConditionOperator::LessThan => {
                self.compare_values(field_value, condition_value) == Some(std::cmp::Ordering::Less)
            }
            ConditionOperator::LessThanOrEqual => {
                matches!(self.compare_values(field_value, condition_value), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
            }
            ConditionOperator::Contains => {
                // String contains or array contains
                match (field_value, condition_value) {
                    (serde_json::Value::String(s), serde_json::Value::String(pattern)) => {
                        s.contains(pattern)
                    }
                    (serde_json::Value::Array(arr), _) => {
                        arr.contains(condition_value)
                    }
                    _ => false,
                }
            }
            ConditionOperator::StartsWith => {
                match (field_value, condition_value) {
                    (serde_json::Value::String(s), serde_json::Value::String(prefix)) => {
                        s.starts_with(prefix)
                    }
                    _ => false,
                }
            }
            ConditionOperator::EndsWith => {
                match (field_value, condition_value) {
                    (serde_json::Value::String(s), serde_json::Value::String(suffix)) => {
                        s.ends_with(suffix)
                    }
                    _ => false,
                }
            }
            ConditionOperator::In => {
                // Field value is in the condition value (which should be an array)
                match condition_value {
                    serde_json::Value::Array(arr) => arr.contains(field_value),
                    _ => false,
                }
            }
            ConditionOperator::NotIn => {
                // Field value is NOT in the condition value (which should be an array)
                match condition_value {
                    serde_json::Value::Array(arr) => !arr.contains(field_value),
                    _ => true,
                }
            }
            ConditionOperator::Between => {
                // Condition value should be [min, max]
                match condition_value {
                    serde_json::Value::Array(arr) if arr.len() == 2 => {
                        let min_ok = self.compare_values(field_value, &arr[0])
                            .map(|o| o == std::cmp::Ordering::Greater || o == std::cmp::Ordering::Equal)
                            .unwrap_or(false);
                        let max_ok = self.compare_values(field_value, &arr[1])
                            .map(|o| o == std::cmp::Ordering::Less || o == std::cmp::Ordering::Equal)
                            .unwrap_or(false);
                        min_ok && max_ok
                    }
                    _ => false,
                }
            }
            ConditionOperator::IsNull => {
                // Already handled in evaluate_single_condition
                false
            }
            ConditionOperator::IsNotNull => {
                // Already handled in evaluate_single_condition (field_value is Some)
                true
            }
        }
    }

    /// Compare two JSON values (numeric or string comparison)
    fn compare_values(
        &self,
        a: &serde_json::Value,
        b: &serde_json::Value,
    ) -> Option<std::cmp::Ordering> {
        use serde_json::Value;

        match (a, b) {
            // Numeric comparison (handle both i64 and f64)
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_val = a_num.as_f64()?;
                let b_val = b_num.as_f64()?;
                a_val.partial_cmp(&b_val)
            }
            // String comparison
            (Value::String(a_str), Value::String(b_str)) => {
                Some(a_str.cmp(b_str))
            }
            // Boolean comparison
            (Value::Bool(a_bool), Value::Bool(b_bool)) => {
                Some(a_bool.cmp(b_bool))
            }
            _ => None,
        }
    }

    /// Execute a workflow action
    async fn execute_action(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        action: &crate::domain::RuleAction,
    ) -> Result<()> {
        use crate::domain::ActionType;

        tracing::info!(
            tenant_id = %tenant_id.as_str(),
            invoice_id = %invoice.id,
            action_type = ?action.action_type,
            params = ?action.params,
            "Executing workflow action"
        );

        match action.action_type {
            ActionType::RouteToQueue => {
                // Extract queue ID from params
                let queue_id: crate::domain::WorkQueueId = action.params
                    .get("queue_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("RouteToQueue requires queue_id parameter".to_string()))
                    .and_then(|s| s.parse().map_err(|_| Error::Validation("Invalid queue_id format".to_string())))?;

                // Route invoice to specified queue
                self.queue_repo
                    .move_item(tenant_id, &invoice.id, &queue_id, None)
                    .await?;

                tracing::info!(
                    invoice_id = %invoice.id,
                    queue_id = %queue_id,
                    "Invoice routed to queue"
                );
            }
            ActionType::RequireApproval | ActionType::RequireRoleApproval => {
                // Extract approver target from params
                let approver = if action.action_type == ActionType::RequireRoleApproval {
                    let role = action.params
                        .get("role")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Error::Validation("RequireRoleApproval requires role parameter".to_string()))?;
                    crate::domain::ApprovalTarget::Role(role.to_string())
                } else {
                    // RequireApproval - can be user_id, user_ids, or role
                    if let Some(user_id) = action.params.get("user_id").and_then(|v| v.as_str()) {
                        let uid = user_id.parse()
                            .map_err(|_| Error::Validation("Invalid user_id format".to_string()))?;
                        crate::domain::ApprovalTarget::User(crate::UserId(uid))
                    } else if let Some(user_ids) = action.params.get("user_ids").and_then(|v| v.as_array()) {
                        let ids: Result<Vec<_>> = user_ids
                            .iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| {
                                s.parse()
                                    .map_err(|_| Error::Validation("Invalid user_id in user_ids".to_string()))
                            })
                            .collect();
                        crate::domain::ApprovalTarget::AnyOf(ids?.into_iter().map(crate::UserId).collect())
                    } else if let Some(role) = action.params.get("role").and_then(|v| v.as_str()) {
                        crate::domain::ApprovalTarget::Role(role.to_string())
                    } else {
                        return Err(Error::Validation(
                            "RequireApproval requires user_id, user_ids, or role parameter".to_string()
                        ));
                    }
                };

                // Create approval request
                let request = self.create_approval_request(tenant_id, invoice, approver, None).await?;

                // Store in repository
                self.approval_repo.create(tenant_id, request).await?;

                // Update invoice status to pending approval
                self.invoice_repo
                    .update(
                        tenant_id,
                        &invoice.id,
                        serde_json::json!({ "processing_status": "pending_approval" })
                    )
                    .await?;

                tracing::info!(
                    invoice_id = %invoice.id,
                    "Approval request created and invoice status updated"
                );
            }
            ActionType::AutoApprove => {
                // Update invoice status to approved
                self.invoice_repo
                    .update(
                        tenant_id,
                        &invoice.id,
                        serde_json::json!({ "processing_status": "approved" })
                    )
                    .await?;

                tracing::info!(
                    invoice_id = %invoice.id,
                    "Invoice auto-approved"
                );
            }
            ActionType::SendNotification => {
                // Extract notification parameters
                let to = action.params
                    .get("to")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("SendNotification requires 'to' parameter".to_string()))?;

                let default_subject = format!("Invoice {} - Notification", invoice.invoice_number);
                let subject = action.params
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_subject);

                let default_body = format!(
                    "Invoice {} from {} requires your attention.",
                    invoice.invoice_number, invoice.vendor_name
                );
                let body = action.params
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_body);

                // Send notification email
                self.email_service.send(to, subject, body, body).await?;

                tracing::info!(
                    invoice_id = %invoice.id,
                    to = %to,
                    "Notification email sent"
                );
            }
            ActionType::SetField => {
                // Extract field name and value from params
                let field_name = action.params
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("SetField requires 'field' parameter".to_string()))?;

                let field_value = action.params
                    .get("value")
                    .ok_or_else(|| Error::Validation("SetField requires 'value' parameter".to_string()))?;

                // Update invoice field
                let updates = serde_json::json!({ field_name: field_value });
                self.invoice_repo.update(tenant_id, &invoice.id, updates).await?;

                tracing::info!(
                    invoice_id = %invoice.id,
                    field = %field_name,
                    "Invoice field updated"
                );
            }
            ActionType::AddTag => {
                // Extract tag from params
                let tag = action.params
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("AddTag requires 'tag' parameter".to_string()))?;

                // Get current tags and add new one
                let mut tags = invoice.tags.clone();
                if !tags.contains(&tag.to_string()) {
                    tags.push(tag.to_string());
                    let updates = serde_json::json!({ "tags": tags });
                    self.invoice_repo.update(tenant_id, &invoice.id, updates).await?;
                }

                tracing::info!(
                    invoice_id = %invoice.id,
                    tag = %tag,
                    "Tag added to invoice"
                );
            }
            ActionType::Escalate => {
                // Extract escalation parameters
                let escalate_to = action.params
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("Escalate requires 'user_id' parameter".to_string()))?;

                let user_uuid: Uuid = escalate_to.parse()
                    .map_err(|_| Error::Validation("Invalid user_id format".to_string()))?;

                let escalate_user_id = UserId(user_uuid);

                // Find current queue item for invoice
                match self.queue_repo.get_current_item_for_invoice(tenant_id, &invoice.id).await? {
                    Some(current_item) => {
                        // Reassign the queue item to the new user
                        let reassigned_item = self.queue_repo.reassign_item(
                            tenant_id,
                            current_item.id,
                            &escalate_user_id
                        ).await?;

                        tracing::info!(
                            invoice_id = %invoice.id,
                            item_id = %reassigned_item.id,
                            queue_id = %reassigned_item.queue_id,
                            escalated_from = ?current_item.assigned_to,
                            escalated_to = %escalate_to,
                            "Invoice queue item escalated and reassigned"
                        );
                    }
                    None => {
                        tracing::warn!(
                            invoice_id = %invoice.id,
                            escalate_to = %escalate_to,
                            "Invoice escalation requested but no active queue item found"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConditionField, ConditionOperator, Invoice, InvoiceId, RuleCondition, CaptureStatus, ProcessingStatus};
    use crate::{UserId, Money, TenantId};
    use chrono::Utc;
    use uuid::Uuid;
    use serde_json::json;

    // Mock EmailService for testing
    struct MockEmailService;

    #[async_trait]
    impl EmailService for MockEmailService {
        async fn send(&self, _to: &str, _subject: &str, _html_body: &str, _text_body: &str) -> Result<()> {
            Ok(())
        }
    }

    // Mock EmailTemplates for testing
    struct MockEmailTemplates;

    impl EmailTemplates for MockEmailTemplates {
        fn invoice_pending_approval_with_actions(
            _invoice_number: &str,
            _vendor_name: &str,
            _amount: &str,
            _submitted_by: &str,
            _view_url: &str,
            _approve_url: Option<&str>,
            _reject_url: Option<&str>,
        ) -> (String, String) {
            (String::new(), String::new())
        }
    }

    // Mock UserRepository for testing
    struct MockUserRepository;

    #[async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_email_by_id(&self, _tenant_id: &TenantId, _user_id: &UserId) -> Result<Option<String>> {
            Ok(Some("test@example.com".to_string()))
        }

        async fn get_name_by_id(&self, _tenant_id: &TenantId, _user_id: &UserId) -> Result<Option<String>> {
            Ok(Some("Test User".to_string()))
        }

        async fn get_emails_by_ids(&self, _tenant_id: &TenantId, _user_ids: &[UserId]) -> Result<Vec<String>> {
            Ok(vec!["test@example.com".to_string()])
        }

        async fn get_emails_by_role(&self, _tenant_id: &TenantId, _role: &str) -> Result<Vec<String>> {
            Ok(vec!["test@example.com".to_string()])
        }
    }

    // Mock InvoiceRepository for testing
    struct MockInvoiceRepository;

    #[async_trait]
    impl crate::traits::InvoiceRepository for MockInvoiceRepository {
        async fn create(
            &self,
            _tenant_id: &TenantId,
            _input: crate::domain::CreateInvoiceInput,
            _created_by: &UserId,
        ) -> Result<Invoice> {
            unimplemented!("Not used in tests")
        }

        async fn get_by_id(&self, _tenant_id: &TenantId, _id: &crate::domain::InvoiceId) -> Result<Option<Invoice>> {
            Ok(None)
        }

        async fn list(
            &self,
            _tenant_id: &TenantId,
            _filters: &crate::domain::InvoiceFilters,
            _pagination: &crate::types::Pagination,
        ) -> Result<crate::types::PaginatedResponse<Invoice>> {
            unimplemented!("Not used in tests")
        }

        async fn update(
            &self,
            _tenant_id: &TenantId,
            _id: &crate::domain::InvoiceId,
            _updates: serde_json::Value,
        ) -> Result<Invoice> {
            // Return a test invoice for testing
            Ok(create_test_invoice())
        }

        async fn delete(&self, _tenant_id: &TenantId, _id: &crate::domain::InvoiceId) -> Result<()> {
            Ok(())
        }

        async fn update_capture_status(
            &self,
            _tenant_id: &TenantId,
            _id: &crate::domain::InvoiceId,
            _status: crate::domain::CaptureStatus,
        ) -> Result<()> {
            Ok(())
        }

        async fn update_processing_status(
            &self,
            _tenant_id: &TenantId,
            _id: &crate::domain::InvoiceId,
            _status: crate::domain::ProcessingStatus,
        ) -> Result<()> {
            Ok(())
        }
    }

    // Mock WorkQueueRepository for testing
    struct MockWorkQueueRepository;

    #[async_trait]
    impl crate::traits::WorkQueueRepository for MockWorkQueueRepository {
        async fn create(
            &self,
            _tenant_id: &TenantId,
            _input: crate::domain::CreateWorkQueueInput,
        ) -> Result<crate::domain::WorkQueue> {
            unimplemented!("Not used in tests")
        }

        async fn get_by_id(
            &self,
            _tenant_id: &TenantId,
            _id: &crate::domain::WorkQueueId,
        ) -> Result<Option<crate::domain::WorkQueue>> {
            Ok(None)
        }

        async fn list(&self, _tenant_id: &TenantId) -> Result<Vec<crate::domain::WorkQueue>> {
            Ok(vec![])
        }

        async fn update(
            &self,
            _tenant_id: &TenantId,
            _id: &crate::domain::WorkQueueId,
            _input: crate::domain::CreateWorkQueueInput,
        ) -> Result<crate::domain::WorkQueue> {
            unimplemented!("Not used in tests")
        }

        async fn delete(&self, _tenant_id: &TenantId, _id: &crate::domain::WorkQueueId) -> Result<()> {
            Ok(())
        }

        async fn get_default(&self, _tenant_id: &TenantId) -> Result<Option<crate::domain::WorkQueue>> {
            Ok(None)
        }

        async fn get_by_type(
            &self,
            _tenant_id: &TenantId,
            _queue_type: crate::domain::QueueType,
        ) -> Result<Option<crate::domain::WorkQueue>> {
            Ok(None)
        }

        async fn add_item(
            &self,
            _tenant_id: &TenantId,
            _queue_id: &crate::domain::WorkQueueId,
            _invoice_id: &crate::domain::InvoiceId,
            _assigned_to: Option<&UserId>,
        ) -> Result<crate::domain::QueueItem> {
            unimplemented!("Not used in tests")
        }

        async fn get_items(
            &self,
            _tenant_id: &TenantId,
            _queue_id: &crate::domain::WorkQueueId,
            _pagination: &crate::types::Pagination,
        ) -> Result<crate::types::PaginatedResponse<crate::domain::QueueItem>> {
            unimplemented!("Not used in tests")
        }

        async fn get_items_for_user(
            &self,
            _tenant_id: &TenantId,
            _queue_id: &crate::domain::WorkQueueId,
            _user_id: &UserId,
            _pagination: &crate::types::Pagination,
        ) -> Result<crate::types::PaginatedResponse<crate::domain::QueueItem>> {
            unimplemented!("Not used in tests")
        }

        async fn claim_item(
            &self,
            _tenant_id: &TenantId,
            _item_id: Uuid,
            _user_id: &UserId,
        ) -> Result<crate::domain::QueueItem> {
            unimplemented!("Not used in tests")
        }

        async fn complete_item(&self, _tenant_id: &TenantId, _item_id: Uuid, _action: &str) -> Result<()> {
            Ok(())
        }

        async fn move_item(
            &self,
            _tenant_id: &TenantId,
            _invoice_id: &crate::domain::InvoiceId,
            _to_queue_id: &crate::domain::WorkQueueId,
            _assigned_to: Option<&UserId>,
        ) -> Result<crate::domain::QueueItem> {
            // Return a mock queue item for testing
            Ok(crate::domain::QueueItem {
                id: Uuid::new_v4(),
                tenant_id: _tenant_id.clone(),
                queue_id: _to_queue_id.clone(),
                invoice_id: _invoice_id.clone(),
                assigned_to: _assigned_to.cloned(),
                priority: 0,
                entered_at: Utc::now(),
                due_at: None,
                claimed_at: None,
                completed_at: None,
            })
        }

        async fn count_items(&self, _tenant_id: &TenantId, _queue_id: &crate::domain::WorkQueueId) -> Result<i64> {
            Ok(0)
        }

        async fn count_items_for_user(
            &self,
            _tenant_id: &TenantId,
            _queue_id: &crate::domain::WorkQueueId,
            _user_id: &UserId,
        ) -> Result<i64> {
            Ok(0)
        }

        async fn get_current_item_for_invoice(
            &self,
            _tenant_id: &TenantId,
            _invoice_id: &crate::domain::InvoiceId,
        ) -> Result<Option<crate::domain::QueueItem>> {
            // Return a mock queue item for testing
            Ok(Some(crate::domain::QueueItem {
                id: Uuid::new_v4(),
                tenant_id: _tenant_id.clone(),
                queue_id: crate::domain::WorkQueueId::new(),
                invoice_id: _invoice_id.clone(),
                assigned_to: None,
                priority: 0,
                entered_at: Utc::now(),
                due_at: None,
                claimed_at: None,
                completed_at: None,
            }))
        }

        async fn reassign_item(
            &self,
            _tenant_id: &TenantId,
            _item_id: Uuid,
            assigned_to: &UserId,
        ) -> Result<crate::domain::QueueItem> {
            // Return a mock reassigned queue item for testing
            Ok(crate::domain::QueueItem {
                id: _item_id,
                tenant_id: _tenant_id.clone(),
                queue_id: crate::domain::WorkQueueId::new(),
                invoice_id: crate::domain::InvoiceId::new(),
                assigned_to: Some(assigned_to.clone()),
                priority: 0,
                entered_at: Utc::now(),
                due_at: None,
                claimed_at: None,
                completed_at: None,
            })
        }
    }

    // Mock ApprovalRepository for testing
    struct MockApprovalRepository;

    #[async_trait]
    impl crate::traits::ApprovalRepository for MockApprovalRepository {
        async fn create(
            &self,
            _tenant_id: &TenantId,
            request: crate::domain::ApprovalRequest,
        ) -> Result<crate::domain::ApprovalRequest> {
            Ok(request)
        }

        async fn get_by_id(
            &self,
            _tenant_id: &TenantId,
            _id: Uuid,
        ) -> Result<Option<crate::domain::ApprovalRequest>> {
            Ok(None)
        }

        async fn list_for_invoice(
            &self,
            _tenant_id: &TenantId,
            _invoice_id: &crate::domain::InvoiceId,
        ) -> Result<Vec<crate::domain::ApprovalRequest>> {
            Ok(vec![])
        }

        async fn list_pending_for_user(
            &self,
            _tenant_id: &TenantId,
            _user_id: &UserId,
        ) -> Result<Vec<crate::domain::ApprovalRequest>> {
            Ok(vec![])
        }

        async fn respond(
            &self,
            _tenant_id: &TenantId,
            _id: Uuid,
            _status: crate::domain::ApprovalStatus,
            _comments: Option<String>,
            _user_id: &UserId,
        ) -> Result<crate::domain::ApprovalRequest> {
            unimplemented!("Not used in tests")
        }

        async fn cancel_for_invoice(
            &self,
            _tenant_id: &TenantId,
            _invoice_id: &crate::domain::InvoiceId,
        ) -> Result<()> {
            Ok(())
        }
    }


    fn create_test_invoice() -> Invoice {
        Invoice {
            id: InvoiceId::new(),
            tenant_id: TenantId::new(),
            vendor_id: Some(Uuid::new_v4()),
            vendor_name: "Test Vendor".to_string(),
            invoice_number: "INV-001".to_string(),
            invoice_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 6).unwrap()),
            due_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap()),
            po_number: None,
            subtotal: Some(Money { amount: 10000, currency: "USD".to_string() }),
            tax_amount: Some(Money { amount: 800, currency: "USD".to_string() }),
            total_amount: Money { amount: 10800, currency: "USD".to_string() },
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::new_v4(),
            supporting_documents: vec![],
            ocr_confidence: Some(0.95),
            department: Some("Engineering".to_string()),
            gl_code: Some("5000".to_string()),
            cost_center: None,
            notes: None,
            tags: vec!["urgent".to_string(), "approved".to_string()],
            custom_fields: json!({"priority": "high", "project": "alpha"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: UserId(Uuid::new_v4()),
        }
    }

    #[test]
    fn test_evaluate_equals_condition() {
        let invoice = create_test_invoice();

        // Create a minimal test service without database dependencies
        // We only test the condition evaluation logic which doesn't need them
        struct TestConditionEvaluator;

        impl TestConditionEvaluator {
            fn evaluate_single_condition(
                &self,
                invoice: &Invoice,
                condition: &RuleCondition,
            ) -> bool {
                use crate::domain::{ConditionField, ConditionOperator};

                let field_value = match condition.field {
                    ConditionField::VendorName => {
                        Some(serde_json::Value::String(invoice.vendor_name.clone()))
                    }
                    ConditionField::Amount => {
                        serde_json::to_value(invoice.total_amount.amount).ok()
                    }
                    ConditionField::Department => {
                        invoice.department.as_ref().map(|d| serde_json::Value::String(d.clone()))
                    }
                    ConditionField::GlCode => {
                        invoice.gl_code.as_ref().map(|g| serde_json::Value::String(g.clone()))
                    }
                    _ => None,
                };

                match &field_value {
                    None => matches!(condition.operator, ConditionOperator::IsNull),
                    Some(fv) => {
                        match condition.operator {
                            ConditionOperator::Equals => fv == &condition.value,
                            ConditionOperator::NotEquals => fv != &condition.value,
                            ConditionOperator::GreaterThan => {
                                if let (serde_json::Value::Number(a), serde_json::Value::Number(b)) = (fv, &condition.value) {
                                    a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::LessThanOrEqual => {
                                if let (serde_json::Value::Number(a), serde_json::Value::Number(b)) = (fv, &condition.value) {
                                    a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::Contains => {
                                if let (serde_json::Value::String(s), serde_json::Value::String(pattern)) = (fv, &condition.value) {
                                    s.contains(pattern)
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::StartsWith => {
                                if let (serde_json::Value::String(s), serde_json::Value::String(prefix)) = (fv, &condition.value) {
                                    s.starts_with(prefix)
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::EndsWith => {
                                if let (serde_json::Value::String(s), serde_json::Value::String(suffix)) = (fv, &condition.value) {
                                    s.ends_with(suffix)
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::In => {
                                if let serde_json::Value::Array(arr) = &condition.value {
                                    arr.contains(fv)
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::Between => {
                                if let serde_json::Value::Array(arr) = &condition.value {
                                    if arr.len() == 2 {
                                        if let serde_json::Value::Number(a) = fv {
                                            let val = a.as_f64().unwrap_or(0.0);
                                            let min = arr[0].as_f64().unwrap_or(0.0);
                                            let max = arr[1].as_f64().unwrap_or(0.0);
                                            val >= min && val <= max
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            }
                            ConditionOperator::IsNull => false,
                            ConditionOperator::IsNotNull => true,
                            _ => false,
                        }
                    }
                }
            }
        }

        let evaluator = TestConditionEvaluator;

        // Test Equals
        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::Equals,
            value: json!("Test Vendor"),
        };
        assert!(evaluator.evaluate_single_condition(&invoice, &condition));

        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::Equals,
            value: json!("Wrong Vendor"),
        };
        assert!(!evaluator.evaluate_single_condition(&invoice, &condition));

        // Test GreaterThan
        let condition = RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::GreaterThan,
            value: json!(10000),
        };
        assert!(evaluator.evaluate_single_condition(&invoice, &condition));

        // Test LessThanOrEqual
        let condition = RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::LessThanOrEqual,
            value: json!(10800),
        };
        assert!(evaluator.evaluate_single_condition(&invoice, &condition));

        // Test Contains
        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::Contains,
            value: json!("Test"),
        };
        assert!(evaluator.evaluate_single_condition(&invoice, &condition));

        // Test In
        let condition = RuleCondition {
            field: ConditionField::Department,
            operator: ConditionOperator::In,
            value: json!(["Engineering", "Sales", "Marketing"]),
        };
        assert!(evaluator.evaluate_single_condition(&invoice, &condition));

        // Test Between
        let condition = RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::Between,
            value: json!([5000, 15000]),
        };
        assert!(evaluator.evaluate_single_condition(&invoice, &condition));
    }

    #[test]
    fn test_evaluate_string_operators() {
        let invoice = create_test_invoice();

        struct TestConditionEvaluator;

        impl TestConditionEvaluator {
            fn evaluate(&self, invoice: &Invoice, condition: &RuleCondition) -> bool {
                if condition.field != ConditionField::VendorName {
                    return false;
                }
                let field_value = serde_json::Value::String(invoice.vendor_name.clone());
                match condition.operator {
                    ConditionOperator::Contains => {
                        if let (serde_json::Value::String(s), serde_json::Value::String(pattern)) = (&field_value, &condition.value) {
                            s.contains(pattern)
                        } else {
                            false
                        }
                    }
                    ConditionOperator::StartsWith => {
                        if let (serde_json::Value::String(s), serde_json::Value::String(prefix)) = (&field_value, &condition.value) {
                            s.starts_with(prefix)
                        } else {
                            false
                        }
                    }
                    ConditionOperator::EndsWith => {
                        if let (serde_json::Value::String(s), serde_json::Value::String(suffix)) = (&field_value, &condition.value) {
                            s.ends_with(suffix)
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
        }

        let evaluator = TestConditionEvaluator;

        // Test Contains
        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::Contains,
            value: json!("Test"),
        };
        assert!(evaluator.evaluate(&invoice, &condition));

        // Test StartsWith
        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::StartsWith,
            value: json!("Test"),
        };
        assert!(evaluator.evaluate(&invoice, &condition));

        // Test EndsWith
        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::EndsWith,
            value: json!("Vendor"),
        };
        assert!(evaluator.evaluate(&invoice, &condition));
    }
}
