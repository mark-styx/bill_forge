//! Email templates for BillForge notifications

use crate::EmailService;
use billforge_core::Result;

/// Email template generator
pub struct EmailTemplates;

impl EmailTemplates {
    /// Base HTML template wrapper
    fn wrap_html(content: &str, title: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 600px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        .container {{
            background-color: #ffffff;
            border-radius: 8px;
            padding: 30px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .header {{
            text-align: center;
            margin-bottom: 30px;
        }}
        .logo {{
            font-size: 24px;
            font-weight: bold;
            color: #2563eb;
        }}
        h1 {{
            color: #1f2937;
            font-size: 20px;
            margin-bottom: 20px;
        }}
        .button {{
            display: inline-block;
            background-color: #2563eb;
            color: #ffffff;
            padding: 12px 24px;
            text-decoration: none;
            border-radius: 6px;
            font-weight: 500;
            margin: 20px 0;
        }}
        .button:hover {{
            background-color: #1d4ed8;
        }}
        .footer {{
            margin-top: 30px;
            padding-top: 20px;
            border-top: 1px solid #e5e7eb;
            text-align: center;
            font-size: 12px;
            color: #6b7280;
        }}
        .info-box {{
            background-color: #f3f4f6;
            border-radius: 6px;
            padding: 15px;
            margin: 15px 0;
        }}
        .info-row {{
            display: flex;
            justify-content: space-between;
            padding: 5px 0;
        }}
        .info-label {{
            color: #6b7280;
        }}
        .info-value {{
            font-weight: 500;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div class="logo">BillForge</div>
        </div>
        {content}
        <div class="footer">
            <p>This email was sent by BillForge.</p>
            <p>If you did not expect this email, please ignore it.</p>
        </div>
    </div>
</body>
</html>"#,
            title = title,
            content = content
        )
    }

    /// Invoice pending approval email
    pub fn invoice_pending_approval(
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        submitted_by: &str,
        approval_url: &str,
    ) -> (String, String) {
        let html = Self::wrap_html(
            &format!(
                r#"<h1>Invoice Pending Your Approval</h1>
<p>A new invoice requires your approval:</p>
<div class="info-box">
    <div class="info-row">
        <span class="info-label">Invoice Number:</span>
        <span class="info-value">{invoice_number}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Vendor:</span>
        <span class="info-value">{vendor_name}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Amount:</span>
        <span class="info-value">{amount}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Submitted By:</span>
        <span class="info-value">{submitted_by}</span>
    </div>
</div>
<p style="text-align: center;">
    <a href="{approval_url}" class="button">Review Invoice</a>
</p>
<p>You can also review this invoice by logging into BillForge.</p>"#
            ),
            "Invoice Pending Approval",
        );

        let text = format!(
            r#"Invoice Pending Your Approval

A new invoice requires your approval:

Invoice Number: {invoice_number}
Vendor: {vendor_name}
Amount: {amount}
Submitted By: {submitted_by}

Review the invoice at: {approval_url}

---
This email was sent by BillForge."#
        );

        (html, text)
    }

    /// Invoice approved notification
    pub fn invoice_approved(
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        approved_by: &str,
    ) -> (String, String) {
        let html = Self::wrap_html(
            &format!(
                r#"<h1>Invoice Approved</h1>
<p>An invoice you submitted has been approved:</p>
<div class="info-box">
    <div class="info-row">
        <span class="info-label">Invoice Number:</span>
        <span class="info-value">{invoice_number}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Vendor:</span>
        <span class="info-value">{vendor_name}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Amount:</span>
        <span class="info-value">{amount}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Approved By:</span>
        <span class="info-value">{approved_by}</span>
    </div>
</div>
<p>The invoice is now ready for payment processing.</p>"#
            ),
            "Invoice Approved",
        );

        let text = format!(
            r#"Invoice Approved

An invoice you submitted has been approved:

Invoice Number: {invoice_number}
Vendor: {vendor_name}
Amount: {amount}
Approved By: {approved_by}

The invoice is now ready for payment processing.

---
This email was sent by BillForge."#
        );

        (html, text)
    }

    /// Invoice rejected notification
    pub fn invoice_rejected(
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        rejected_by: &str,
        reason: &str,
    ) -> (String, String) {
        let html = Self::wrap_html(
            &format!(
                r#"<h1>Invoice Rejected</h1>
<p>An invoice you submitted has been rejected:</p>
<div class="info-box">
    <div class="info-row">
        <span class="info-label">Invoice Number:</span>
        <span class="info-value">{invoice_number}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Vendor:</span>
        <span class="info-value">{vendor_name}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Amount:</span>
        <span class="info-value">{amount}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Rejected By:</span>
        <span class="info-value">{rejected_by}</span>
    </div>
</div>
<p><strong>Reason:</strong> {reason}</p>
<p>Please review and resubmit the invoice if applicable.</p>"#
            ),
            "Invoice Rejected",
        );

        let text = format!(
            r#"Invoice Rejected

An invoice you submitted has been rejected:

Invoice Number: {invoice_number}
Vendor: {vendor_name}
Amount: {amount}
Rejected By: {rejected_by}

Reason: {reason}

Please review and resubmit the invoice if applicable.

---
This email was sent by BillForge."#
        );

        (html, text)
    }

    /// Welcome email for new users
    pub fn welcome_user(
        user_name: &str,
        tenant_name: &str,
        login_url: &str,
    ) -> (String, String) {
        let html = Self::wrap_html(
            &format!(
                r#"<h1>Welcome to BillForge!</h1>
<p>Hi {user_name},</p>
<p>Your account has been created for <strong>{tenant_name}</strong>.</p>
<p>You can now log in to start managing invoices and streamlining your accounts payable workflow.</p>
<p style="text-align: center;">
    <a href="{login_url}" class="button">Log In to BillForge</a>
</p>
<p>If you have any questions, please contact your administrator.</p>"#
            ),
            "Welcome to BillForge",
        );

        let text = format!(
            r#"Welcome to BillForge!

Hi {user_name},

Your account has been created for {tenant_name}.

You can now log in to start managing invoices:
{login_url}

If you have any questions, please contact your administrator.

---
This email was sent by BillForge."#
        );

        (html, text)
    }

    /// Password reset email
    pub fn password_reset(
        user_name: &str,
        reset_url: &str,
        expires_in: &str,
    ) -> (String, String) {
        let html = Self::wrap_html(
            &format!(
                r#"<h1>Password Reset Request</h1>
<p>Hi {user_name},</p>
<p>We received a request to reset your password. Click the button below to create a new password:</p>
<p style="text-align: center;">
    <a href="{reset_url}" class="button">Reset Password</a>
</p>
<p>This link will expire in {expires_in}.</p>
<p>If you didn't request this password reset, you can safely ignore this email. Your password will remain unchanged.</p>"#
            ),
            "Password Reset",
        );

        let text = format!(
            r#"Password Reset Request

Hi {user_name},

We received a request to reset your password.

Reset your password at: {reset_url}

This link will expire in {expires_in}.

If you didn't request this password reset, you can safely ignore this email.

---
This email was sent by BillForge."#
        );

        (html, text)
    }

    /// Payment reminder for overdue invoices
    pub fn payment_reminder(
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        due_date: &str,
        days_overdue: i32,
        invoice_url: &str,
    ) -> (String, String) {
        let urgency = if days_overdue > 30 {
            "Urgent: "
        } else if days_overdue > 14 {
            "Important: "
        } else {
            ""
        };

        let html = Self::wrap_html(
            &format!(
                r#"<h1>{urgency}Invoice Payment Reminder</h1>
<p>The following invoice is {days_overdue} days past due:</p>
<div class="info-box">
    <div class="info-row">
        <span class="info-label">Invoice Number:</span>
        <span class="info-value">{invoice_number}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Vendor:</span>
        <span class="info-value">{vendor_name}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Amount:</span>
        <span class="info-value">{amount}</span>
    </div>
    <div class="info-row">
        <span class="info-label">Due Date:</span>
        <span class="info-value">{due_date}</span>
    </div>
</div>
<p style="text-align: center;">
    <a href="{invoice_url}" class="button">View Invoice</a>
</p>
<p>Please process this payment at your earliest convenience.</p>"#
            ),
            &format!("{}Invoice Payment Reminder", urgency),
        );

        let text = format!(
            r#"{urgency}Invoice Payment Reminder

The following invoice is {days_overdue} days past due:

Invoice Number: {invoice_number}
Vendor: {vendor_name}
Amount: {amount}
Due Date: {due_date}

View invoice at: {invoice_url}

Please process this payment at your earliest convenience.

---
This email was sent by BillForge."#
        );

        (html, text)
    }
}

/// Helper to send templated emails
pub struct EmailNotifier<E: EmailService> {
    service: E,
    app_url: String,
}

impl<E: EmailService> EmailNotifier<E> {
    pub fn new(service: E, app_url: String) -> Self {
        Self { service, app_url }
    }

    pub async fn send_invoice_pending_approval(
        &self,
        to: &str,
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        submitted_by: &str,
        invoice_id: &str,
    ) -> Result<()> {
        let approval_url = format!("{}/invoices/{}", self.app_url, invoice_id);
        let (html, text) = EmailTemplates::invoice_pending_approval(
            invoice_number,
            vendor_name,
            amount,
            submitted_by,
            &approval_url,
        );

        self.service
            .send(
                to,
                &format!("Approval Required: Invoice {} from {}", invoice_number, vendor_name),
                &html,
                &text,
            )
            .await
    }

    pub async fn send_invoice_approved(
        &self,
        to: &str,
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        approved_by: &str,
    ) -> Result<()> {
        let (html, text) = EmailTemplates::invoice_approved(
            invoice_number,
            vendor_name,
            amount,
            approved_by,
        );

        self.service
            .send(
                to,
                &format!("Invoice {} Approved", invoice_number),
                &html,
                &text,
            )
            .await
    }

    pub async fn send_invoice_rejected(
        &self,
        to: &str,
        invoice_number: &str,
        vendor_name: &str,
        amount: &str,
        rejected_by: &str,
        reason: &str,
    ) -> Result<()> {
        let (html, text) = EmailTemplates::invoice_rejected(
            invoice_number,
            vendor_name,
            amount,
            rejected_by,
            reason,
        );

        self.service
            .send(
                to,
                &format!("Invoice {} Rejected", invoice_number),
                &html,
                &text,
            )
            .await
    }

    pub async fn send_welcome_email(
        &self,
        to: &str,
        user_name: &str,
        tenant_name: &str,
    ) -> Result<()> {
        let login_url = format!("{}/login", self.app_url);
        let (html, text) = EmailTemplates::welcome_user(user_name, tenant_name, &login_url);

        self.service
            .send(to, "Welcome to BillForge", &html, &text)
            .await
    }
}
