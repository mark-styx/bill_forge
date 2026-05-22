//! AI Agent Tools - Invoice query capabilities

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::models::AgentContext;
use super::product_knowledge::product_knowledge_context_for_query_with_limit;

/// All known BillForge modules, ordered for stable output.
const ALL_MODULES: &[billforge_core::Module] = &[
    billforge_core::Module::InvoiceCapture,
    billforge_core::Module::InvoiceProcessing,
    billforge_core::Module::VendorManagement,
    billforge_core::Module::Reporting,
    billforge_core::Module::AiAssistant,
];

/// What Winston may explain or assist with when a module is enabled,
/// and the boundary language when it is disabled.
struct ModuleInfo {
    key: &'static str,
    display_name: &'static str,
    enabled_help: &'static str,
    disabled_boundary: &'static str,
}

fn module_info(m: billforge_core::Module) -> ModuleInfo {
    match m {
        billforge_core::Module::InvoiceCapture => ModuleInfo {
            key: "invoice_capture",
            display_name: "Invoice Capture",
            enabled_help: "Winston can look up invoice details, explain capture status, and answer questions about uploaded invoices.",
            disabled_boundary: "Invoice Capture is not available for this organization. Winston cannot access or look up invoice data. Contact your administrator to enable this module.",
        },
        billforge_core::Module::InvoiceProcessing => ModuleInfo {
            key: "invoice_processing",
            display_name: "Invoice Processing",
            enabled_help: "Winston can explain processing workflows, approval requirements, and invoice processing status.",
            disabled_boundary: "Invoice Processing is not available for this organization. Winston cannot explain processing workflows or approval steps. Contact your administrator to enable this module.",
        },
        billforge_core::Module::VendorManagement => ModuleInfo {
            key: "vendor_management",
            display_name: "Vendor Management",
            enabled_help: "Winston can look up vendor invoices and answer questions about vendor relationships.",
            disabled_boundary: "Vendor Management is not available for this organization. Winston cannot access vendor data. Contact your administrator to enable this module.",
        },
        billforge_core::Module::Reporting => ModuleInfo {
            key: "reporting",
            display_name: "Reporting & Analytics",
            enabled_help: "Winston can explain available reports and help interpret reporting data.",
            disabled_boundary: "Reporting & Analytics is not available for this organization. Winston cannot generate or explain reports. Contact your administrator to enable this module.",
        },
        billforge_core::Module::AiAssistant => ModuleInfo {
            key: "ai_assistant",
            display_name: "Winston AI Assistant",
            enabled_help: "Winston AI Assistant is active. Winston can answer questions, use tools, and assist with enabled modules.",
            disabled_boundary: "Winston AI Assistant is a paid add-on that is not enabled for this organization. The AI assistant cannot be used until it is added to the tenant plan.",
        },
    }
}

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
        let invoice_id: Uuid = args.trim().parse()
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
            None => Ok(format!("Invoice {} not found in your organization.", invoice_id)),
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
            return Ok(format!("No invoices found for vendor matching '{}'.", vendor_name));
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
        let invoice_id: Uuid = args.trim().parse()
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
                    approved_at.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "N/A".to_string())
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
        let invoice_id: Uuid = args.trim().parse()
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
                    approved_at.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "Not approved".to_string()),
                ))
            }
            None => Ok(format!("Invoice {} not found.", invoice_id)),
        }
    }
}

/// Read-only tool that reports tenant module availability and capability boundaries.
/// No database queries, no mutations, no external calls.
pub struct ModuleCapabilitiesTool;

#[async_trait]
impl Tool for ModuleCapabilitiesTool {
    fn name(&self) -> &str {
        "get_module_capabilities"
    }

    fn description(&self) -> &str {
        "Report which modules are enabled for the tenant and describe capability boundaries. No args required."
    }

    async fn execute(&self, context: &AgentContext, _args: &str) -> Result<String> {
        let mut lines = Vec::new();
        lines.push("Module Capabilities Report\n".to_string());

        for &module in ALL_MODULES {
            let info = module_info(module);
            let enabled = context.enabled_modules.contains(&module);
            let status = if enabled { "ENABLED" } else { "DISABLED" };
            lines.push(format!(
                "- {} ({}): {}\n  {}",
                info.display_name,
                info.key,
                status,
                if enabled {
                    info.enabled_help
                } else {
                    info.disabled_boundary
                },
            ));
        }

        Ok(lines.join("\n"))
    }
}

/// Read-only tool that searches the product documentation index for relevant
/// snippets. No database queries, no mutations, no external calls.
pub struct SearchProductDocsTool;

#[async_trait]
impl Tool for SearchProductDocsTool {
    fn name(&self) -> &str {
        "search_product_docs"
    }

    fn description(&self) -> &str {
        "Search BillForge product documentation for relevant snippets. Args: query (plain text)"
    }

    async fn execute(&self, _context: &AgentContext, args: &str) -> Result<String> {
        let query = args.trim();
        if query.is_empty() {
            return Ok("Please provide a search query to look up product documentation.".to_string());
        }

        let snippets = product_knowledge_context_for_query_with_limit(query, 5);

        if snippets.is_empty() {
            return Ok(format!("No product documentation found for query: '{}'. Try different keywords.", query));
        }

        let mut lines = Vec::new();
        lines.push(format!("Product documentation results for '{}':\n", query));

        for (i, snippet) in snippets.iter().enumerate() {
            lines.push(format!(
                "{}. [{}] {}\n   {}",
                i + 1,
                snippet.source_path,
                snippet.heading,
                snippet.excerpt,
            ));
        }

        Ok(lines.join("\n\n"))
    }
}

/// Read-only tool that explains a product feature using the documentation
/// index. Returns a concise explanation grounded in matched snippets with
/// source references. No database queries, no mutations, no external calls.
pub struct ExplainFeatureTool;

#[async_trait]
impl Tool for ExplainFeatureTool {
    fn name(&self) -> &str {
        "explain_feature"
    }

    fn description(&self) -> &str {
        "Explain a BillForge feature or concept using product documentation. Args: feature (name or question)"
    }

    async fn execute(&self, _context: &AgentContext, args: &str) -> Result<String> {
        let feature = args.trim();
        if feature.is_empty() {
            return Ok("Please provide a feature name or question to get an explanation.".to_string());
        }

        let snippets = product_knowledge_context_for_query_with_limit(feature, 5);

        if snippets.is_empty() {
            return Ok(format!(
                "No product documentation found for '{}'. The feature may not be documented yet or may use different terminology.",
                feature
            ));
        }

        let mut lines = Vec::new();
        lines.push(format!("Explanation for '{}':\n", feature));

        for snippet in &snippets {
            lines.push(format!(
                "- {} (from {}): {}",
                snippet.heading, snippet.source_path, snippet.excerpt,
            ));
        }

        lines.push(String::new());
        lines.push("Note: This explanation is based solely on indexed product documentation.".to_string());

        Ok(lines.join("\n\n"))
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
            ("get_invoice_status", "Get status of an invoice by ID. Args: invoice_id (UUID)"),
            ("get_vendor_invoices", "Find all invoices from a vendor. Args: vendor_name"),
            ("get_approval_requirements", "Check who needs to approve an invoice. Args: invoice_id (UUID)"),
            ("summarize_invoice", "Generate a summary of an invoice. Args: invoice_id (UUID)"),
            ("get_module_capabilities", "Report which modules are enabled for the tenant and describe capability boundaries. No args required."),
            ("search_product_docs", "Search BillForge product documentation for relevant snippets. Args: query (plain text)"),
            ("explain_feature", "Explain a BillForge feature or concept using product documentation. Args: feature (name or question)"),
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
                InvoiceStatusTool::new(self.pool.clone()).execute(context, args).await
            }
            "get_vendor_invoices" => {
                VendorInvoicesTool::new(self.pool.clone()).execute(context, args).await
            }
            "get_approval_requirements" => {
                ApprovalRequirementsTool::new(self.pool.clone()).execute(context, args).await
            }
            "summarize_invoice" => {
                InvoiceSummaryTool::new(self.pool.clone()).execute(context, args).await
            }
            "get_module_capabilities" => {
                ModuleCapabilitiesTool.execute(context, args).await
            }
            "search_product_docs" => {
                SearchProductDocsTool.execute(context, args).await
            }
            "explain_feature" => {
                ExplainFeatureTool.execute(context, args).await
            }
            _ => anyhow::bail!("Tool '{}' not found", tool_name),
        }
    }
}
