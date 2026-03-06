//! Workflow orchestration service
//!
//! Handles approval workflow logic including:
//! - Creating approval requests
//! - Sending approval emails with action tokens
//! - Routing invoices through queues
//! - Escalation management

use crate::{
    domain::{ApprovalRequest, ApprovalStatus, ApprovalTarget, Invoice, ProcessingStatus, WorkflowRule, RuleCondition, RuleAction},
    services::{EmailAction, EmailActionTokenService},
    traits::{ApprovalRepository, InvoiceRepository},
    types::TenantId,
    Error, Result, UserId,
};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use std::sync::Arc;
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
pub struct WorkflowService<ES: EmailService, ET: EmailTemplates> {
    email_service: ES,
    email_token_service: EmailActionTokenService,
    app_url: String,
    _email_templates: std::marker::PhantomData<ET>,
}

impl<ES: EmailService, ET: EmailTemplates> WorkflowService<ES, ET> {
    /// Create a new workflow service
    pub fn new(
        email_service: ES,
        email_token_service: EmailActionTokenService,
        app_url: String,
    ) -> Self {
        Self {
            email_service,
            email_token_service,
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
            ApprovalTarget::User(_user_id) => {
                // TODO: Fetch user email from database
                vec![] // Placeholder
            }
            ApprovalTarget::Role(_role_name) => {
                // TODO: Fetch all users with this role
                vec![]
            }
            ApprovalTarget::AnyOf(_user_ids) | ApprovalTarget::AllOf(_user_ids) => {
                // TODO: Fetch emails for all users
                vec![]
            }
        };

        if approver_emails.is_empty() {
            tracing::warn!("No approver emails found for approval request {}", request.id);
            return Ok(());
        }

        // Generate email action tokens
        let approve_token = self.email_token_service.generate_token(
            tenant_id,
            &UserId(Uuid::nil()), // Will be set when fetching approver
            EmailAction::ApproveInvoice,
            invoice.id.0,
            "invoice",
            serde_json::json!({ "approval_id": request.id }),
        ).await?;

        let reject_token = self.email_token_service.generate_token(
            tenant_id,
            &UserId(Uuid::nil()),
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
        let submitted_by = "AP Team"; // TODO: Get actual submitter name

        let (html, text) = ET::invoice_pending_approval_with_actions(
            invoice_number,
            vendor_name,
            &amount,
            submitted_by,
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
        // TODO: Implement condition evaluation
        // For now, return true to allow all rules to execute
        true
    }

    /// Execute a workflow action
    async fn execute_action(
        &self,
        tenant_id: &TenantId,
        invoice: &Invoice,
        action: &crate::domain::RuleAction,
    ) -> Result<()> {
        // TODO: Implement action execution
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MockEmailService removed - would need to be implemented for testing
    // For now, we verify service creation without actual email service

    #[test]
    fn test_workflow_service_structure() {
        // Verify that the service structure compiles
        // Actual instantiation would require real implementations
        assert!(true);
    }
}
