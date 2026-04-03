//! AI Agent Tools - Invoice query capabilities

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::models::AgentContext;

/// Tool trait for agent capabilities
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String>;
}

/// Invoice status query tool
pub struct InvoiceStatusTool {
    pool: PgPool,
}

impl InvoiceStatusTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for InvoiceStatusTool {
    fn name(&self) -> &str {
        "get_invoice_status"
    }

    fn description(&self) -> &str {
        "Get the status of an invoice by ID. Args: invoice_id (UUID)"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id: Uuid = args
            .trim()
            .parse()
            .context("Invalid invoice ID format. Please provide a valid UUID.")?;

        let row = sqlx::query(
            r#"
            SELECT
                i.id,
                i.invoice_number,
                i.status,
                i.total_amount,
                i.currency,
                v.name as vendor_name,
                i.created_at,
                i.approved_at
            FROM invoices i
            JOIN vendors v ON i.vendor_id = v.id
            WHERE i.id = $1 AND i.tenant_id = $2
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let invoice_number: String = row.try_get("invoice_number")?;
                let id: Uuid = row.try_get("id")?;
                let status: String = row.try_get("status")?;
                let total_amount: f64 = row.try_get("total_amount")?;
                let currency: String = row.try_get("currency")?;
                let vendor_name: String = row.try_get("vendor_name")?;
                let created_at: DateTime<Utc> = row.try_get("created_at")?;
                let approved_at: Option<DateTime<Utc>> = row.try_get("approved_at")?;

                Ok(format!(
                    "Invoice {} (ID: {})\nStatus: {}\nVendor: {}\nAmount: {} {}\nCreated: {}\nApproved: {}",
                    invoice_number,
                    id,
                    status,
                    vendor_name,
                    total_amount,
                    currency,
                    created_at.format("%Y-%m-%d %H:%M"),
                    approved_at.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "Not approved".to_string()),
                ))
            }
            None => Ok(format!(
                "Invoice {} not found in your organization.",
                invoice_id
            )),
        }
    }
}

/// Vendor invoices query tool
pub struct VendorInvoicesTool {
    pool: PgPool,
}

impl VendorInvoicesTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for VendorInvoicesTool {
    fn name(&self) -> &str {
        "get_vendor_invoices"
    }

    fn description(&self) -> &str {
        "Get all invoices from a vendor. Args: vendor_name"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let vendor_name = args.trim();

        let rows = sqlx::query(
            r#"
            SELECT
                i.id,
                i.invoice_number,
                i.status,
                i.total_amount,
                i.currency,
                i.created_at
            FROM invoices i
            JOIN vendors v ON i.vendor_id = v.id
            WHERE v.name ILIKE $1 AND i.tenant_id = $2
            ORDER BY i.created_at DESC
            LIMIT 20
            "#,
        )
        .bind(format!("%{}%", vendor_name))
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(format!(
                "No invoices found for vendor matching '{}'.",
                vendor_name
            ));
        }

        let mut result = format!("Invoices from vendor matching '{}':\n\n", vendor_name);
        for row in rows {
            let invoice_number: String = row.try_get("invoice_number")?;
            let status: String = row.try_get("status")?;
            let created_at: DateTime<Utc> = row.try_get("created_at")?;
            let total_amount: f64 = row.try_get("total_amount")?;
            let currency: String = row.try_get("currency")?;

            result.push_str(&format!(
                "- {} | {} | {} | {} {}\n",
                invoice_number,
                status,
                created_at.format("%Y-%m-%d"),
                total_amount,
                currency,
            ));
        }

        Ok(result)
    }
}

/// Approval requirements query tool
pub struct ApprovalRequirementsTool {
    pool: PgPool,
}

impl ApprovalRequirementsTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for ApprovalRequirementsTool {
    fn name(&self) -> &str {
        "get_approval_requirements"
    }

    fn description(&self) -> &str {
        "Check who needs to approve an invoice. Args: invoice_id (UUID)"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id: Uuid = args
            .trim()
            .parse()
            .context("Invalid invoice ID format. Please provide a valid UUID.")?;

        // Get invoice amount
        let invoice = sqlx::query(
            r#"
            SELECT total_amount, currency
            FROM invoices
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        let invoice = match invoice {
            Some(inv) => inv,
            None => return Ok(format!("Invoice {} not found.", invoice_id)),
        };

        let total_amount: f64 = invoice.try_get("total_amount")?;
        let currency: String = invoice.try_get("currency")?;

        // Get approval workflow steps
        let steps = sqlx::query(
            r#"
            SELECT
                aws.step_order,
                aws.approver_role,
                u.email as approver_email,
                a.status as approval_status,
                a.approved_at
            FROM approval_workflow_steps aws
            LEFT JOIN approvals a ON a.invoice_id = $1 AND a.workflow_step_id = aws.id
            LEFT JOIN users u ON a.approver_id = u.id
            WHERE aws.tenant_id = $2
            ORDER BY aws.step_order
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if steps.is_empty() {
            return Ok(format!(
                "Invoice {} ({} {}): No approval workflow configured for this organization.",
                invoice_id, total_amount, currency
            ));
        }

        let mut result = format!(
            "Approval requirements for invoice {} ({} {}):\n\n",
            invoice_id, total_amount, currency
        );

        for step in steps {
            let step_order: i32 = step.try_get("step_order")?;
            let approver_role: String = step.try_get("approver_role")?;
            let approver_email: Option<String> = step.try_get("approver_email")?;
            let approval_status: Option<String> = step.try_get("approval_status")?;
            let approved_at: Option<DateTime<Utc>> = step.try_get("approved_at")?;

            let status = match approval_status {
                Some(s) => s,
                None => "Pending".to_string(),
            };

            result.push_str(&format!(
                "{}. {} - {} ({}): {}\n",
                step_order,
                approver_role,
                approver_email.unwrap_or_else(|| "Not assigned".to_string()),
                if status == "approved" {
                    approved_at
                        .map(|t| t.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "N/A".to_string())
                } else {
                    "Pending".to_string()
                },
                status,
            ));
        }

        Ok(result)
    }
}

/// Invoice summary tool
pub struct InvoiceSummaryTool {
    pool: PgPool,
}

impl InvoiceSummaryTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for InvoiceSummaryTool {
    fn name(&self) -> &str {
        "summarize_invoice"
    }

    fn description(&self) -> &str {
        "Generate a summary of an invoice. Args: invoice_id (UUID)"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id: Uuid = args
            .trim()
            .parse()
            .context("Invalid invoice ID format. Please provide a valid UUID.")?;

        let invoice = sqlx::query(
            r#"
            SELECT
                i.id,
                i.invoice_number,
                i.status,
                i.total_amount,
                i.currency,
                v.name as vendor_name,
                i.description,
                i.created_at,
                i.approved_at
            FROM invoices i
            JOIN vendors v ON i.vendor_id = v.id
            WHERE i.id = $1 AND i.tenant_id = $2
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        match invoice {
            Some(inv) => {
                let invoice_number: String = inv.try_get("invoice_number")?;
                let vendor_name: String = inv.try_get("vendor_name")?;
                let total_amount: f64 = inv.try_get("total_amount")?;
                let currency: String = inv.try_get("currency")?;
                let status: String = inv.try_get("status")?;
                let description: Option<String> = inv.try_get("description")?;
                let created_at: DateTime<Utc> = inv.try_get("created_at")?;
                let approved_at: Option<DateTime<Utc>> = inv.try_get("approved_at")?;

                Ok(format!(
                    "**Invoice Summary**\n\n\
                    **Invoice Number:** {}\n\
                    **Vendor:** {}\n\
                    **Amount:** {} {}\n\
                    **Status:** {}\n\
                    **Description:** {}\n\
                    **Created:** {}\n\
                    **Approved:** {}",
                    invoice_number,
                    vendor_name,
                    total_amount,
                    currency,
                    status,
                    description.unwrap_or_else(|| "No description".to_string()),
                    created_at.format("%Y-%m-%d"),
                    approved_at
                        .map(|t| t.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "Not approved".to_string()),
                ))
            }
            None => Ok(format!("Invoice {} not found.", invoice_id)),
        }
    }
}

/// Collection of available tools
#[derive(Clone)]
pub struct ToolRegistry {
    pool: PgPool,
}

impl ToolRegistry {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn get_tool_descriptions(&self) -> String {
        let descriptions = vec![
            (
                "get_invoice_status",
                "Get status of an invoice by ID. Args: invoice_id (UUID)",
            ),
            (
                "get_vendor_invoices",
                "Find all invoices from a vendor. Args: vendor_name",
            ),
            (
                "get_approval_requirements",
                "Check who needs to approve an invoice. Args: invoice_id (UUID)",
            ),
            (
                "summarize_invoice",
                "Generate a summary of an invoice. Args: invoice_id (UUID)",
            ),
        ];

        descriptions
            .iter()
            .map(|(name, desc)| format!("- {}: {}", name, desc))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn execute_tool(
        &self,
        tool_name: &str,
        context: &AgentContext,
        args: &str,
    ) -> Result<String> {
        match tool_name {
            "get_invoice_status" => {
                InvoiceStatusTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_vendor_invoices" => {
                VendorInvoicesTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_approval_requirements" => {
                ApprovalRequirementsTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "summarize_invoice" => {
                InvoiceSummaryTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            _ => anyhow::bail!("Tool '{}' not found", tool_name),
        }
    }
}
