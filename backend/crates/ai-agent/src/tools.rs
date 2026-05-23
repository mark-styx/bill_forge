//! AI Agent Tools - Invoice query capabilities

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::models::AgentContext;
use super::product_knowledge::{
    product_knowledge_context_for_query_with_limit,
    search_known_issues_context_for_query_with_limit,
    release_changes_context_for_query_with_limit,
};

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

// ── Typed tool registry metadata ──────────────────────────────────────────────

/// Functional class of an AI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiToolClass {
    Invoice,
    Vendor,
    Approval,
    TenantCapability,
    ProductKnowledge,
    Workflow,
    IssueIntake,
}

/// Permission required to invoke an AI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiToolPermission {
    InvoiceRead,
    VendorRead,
    ApprovalRead,
    TenantModuleRead,
    ProductKnowledgeRead,
    WorkflowRead,
    IssueRequest,
}

/// Risk level of invoking an AI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiToolRiskLevel {
    Low,
    Medium,
    High,
}

/// Typed metadata for a single AI tool registered in the [`ToolRegistry`].
#[derive(Debug, Clone)]
pub struct AiToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub class: AiToolClass,
    pub required_permission: AiToolPermission,
    pub risk_level: AiToolRiskLevel,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub mutates: bool,
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
                id, invoice_number, status, total_amount_cents, currency,
                vendor_name, capture_status, processing_status,
                invoice_date, due_date, po_number,
                current_queue_id, assigned_to,
                created_at, updated_at
            FROM invoices
            WHERE id = $1 AND tenant_id = $2
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
                let total_cents: Option<i64> = row.try_get("total_amount_cents")?;
                let currency: String = row.try_get("currency")?;
                let vendor_name: Option<String> = row.try_get("vendor_name")?;
                let capture_status: Option<String> = row.try_get("capture_status")?;
                let processing_status: Option<String> = row.try_get("processing_status")?;
                let invoice_date: Option<NaiveDate> = row.try_get("invoice_date")?;
                let due_date: Option<NaiveDate> = row.try_get("due_date")?;
                let po_number: Option<String> = row.try_get("po_number")?;
                let current_queue_id: Option<Uuid> = row.try_get("current_queue_id")?;
                let assigned_to: Option<Uuid> = row.try_get("assigned_to")?;
                let created_at: DateTime<Utc> = row.try_get("created_at")?;
                let updated_at: Option<DateTime<Utc>> = row.try_get("updated_at")?;

                let amount_display = total_cents
                    .map(|c| format!("{}.{:02}", c / 100, c % 100))
                    .unwrap_or_else(|| "N/A".to_string());

                Ok(format!(
                    "Invoice {} (ID: {})\n\
                     Status: {}\n\
                     Vendor: {}\n\
                     Amount: {} {}\n\
                     Capture Status: {}\n\
                     Processing Status: {}\n\
                     Invoice Date: {}\n\
                     Due Date: {}\n\
                     PO Number: {}\n\
                     Queue: {}\n\
                     Assigned To: {}\n\
                     Created: {}\n\
                     Updated: {}",
                    invoice_number,
                    id,
                    status,
                    vendor_name.unwrap_or_else(|| "Unknown".to_string()),
                    amount_display,
                    currency,
                    capture_status.unwrap_or_else(|| "N/A".to_string()),
                    processing_status.unwrap_or_else(|| "N/A".to_string()),
                    invoice_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                    due_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                    po_number.unwrap_or_else(|| "N/A".to_string()),
                    current_queue_id.map(|q| q.to_string()).unwrap_or_else(|| "None".to_string()),
                    assigned_to.map(|a| a.to_string()).unwrap_or_else(|| "Unassigned".to_string()),
                    created_at.format("%Y-%m-%d %H:%M"),
                    updated_at.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "N/A".to_string()),
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
                id, invoice_number, status, total_amount_cents, currency,
                vendor_name, capture_status, processing_status,
                invoice_date, due_date, po_number,
                created_at, updated_at
            FROM invoices
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        match invoice {
            Some(inv) => {
                let invoice_number: String = inv.try_get("invoice_number")?;
                let vendor_name: Option<String> = inv.try_get("vendor_name")?;
                let total_cents: Option<i64> = inv.try_get("total_amount_cents")?;
                let currency: String = inv.try_get("currency")?;
                let status: String = inv.try_get("status")?;
                let capture_status: Option<String> = inv.try_get("capture_status")?;
                let processing_status: Option<String> = inv.try_get("processing_status")?;
                let invoice_date: Option<NaiveDate> = inv.try_get("invoice_date")?;
                let due_date: Option<NaiveDate> = inv.try_get("due_date")?;
                let po_number: Option<String> = inv.try_get("po_number")?;
                let created_at: DateTime<Utc> = inv.try_get("created_at")?;
                let updated_at: Option<DateTime<Utc>> = inv.try_get("updated_at")?;

                let amount_display = total_cents
                    .map(|c| format!("{}.{:02}", c / 100, c % 100))
                    .unwrap_or_else(|| "N/A".to_string());

                Ok(format!(
                    "**Invoice Summary**\n\n\
                    **Invoice Number:** {}\n\
                    **Vendor:** {}\n\
                    **Amount:** {} {}\n\
                    **Status:** {}\n\
                    **Capture Status:** {}\n\
                    **Processing Status:** {}\n\
                    **Invoice Date:** {}\n\
                    **Due Date:** {}\n\
                    **PO Number:** {}\n\
                    **Created:** {}\n\
                    **Updated:** {}",
                    invoice_number,
                    vendor_name.unwrap_or_else(|| "Unknown".to_string()),
                    amount_display,
                    currency,
                    status,
                    capture_status.unwrap_or_else(|| "N/A".to_string()),
                    processing_status.unwrap_or_else(|| "N/A".to_string()),
                    invoice_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                    due_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                    po_number.unwrap_or_else(|| "N/A".to_string()),
                    created_at.format("%Y-%m-%d"),
                    updated_at.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "N/A".to_string()),
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

/// Read-only tool that searches the known issue register for relevant snippets.
/// No database queries, no mutations, no external calls.
pub struct SearchKnownIssuesTool;

#[async_trait]
impl Tool for SearchKnownIssuesTool {
    fn name(&self) -> &str {
        "search_known_issues"
    }

    fn description(&self) -> &str {
        "Search the known issue register for relevant issues. Args: query (plain text)"
    }

    async fn execute(&self, _context: &AgentContext, args: &str) -> Result<String> {
        let query = args.trim();
        if query.is_empty() {
            return Ok("Please provide a search query to look up known issues.".to_string());
        }

        let snippets = search_known_issues_context_for_query_with_limit(query, 5);

        if snippets.is_empty() {
            return Ok(format!(
                "No known issues found for query: '{}'. Try different keywords.",
                query
            ));
        }

        let mut lines = Vec::new();
        lines.push(format!("Known issue results for '{}':\n", query));

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

/// Read-only tool that summarizes release changes from indexed release notes.
/// No database queries, no mutations, no external calls.
pub struct SummarizeReleaseChangesTool;

#[async_trait]
impl Tool for SummarizeReleaseChangesTool {
    fn name(&self) -> &str {
        "summarize_release_changes"
    }

    fn description(&self) -> &str {
        "Summarize release changes from release notes. Args: query or version (optional plain text)"
    }

    async fn execute(&self, _context: &AgentContext, args: &str) -> Result<String> {
        let query = args.trim();
        let effective_query = if query.is_empty() {
            "recent release changes changelog fixes"
        } else {
            query
        };

        let snippets = release_changes_context_for_query_with_limit(effective_query, 5);

        if snippets.is_empty() {
            return Ok(format!(
                "No release changes found for query: '{}'. Try different keywords.",
                effective_query
            ));
        }

        let mut lines = Vec::new();
        lines.push("Release Summary\n".to_string());

        for (i, snippet) in snippets.iter().enumerate() {
            lines.push(format!(
                "{}. [{}] {}\n   {}",
                i + 1,
                snippet.source_path,
                snippet.heading,
                snippet.excerpt,
            ));
        }

        lines.push(String::new());
        lines.push(
            "Note: This summary is based solely on indexed release notes (CHANGELOG.md)."
                .to_string(),
        );

        Ok(lines.join("\n\n"))
    }
}

/// Read-only tool that explains workflow behavior for an invoice by examining
/// workflow state, queue items, approval requests, and audit history.
/// No mutations. Gracefully reports missing records.
pub struct ExplainWorkflowBehaviorTool {
    pool: PgPool,
}

impl ExplainWorkflowBehaviorTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Parse invoice_id from args: raw UUID text or JSON {"invoice_id":"..."}.
    fn parse_invoice_id(args: &str) -> Result<Uuid> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "Please provide an invoice_id to explain workflow behavior for. \
                 Expected: invoice UUID or {{\"invoice_id\":\"<uuid>\"}}."
            );
        }
        if trimmed.starts_with('{') {
            let val: serde_json::Value = serde_json::from_str(trimmed)
                .context("Invalid JSON input. Expected: {\"invoice_id\":\"<uuid>\"}.")?;
            let id_str = val
                .get("invoice_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "JSON input missing 'invoice_id' key. Expected: {{\"invoice_id\":\"<uuid>\"}}."
                    )
                })?;
            return id_str
                .parse::<Uuid>()
                .context("Invalid invoice_id format in JSON. Please provide a valid UUID.");
        }
        trimmed
            .parse::<Uuid>()
            .context("Invalid invoice ID format. Please provide a valid UUID.")
    }
}

#[async_trait]
impl Tool for ExplainWorkflowBehaviorTool {
    fn name(&self) -> &str {
        "explain_workflow_behavior"
    }

    fn description(&self) -> &str {
        "Explain workflow behavior for an invoice using workflow state and audit logs when available. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id = Self::parse_invoice_id(args)?;

        // --- Invoice State ---
        let invoice = sqlx::query(
            r#"
            SELECT invoice_number, status, processing_status,
                   current_queue_id, assigned_to,
                   total_amount_cents, currency
            FROM invoices
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        let invoice_row = match invoice {
            Some(r) => r,
            None => return Ok(format!("Invoice {} not found in your organization.", invoice_id)),
        };

        let invoice_number: String = invoice_row.try_get("invoice_number")?;
        let inv_status: String = invoice_row.try_get("status")?;
        let proc_status: String = invoice_row.try_get("processing_status")?;
        let current_queue_id: Option<Uuid> = invoice_row.try_get("current_queue_id")?;
        let assigned_to: Option<Uuid> = invoice_row.try_get("assigned_to")?;
        let total_cents: Option<i64> = invoice_row.try_get("total_amount_cents")?;
        let currency: String = invoice_row.try_get("currency")?;

        let mut sections: Vec<String> = Vec::new();

        // Section: Invoice State
        sections.push(format!(
            "## Invoice State\n\
             - Invoice: {} (ID: {})\n\
             - Status: {}\n\
             - Processing Status: {}\n\
             - Current Queue ID: {}\n\
             - Assigned To: {}\n\
             - Amount: {} {}",
            invoice_number,
            invoice_id,
            inv_status,
            proc_status,
            current_queue_id
                .map(|q| q.to_string())
                .unwrap_or_else(|| "None".to_string()),
            assigned_to
                .map(|a| a.to_string())
                .unwrap_or_else(|| "Unassigned".to_string()),
            total_cents
                .map(|c| format!("{}.{:02}", c / 100, c % 100))
                .unwrap_or_else(|| "N/A".to_string()),
            currency,
        ));

        // --- Workflow Queue State ---
        let queue_items = sqlx::query(
            r#"
            SELECT qi.id, qi.status AS qi_status, qi.priority, qi.due_at,
                   qi.completion_action, qi.notes,
                   wq.name AS queue_name, wq.queue_type
            FROM queue_items qi
            JOIN work_queues wq ON wq.id = qi.queue_id
            WHERE qi.invoice_id = $1 AND qi.tenant_id = $2
            ORDER BY qi.entered_at DESC
            LIMIT 10
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if queue_items.is_empty() {
            sections.push(
                "## Workflow Queue State\nNo queue items found for this invoice.".to_string(),
            );
        } else {
            let mut lines = vec!["## Workflow Queue State".to_string()];
            for qi in &queue_items {
                let q_name: String = qi.try_get("queue_name").unwrap_or_else(|_| "Unknown".to_string());
                let q_type: String = qi.try_get("queue_type").unwrap_or_else(|_| "Unknown".to_string());
                let qi_status: String = qi.try_get("qi_status").unwrap_or_else(|_| "Unknown".to_string());
                let priority: i32 = qi.try_get("priority").unwrap_or(0);
                let due_at: Option<DateTime<Utc>> = qi.try_get("due_at").ok().flatten();
                let comp_action: Option<String> = qi.try_get("completion_action").ok().flatten();
                let notes: Option<String> = qi.try_get("notes").ok().flatten();

                lines.push(format!(
                    "- Queue: {} ({}), Status: {}, Priority: {}{}{}{}",
                    q_name,
                    q_type,
                    qi_status,
                    priority,
                    due_at
                        .map(|d| format!(", Due: {}", d.format("%Y-%m-%d %H:%M")))
                        .unwrap_or_default(),
                    comp_action
                        .as_ref()
                        .map(|a| format!(", Action: {}", a))
                        .unwrap_or_default(),
                    notes
                        .as_ref()
                        .map(|n| format!(", Notes: {}", n))
                        .unwrap_or_default(),
                ));
            }
            sections.push(lines.join("\n"));
        }

        // --- Approval Requests ---
        let approvals = sqlx::query(
            r#"
            SELECT ar.status AS ar_status, ar.requested_from, ar.comments,
                   ar.responded_at,
                   wr.name AS rule_name, wr.priority AS rule_priority
            FROM approval_requests ar
            LEFT JOIN workflow_rules wr ON wr.id = ar.rule_id
            WHERE ar.invoice_id = $1 AND ar.tenant_id = $2
            ORDER BY ar.created_at DESC
            LIMIT 10
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if approvals.is_empty() {
            sections.push(
                "## Approval Requests\nNo approval requests found for this invoice."
                    .to_string(),
            );
        } else {
            let mut lines = vec!["## Approval Requests".to_string()];
            for ar in &approvals {
                let ar_status: String = ar.try_get("ar_status").unwrap_or_else(|_| "Unknown".to_string());
                let req_from: serde_json::Value =
                    ar.try_get("requested_from").unwrap_or(serde_json::Value::Null);
                let comments: Option<String> = ar.try_get("comments").ok().flatten();
                let responded_at: Option<DateTime<Utc>> = ar.try_get("responded_at").ok().flatten();
                let rule_name: Option<String> = ar.try_get("rule_name").ok().flatten();
                let rule_priority: Option<i32> = ar.try_get("rule_priority").ok().flatten();

                lines.push(format!(
                    "- Status: {}, Requested From: {}{}{}{}",
                    ar_status,
                    req_from,
                    rule_name
                        .as_ref()
                        .map(|n| format!(", Rule: {}", n))
                        .unwrap_or_default(),
                    rule_priority
                        .map(|p| format!(" (priority {})", p))
                        .unwrap_or_default(),
                    responded_at
                        .map(|r| format!(", Responded: {}", r.format("%Y-%m-%d %H:%M")))
                        .unwrap_or_default(),
                ));
                if let Some(c) = comments {
                    lines.push(format!("  Comments: {}", c));
                }
            }
            sections.push(lines.join("\n"));
        }

        // --- Audit Evidence ---
        let mut audit_lines = vec!["## Audit Evidence".to_string()];

        // invoice_audit_log (last 5)
        let inv_audit = sqlx::query(
            r#"
            SELECT from_status, to_status, event_type, actor_id, created_at
            FROM invoice_audit_log
            WHERE invoice_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            LIMIT 5
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if inv_audit.is_empty() {
            audit_lines.push("No invoice_audit_log rows found.".to_string());
        } else {
            audit_lines.push(format!("Invoice audit (latest {}):", inv_audit.len()));
            for row in &inv_audit {
                let from: Option<String> = row.try_get("from_status").ok().flatten();
                let to: String = row.try_get("to_status").unwrap_or_else(|_| "?".to_string());
                let evt: String = row.try_get("event_type").unwrap_or_else(|_| "?".to_string());
                let actor: Option<Uuid> = row.try_get("actor_id").ok().flatten();
                let at: DateTime<Utc> = row.try_get("created_at").unwrap_or_else(|_| Utc::now());
                audit_lines.push(format!(
                    "  {} -> {} ({}) by {} at {}",
                    from.unwrap_or_else(|| "—".to_string()),
                    to,
                    evt,
                    actor
                        .map(|a| a.to_string())
                        .unwrap_or_else(|| "system".to_string()),
                    at.format("%Y-%m-%d %H:%M"),
                ));
            }
        }

        // workflow_audit_log (last 5)
        let wf_audit = sqlx::query(
            r#"
            SELECT entity_type, entity_id, action, actor_type, created_at
            FROM workflow_audit_log
            WHERE entity_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            LIMIT 5
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if wf_audit.is_empty() {
            audit_lines.push("No workflow_audit_log rows found.".to_string());
        } else {
            audit_lines.push(format!("Workflow audit (latest {}):", wf_audit.len()));
            for row in &wf_audit {
                let etype: String = row.try_get("entity_type").unwrap_or_else(|_| "?".to_string());
                let action: String = row.try_get("action").unwrap_or_else(|_| "?".to_string());
                let atype: String = row.try_get("actor_type").unwrap_or_else(|_| "?".to_string());
                let at: DateTime<Utc> = row.try_get("created_at").unwrap_or_else(|_| Utc::now());
                audit_lines.push(format!(
                    "  {} {} (by {}) at {}",
                    etype,
                    action,
                    atype,
                    at.format("%Y-%m-%d %H:%M"),
                ));
            }
        }

        // audit_log for resource_id = invoice_id (last 5)
        let gen_audit = sqlx::query(
            r#"
            SELECT action, resource_type, resource_id, user_id, created_at
            FROM audit_log
            WHERE resource_id = $1::text AND tenant_id = $2
            ORDER BY created_at DESC
            LIMIT 5
            "#,
        )
        .bind(invoice_id.to_string())
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await;

        match gen_audit {
            Ok(rows) if rows.is_empty() => {
                audit_lines.push("No audit_log rows found.".to_string());
            }
            Ok(rows) => {
                audit_lines.push(format!("Generic audit log (latest {}):", rows.len()));
                for row in &rows {
                    let action: String = row.try_get("action").unwrap_or_else(|_| "?".to_string());
                    let rtype: String = row.try_get("resource_type").unwrap_or_else(|_| "?".to_string());
                    let uid: Option<Uuid> = row.try_get("user_id").ok().flatten();
                    let at: DateTime<Utc> = row.try_get("created_at").unwrap_or_else(|_| Utc::now());
                    audit_lines.push(format!(
                        "  {} on {} by {} at {}",
                        action,
                        rtype,
                        uid.map(|u| u.to_string()).unwrap_or_else(|| "system".to_string()),
                        at.format("%Y-%m-%d %H:%M"),
                    ));
                }
            }
            Err(_) => {
                audit_lines.push("No audit_log rows found.".to_string());
            }
        }

        sections.push(audit_lines.join("\n"));

        // --- Interpretation ---
        let mut interp = vec!["## Interpretation".to_string()];

        // Pending approvals -> blocked by approval
        let pending_approvals = approvals
            .iter()
            .filter(|r| {
                r.try_get::<String, _>("ar_status")
                    .map(|s| s == "pending")
                    .unwrap_or(false)
            })
            .count();
        if pending_approvals > 0 {
            interp.push(format!(
                "Invoice is blocked by {} pending approval request(s).",
                pending_approvals
            ));
        }

        // Completed queue items -> prior routing
        let completed_queues = queue_items
            .iter()
            .filter(|r| {
                r.try_get::<String, _>("qi_status")
                    .map(|s| s == "completed")
                    .unwrap_or(false)
            })
            .count();
        if completed_queues > 0 {
            interp.push(format!(
                "Invoice has passed through {} completed queue stage(s).",
                completed_queues
            ));
        }

        // No workflow rows at all
        if queue_items.is_empty() && approvals.is_empty() {
            interp.push(
                "No workflow state (queue items or approval requests) is recorded for this invoice.".to_string(),
            );
        }

        sections.push(interp.join("\n"));

        Ok(sections.join("\n\n"))
    }
}

/// Collection of available tools

// ── Search invoices tool ─────────────────────────────────────────────────────

/// Read-only tool for searching invoices with flexible filters.
/// No mutations, no external calls. Tenant-scoped.
pub struct SearchInvoicesTool {
    pool: PgPool,
}

impl SearchInvoicesTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for SearchInvoicesTool {
    fn name(&self) -> &str {
        "search_invoices"
    }

    fn description(&self) -> &str {
        "Search invoices with flexible filters. Args: JSON with optional query, vendor_name, invoice_number, status, capture_status, processing_status, due_before, due_after, min_amount_cents, max_amount_cents, and limit. Raw text is treated as query."
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let trimmed = args.trim();
        let filters: serde_json::Value = if trimmed.is_empty() {
            serde_json::json!({})
        } else if trimmed.starts_with('{') {
            serde_json::from_str(trimmed)
                .context("Invalid JSON input for search_invoices.")?
        } else {
            serde_json::json!({ "query": trimmed })
        };

        let query = filters.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let vendor_name = filters.get("vendor_name").and_then(|v| v.as_str());
        let invoice_number = filters.get("invoice_number").and_then(|v| v.as_str());
        let status = filters.get("status").and_then(|v| v.as_str());
        let capture_status = filters.get("capture_status").and_then(|v| v.as_str());
        let processing_status = filters.get("processing_status").and_then(|v| v.as_str());
        let due_before = filters.get("due_before").and_then(|v| v.as_str());
        let due_after = filters.get("due_after").and_then(|v| v.as_str());
        let min_amount_cents = filters.get("min_amount_cents").and_then(|v| v.as_i64());
        let max_amount_cents = filters.get("max_amount_cents").and_then(|v| v.as_i64());
        let limit_val = filters.get("limit").and_then(|v| v.as_u64()).unwrap_or(25);
        let limit = limit_val.min(25) as i64;

        // Build dynamic query with bind parameters
        let mut sql_parts = vec![
            r#"SELECT id, invoice_number, vendor_name, status, capture_status,"#.to_string(),
            r#"       processing_status, total_amount_cents, currency,"#.to_string(),
            r#"       invoice_date, due_date, created_at"#.to_string(),
            r#"FROM invoices"#.to_string(),
            r#"WHERE tenant_id = $1"#.to_string(),
        ];
        let mut param_idx = 2u32;

        // Text search across invoice_number and vendor_name
        if !query.is_empty() {
            sql_parts.push(format!(
                "  AND (invoice_number ILIKE ${} OR vendor_name ILIKE ${})",
                param_idx, param_idx
            ));
            param_idx += 1;
        }
        if vendor_name.is_some() {
            sql_parts.push(format!("  AND vendor_name ILIKE ${}", param_idx));
            param_idx += 1;
        }
        if invoice_number.is_some() {
            sql_parts.push(format!("  AND invoice_number ILIKE ${}", param_idx));
            param_idx += 1;
        }
        if status.is_some() {
            sql_parts.push(format!("  AND status = ${}", param_idx));
            param_idx += 1;
        }
        if capture_status.is_some() {
            sql_parts.push(format!("  AND capture_status = ${}", param_idx));
            param_idx += 1;
        }
        if processing_status.is_some() {
            sql_parts.push(format!("  AND processing_status = ${}", param_idx));
            param_idx += 1;
        }
        if due_before.is_some() {
            sql_parts.push(format!("  AND due_date <= ${}", param_idx));
            param_idx += 1;
        }
        if due_after.is_some() {
            sql_parts.push(format!("  AND due_date >= ${}", param_idx));
            param_idx += 1;
        }
        if min_amount_cents.is_some() {
            sql_parts.push(format!("  AND total_amount_cents >= ${}", param_idx));
            param_idx += 1;
        }
        if max_amount_cents.is_some() {
            sql_parts.push(format!("  AND total_amount_cents <= ${}", param_idx));
            param_idx += 1;
        }

        sql_parts.push(format!("ORDER BY created_at DESC LIMIT ${}", param_idx));

        let sql = sql_parts.join("\n");
        let mut q = sqlx::query(&sql).bind(&context.tenant_id);

        if !query.is_empty() {
            q = q.bind(format!("%{}%", query));
        }
        if let Some(vn) = vendor_name {
            q = q.bind(format!("%{}%", vn));
        }
        if let Some(inv_num) = invoice_number {
            q = q.bind(format!("%{}%", inv_num));
        }
        if let Some(s) = status {
            q = q.bind(s);
        }
        if let Some(cs) = capture_status {
            q = q.bind(cs);
        }
        if let Some(ps) = processing_status {
            q = q.bind(ps);
        }
        if let Some(db) = due_before {
            let dt: NaiveDate = db.parse()
                .context("Invalid due_before date format. Use YYYY-MM-DD.")?;
            q = q.bind(dt);
        }
        if let Some(da) = due_after {
            let dt: NaiveDate = da.parse()
                .context("Invalid due_after date format. Use YYYY-MM-DD.")?;
            q = q.bind(dt);
        }
        if let Some(min) = min_amount_cents {
            q = q.bind(min);
        }
        if let Some(max) = max_amount_cents {
            q = q.bind(max);
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pool).await?;

        if rows.is_empty() {
            return Ok("No invoices found matching the given criteria.".to_string());
        }

        let mut lines = vec![format!("Found {} invoice(s):\n", rows.len())];
        for row in &rows {
            let id: Uuid = row.try_get("id")?;
            let inv_num: String = row.try_get("invoice_number").unwrap_or_else(|_| "N/A".to_string());
            let vn: Option<String> = row.try_get("vendor_name").ok().flatten();
            let st: String = row.try_get("status").unwrap_or_else(|_| "N/A".to_string());
            let cents: Option<i64> = row.try_get("total_amount_cents").ok().flatten();
            let cur: String = row.try_get("currency").unwrap_or_else(|_| "USD".to_string());
            let dd: Option<NaiveDate> = row.try_get("due_date").ok().flatten();
            let ca: DateTime<Utc> = row.try_get("created_at").unwrap_or_else(|_| Utc::now());

            let amt = cents
                .map(|c| format!("{}.{:02}", c / 100, c % 100))
                .unwrap_or_else(|| "N/A".to_string());

            lines.push(format!(
                "- {} | {} | {} | {} {} | Due: {} | Created: {} (ID: {})",
                inv_num,
                vn.unwrap_or_else(|| "Unknown".to_string()),
                st,
                amt,
                cur,
                dd.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                ca.format("%Y-%m-%d"),
                id,
            ));
        }

        Ok(lines.join("\n"))
    }
}

// ── Duplicate invoice candidates tool ────────────────────────────────────────

/// Read-only tool for finding potential duplicate invoices.
/// No mutations, no external services. Tenant-scoped.
pub struct FindDuplicateInvoiceCandidatesTool {
    pool: PgPool,
}

impl FindDuplicateInvoiceCandidatesTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn parse_invoice_id(args: &str) -> Result<Uuid> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "Please provide an invoice_id to find duplicate candidates for. \
                 Expected: invoice UUID or {{\"invoice_id\":\"<uuid>\"}}."
            );
        }
        if trimmed.starts_with('{') {
            let val: serde_json::Value = serde_json::from_str(trimmed)
                .context("Invalid JSON input. Expected: {\"invoice_id\":\"<uuid>\"}.")?;
            let id_str = val
                .get("invoice_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "JSON input missing 'invoice_id' key. Expected: {{\"invoice_id\":\"<uuid>\"}}."
                    )
                })?;
            return id_str
                .parse::<Uuid>()
                .context("Invalid invoice_id format in JSON. Please provide a valid UUID.");
        }
        trimmed
            .parse::<Uuid>()
            .context("Invalid invoice ID format. Please provide a valid UUID.")
    }
}

#[async_trait]
impl Tool for FindDuplicateInvoiceCandidatesTool {
    fn name(&self) -> &str {
        "find_duplicate_invoice_candidates"
    }

    fn description(&self) -> &str {
        "Find potential duplicate invoices for a given invoice. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id = Self::parse_invoice_id(args)?;

        // Load the target invoice
        let target = sqlx::query(
            r#"
            SELECT invoice_number, vendor_name, total_amount_cents,
                   invoice_date, due_date
            FROM invoices
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        let target = match target {
            Some(t) => t,
            None => return Ok(format!("Invoice {} not found in your organization.", invoice_id)),
        };

        let t_inv_num: Option<String> = target.try_get("invoice_number").ok().flatten();
        let t_vendor: Option<String> = target.try_get("vendor_name").ok().flatten();
        let t_amount: Option<i64> = target.try_get("total_amount_cents").ok().flatten();
        let t_invoice_date: Option<NaiveDate> = target.try_get("invoice_date").ok().flatten();
        let t_due_date: Option<NaiveDate> = target.try_get("due_date").ok().flatten();

        // Find candidates: same invoice_number, or same vendor_name+amount, or same vendor_name+near dates
        let candidates = sqlx::query(
            r#"
            SELECT id, invoice_number, vendor_name, total_amount_cents,
                   invoice_date, due_date, created_at
            FROM invoices
            WHERE tenant_id = $1
              AND id != $2
              AND (
                  (invoice_number IS NOT NULL AND invoice_number = $3)
                  OR (vendor_name IS NOT NULL AND vendor_name = $4
                      AND total_amount_cents IS NOT NULL AND total_amount_cents = $5)
                  OR (vendor_name IS NOT NULL AND vendor_name = $4
                      AND invoice_date IS NOT NULL
                      AND $6::date IS NOT NULL
                      AND ABS(invoice_date - $6::date) < 3)
                  OR (vendor_name IS NOT NULL AND vendor_name = $4
                      AND due_date IS NOT NULL
                      AND $7::date IS NOT NULL
                      AND ABS(due_date - $7::date) < 3)
              )
            ORDER BY created_at DESC
            LIMIT 25
            "#,
        )
        .bind(&context.tenant_id)
        .bind(invoice_id)
        .bind(&t_inv_num)
        .bind(&t_vendor)
        .bind(&t_amount)
        .bind(&t_invoice_date)
        .bind(&t_due_date)
        .fetch_all(&self.pool)
        .await?;

        if candidates.is_empty() {
            return Ok(format!(
                "No duplicate candidates found for invoice {}.",
                invoice_id
            ));
        }

        let mut lines = vec![format!(
            "Found {} potential duplicate candidate(s) for invoice {}:\n",
            candidates.len(),
            invoice_id
        )];

        for row in &candidates {
            let c_id: Uuid = row.try_get("id")?;
            let c_inv_num: Option<String> = row.try_get("invoice_number").ok().flatten();
            let c_vendor: Option<String> = row.try_get("vendor_name").ok().flatten();
            let c_amount: Option<i64> = row.try_get("total_amount_cents").ok().flatten();
            let c_inv_date: Option<NaiveDate> = row.try_get("invoice_date").ok().flatten();
            let c_due_date: Option<NaiveDate> = row.try_get("due_date").ok().flatten();

            let mut reasons = Vec::new();

            // Check match reasons
            if let (Some(ref t_num), Some(ref c_num)) = (&t_inv_num, &c_inv_num) {
                if t_num == c_num {
                    reasons.push("same invoice_number".to_string());
                }
            }
            if let (Some(ref t_v), Some(ref c_v)) = (&t_vendor, &c_vendor) {
                if t_v == c_v {
                    if t_amount == c_amount && t_amount.is_some() {
                        reasons.push("same vendor + amount".to_string());
                    } else {
                        // Check near dates
                        if let (Some(td), Some(cd)) = (t_invoice_date, c_inv_date) {
                            let diff = (td - cd).num_days().abs();
                            if diff <= 3 {
                                reasons.push(format!("same vendor + invoice_date within {} day(s)", diff));
                            }
                        }
                        if let (Some(td), Some(cd)) = (t_due_date, c_due_date) {
                            let diff = (td - cd).num_days().abs();
                            if diff <= 3 {
                                reasons.push(format!("same vendor + due_date within {} day(s)", diff));
                            }
                        }
                    }
                }
            }

            let amt = c_amount
                .map(|c| format!("{}.{:02}", c / 100, c % 100))
                .unwrap_or_else(|| "N/A".to_string());

            lines.push(format!(
                "- ID: {} | {} | {} | {} | Invoice Date: {} | Due: {} | Reason: {}",
                c_id,
                c_inv_num.unwrap_or_else(|| "N/A".to_string()),
                c_vendor.unwrap_or_else(|| "Unknown".to_string()),
                amt,
                c_inv_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                c_due_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string()),
                reasons.join(", "),
            ));
        }

        Ok(lines.join("\n"))
    }
}

// ── Payment risk assessment tool ─────────────────────────────────────────────

/// Read-only tool for assessing invoice payment risk.
/// No mutations, no external services. Tenant-scoped.
/// Defensively handles missing optional support tables.
pub struct AssessInvoicePaymentRiskTool {
    pool: PgPool,
}

impl AssessInvoicePaymentRiskTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn parse_invoice_id(args: &str) -> Result<Uuid> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "Please provide an invoice_id to assess payment risk for. \
                 Expected: invoice UUID or {{\"invoice_id\":\"<uuid>\"}}."
            );
        }
        if trimmed.starts_with('{') {
            let val: serde_json::Value = serde_json::from_str(trimmed)
                .context("Invalid JSON input. Expected: {\"invoice_id\":\"<uuid>\"}.")?;
            let id_str = val
                .get("invoice_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "JSON input missing 'invoice_id' key. Expected: {{\"invoice_id\":\"<uuid>\"}}."
                    )
                })?;
            return id_str
                .parse::<Uuid>()
                .context("Invalid invoice_id format in JSON. Please provide a valid UUID.");
        }
        trimmed
            .parse::<Uuid>()
            .context("Invalid invoice ID format. Please provide a valid UUID.")
    }
}

#[async_trait]
impl Tool for AssessInvoicePaymentRiskTool {
    fn name(&self) -> &str {
        "assess_invoice_payment_risk"
    }

    fn description(&self) -> &str {
        "Assess payment risk for an invoice. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id = Self::parse_invoice_id(args)?;

        // Load the invoice
        let invoice = sqlx::query(
            r#"
            SELECT invoice_number, vendor_name, total_amount_cents, currency,
                   status, processing_status, invoice_date, due_date, created_at
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
            None => return Ok(format!("Invoice {} not found in your organization.", invoice_id)),
        };

        let inv_num: Option<String> = invoice.try_get("invoice_number").ok().flatten();
        let vendor: Option<String> = invoice.try_get("vendor_name").ok().flatten();
        let amount_cents: Option<i64> = invoice.try_get("total_amount_cents").ok().flatten();
        let currency: String = invoice.try_get("currency").unwrap_or_else(|_| "USD".to_string());
        let status: String = invoice.try_get("status").unwrap_or_else(|_| "N/A".to_string());
        let proc_status: Option<String> = invoice.try_get("processing_status").ok().flatten();
        let invoice_date: Option<NaiveDate> = invoice.try_get("invoice_date").ok().flatten();
        let due_date: Option<NaiveDate> = invoice.try_get("due_date").ok().flatten();

        let mut risk_signals: Vec<String> = Vec::new();
        let mut evidence: Vec<String> = Vec::new();
        let mut risk_score: i32 = 0; // 0-100 scale

        // --- Signal 1: Overdue check ---
        if let Some(dd) = due_date {
            let today = Utc::now().date_naive();
            if dd < today {
                let days_overdue = (today - dd).num_days();
                if days_overdue > 0 {
                    risk_score += 30;
                    risk_signals.push(format!("Invoice is {} day(s) overdue (due: {}).", days_overdue, dd));
                    evidence.push(format!("due_date {} is {} days past", dd, days_overdue));
                }
            } else {
                let days_remaining = (dd - today).num_days();
                if days_remaining <= 3 {
                    risk_score += 10;
                    risk_signals.push(format!("Invoice due in {} day(s).", days_remaining));
                    evidence.push(format!("due_date {} is {} days away", dd, days_remaining));
                }
            }
        }

        // --- Signal 2: Processing status ---
        if let Some(ref ps) = proc_status {
            if ps == "pending_approval" {
                risk_score += 15;
                risk_signals.push("Invoice is pending approval.".to_string());
                evidence.push(format!("processing_status: {}", ps));
            } else if ps == "rejected" {
                risk_score += 40;
                risk_signals.push("Invoice has been rejected.".to_string());
                evidence.push(format!("processing_status: {}", ps));
            } else if ps == "draft" {
                risk_score += 20;
                risk_signals.push("Invoice is still in draft.".to_string());
                evidence.push(format!("processing_status: {}", ps));
            }
        }

        // --- Signal 3: High amount ---
        if let Some(cents) = amount_cents {
            if cents > 100_000_00 {
                risk_score += 10;
                risk_signals.push(format!("High-value invoice: {}.{:02} {}.", cents / 100, cents % 100, currency));
                evidence.push(format!("total_amount_cents: {}", cents));
            }
        }

        // --- Signal 4: Duplicate candidates (defensive, same criteria as find_duplicate_invoice_candidates) ---
        let dup_result = sqlx::query(
            r#"
            SELECT COUNT(*) as cnt
            FROM invoices
            WHERE tenant_id = $1
              AND id != $2
              AND (
                  (invoice_number IS NOT NULL AND invoice_number = $3)
                  OR (vendor_name IS NOT NULL AND vendor_name = $4
                      AND total_amount_cents IS NOT NULL AND total_amount_cents = $5)
                  OR (vendor_name IS NOT NULL AND vendor_name = $4
                      AND invoice_date IS NOT NULL
                      AND $6::date IS NOT NULL
                      AND ABS(invoice_date - $6::date) < 3)
                  OR (vendor_name IS NOT NULL AND vendor_name = $4
                      AND due_date IS NOT NULL
                      AND $7::date IS NOT NULL
                      AND ABS(due_date - $7::date) < 3)
              )
            "#,
        )
        .bind(&context.tenant_id)
        .bind(invoice_id)
        .bind(&inv_num)
        .bind(&vendor)
        .bind(&amount_cents)
        .bind(&invoice_date)
        .bind(&due_date)
        .fetch_optional(&self.pool)
        .await;

        match dup_result {
            Ok(Some(row)) => {
                let cnt: i64 = row.try_get("cnt").unwrap_or(0);
                if cnt > 0 {
                    risk_score += 20;
                    risk_signals.push(format!("{} potential duplicate candidate(s) found.", cnt));
                    evidence.push(format!("duplicate_candidates: {}", cnt));
                }
            }
            Ok(None) => {}
            Err(_) => {
                // Defensively skip this signal if table/query fails
                evidence.push("duplicate_candidates: unable to check".to_string());
            }
        }

        // --- Signal 5: Active payment requests (defensive) ---
        let payment_result = sqlx::query(
            r#"
            SELECT pr.status as pr_status
            FROM payment_request_items pri
            JOIN payment_requests pr ON pr.id = pri.payment_request_id
            WHERE pri.invoice_id = $1 AND pr.tenant_id = $2
            LIMIT 5
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await;

        match payment_result {
            Ok(rows) if !rows.is_empty() => {
                let active: Vec<String> = rows
                    .iter()
                    .filter_map(|r| r.try_get::<String, _>("pr_status").ok())
                    .filter(|s| s == "pending" || s == "submitted")
                    .collect();
                if !active.is_empty() {
                    evidence.push(format!("active payment_requests: {}", active.len()));
                    // No risk penalty, but informative
                } else {
                    evidence.push(format!("payment_requests found: {} (all resolved)", rows.len()));
                }
            }
            Ok(_) => {
                evidence.push("payment_requests: none found".to_string());
            }
            Err(_) => {
                // Defensively skip if table is unavailable
                evidence.push("payment_requests: unable to check".to_string());
            }
        }

        // --- Signal 6: Pending approval requests (defensive) ---
        let approval_result = sqlx::query(
            r#"
            SELECT status as ar_status
            FROM approval_requests
            WHERE invoice_id = $1 AND tenant_id = $2
            LIMIT 5
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await;

        match approval_result {
            Ok(rows) if !rows.is_empty() => {
                let pending: Vec<String> = rows
                    .iter()
                    .filter_map(|r| r.try_get::<String, _>("ar_status").ok())
                    .filter(|s| s == "pending")
                    .collect();
                if !pending.is_empty() {
                    risk_score += 15;
                    risk_signals.push(format!("{} pending approval request(s).", pending.len()));
                    evidence.push(format!("pending_approvals: {}", pending.len()));
                }
            }
            Ok(_) => {}
            Err(_) => {
                evidence.push("approval_requests: unable to check".to_string());
            }
        }

        // Cap risk score
        risk_score = risk_score.min(100);

        let risk_level = if risk_score >= 50 {
            "HIGH"
        } else if risk_score >= 25 {
            "MEDIUM"
        } else {
            "LOW"
        };

        let amt_display = amount_cents
            .map(|c| format!("{}.{:02}", c / 100, c % 100))
            .unwrap_or_else(|| "N/A".to_string());

        let mut lines = vec![
            format!("Payment Risk Assessment for Invoice {}", invoice_id),
            format!("Invoice: {} | Vendor: {} | Amount: {} {}", 
                inv_num.unwrap_or_else(|| "N/A".to_string()),
                vendor.unwrap_or_else(|| "Unknown".to_string()),
                amt_display, currency),
            format!("Status: {} | Processing: {}", status, proc_status.unwrap_or_else(|| "N/A".to_string())),
            format!("Due Date: {}", due_date.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string())),
            String::new(),
            format!("Risk Level: {} (score: {}/100)", risk_level, risk_score),
        ];

        if !risk_signals.is_empty() {
            lines.push(String::new());
            lines.push("Risk Signals:".to_string());
            for signal in &risk_signals {
                lines.push(format!("  - {}", signal));
            }
        }

        if !evidence.is_empty() {
            lines.push(String::new());
            lines.push("Evidence:".to_string());
            for ev in &evidence {
                lines.push(format!("  - {}", ev));
            }
        }

        Ok(lines.join("\n"))
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

    /// Return the authoritative list of typed tool definitions for every
    /// executable tool.  The caller must not mutate the returned vec.
    pub fn tool_definitions() -> Vec<AiToolDefinition> {
        vec![
            AiToolDefinition {
                name: "get_invoice_status",
                description: "Get status of an invoice by ID. Args: invoice_id (UUID)",
                class: AiToolClass::Invoice,
                required_permission: AiToolPermission::InvoiceRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["invoice_id"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_number": { "type": "string" },
                        "status": { "type": "string" },
                        "vendor_name": { "type": "string" },
                        "total_amount": { "type": "number" },
                        "currency": { "type": "string" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_vendor_invoices",
                description: "Find all invoices from a vendor. Args: vendor_name",
                class: AiToolClass::Vendor,
                required_permission: AiToolPermission::VendorRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "vendor_name": { "type": "string" }
                    },
                    "required": ["vendor_name"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoices": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "invoice_number": { "type": "string" },
                                    "status": { "type": "string" },
                                    "total_amount": { "type": "number" },
                                    "currency": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_approval_requirements",
                description: "Check who needs to approve an invoice. Args: invoice_id (UUID)",
                class: AiToolClass::Approval,
                required_permission: AiToolPermission::ApprovalRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["invoice_id"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "steps": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "step_order": { "type": "integer" },
                                    "approver_role": { "type": "string" },
                                    "status": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "summarize_invoice",
                description: "Generate a summary of an invoice. Args: invoice_id (UUID)",
                class: AiToolClass::Invoice,
                required_permission: AiToolPermission::InvoiceRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["invoice_id"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_number": { "type": "string" },
                        "vendor": { "type": "string" },
                        "amount": { "type": "number" },
                        "currency": { "type": "string" },
                        "status": { "type": "string" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_module_capabilities",
                description: "Report which modules are enabled for the tenant and describe capability boundaries. No args required.",
                class: AiToolClass::TenantCapability,
                required_permission: AiToolPermission::TenantModuleRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "modules": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "key": { "type": "string" },
                                    "enabled": { "type": "boolean" }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "search_product_docs",
                description: "Search BillForge product documentation for relevant snippets. Args: query (plain text)",
                class: AiToolClass::ProductKnowledge,
                required_permission: AiToolPermission::ProductKnowledgeRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "snippets": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "source_path": { "type": "string" },
                                    "heading": { "type": "string" },
                                    "excerpt": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "explain_feature",
                description: "Explain a BillForge feature or concept using product documentation. Args: feature (name or question)",
                class: AiToolClass::ProductKnowledge,
                required_permission: AiToolPermission::ProductKnowledgeRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "feature": { "type": "string" }
                    },
                    "required": ["feature"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "explanation": { "type": "string" },
                        "sources": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "search_known_issues",
                description: "Search the known issue register for relevant issues. Args: query (plain text)",
                class: AiToolClass::ProductKnowledge,
                required_permission: AiToolPermission::ProductKnowledgeRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "issues": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "source_path": { "type": "string" },
                                    "heading": { "type": "string" },
                                    "excerpt": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "summarize_release_changes",
                description: "Summarize release changes from release notes. Args: query or version (optional plain text)",
                class: AiToolClass::ProductKnowledge,
                required_permission: AiToolPermission::ProductKnowledgeRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string" },
                        "sources": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "explain_workflow_behavior",
                description: "Explain workflow behavior for an invoice using workflow state and audit logs. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})",
                class: AiToolClass::Workflow,
                required_permission: AiToolPermission::WorkflowRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["invoice_id"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_state": { "type": "object" },
                        "queue_items": { "type": "array" },
                        "approval_requests": { "type": "array" },
                        "audit_evidence": { "type": "array" },
                        "interpretation": { "type": "array" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "request_issue_creation",
                description: "Prepare an issue creation request for approval. Does NOT create a GitHub, Linear, Jira, or internal feedback record. Args: JSON {\"target\":\"github|linear|jira|internal_feedback_table\",\"kind\":\"bug|feature_request|support_request|other\",\"title\":\"...\",\"body\":\"...\",\"labels\":[...],\"source_conversation_id\":\"...\",\"source_conversation_link\":\"...\",\"metadata\":{}}",
                class: AiToolClass::IssueIntake,
                required_permission: AiToolPermission::IssueRequest,
                risk_level: AiToolRiskLevel::Medium,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target": { "type": "string", "enum": ["github", "linear", "jira", "internal_feedback_table"] },
                        "kind": { "type": "string", "enum": ["bug", "feature_request", "support_request", "other"] },
                        "title": { "type": "string" },
                        "body": { "type": "string" },
                        "labels": { "type": "array", "items": { "type": "string" } },
                        "source_conversation_id": { "type": "string" },
                        "source_conversation_link": { "type": "string" },
                        "metadata": { "type": "object" }
                    },
                    "required": ["target", "kind", "title", "body"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["approval_required"] },
                        "approval_request_id": { "type": "string", "format": "uuid" },
                        "request": { "type": "object" },
                        "message": { "type": "string" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "search_invoices",
                description: "Search invoices with flexible filters. Args: JSON with optional query, vendor_name, invoice_number, status, capture_status, processing_status, due_before, due_after, min_amount_cents, max_amount_cents, and limit. Raw text is treated as query.",
                class: AiToolClass::Invoice,
                required_permission: AiToolPermission::InvoiceRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Free-text search across invoice_number and vendor_name" },
                        "vendor_name": { "type": "string" },
                        "invoice_number": { "type": "string" },
                        "status": { "type": "string" },
                        "capture_status": { "type": "string" },
                        "processing_status": { "type": "string" },
                        "due_before": { "type": "string", "format": "date" },
                        "due_after": { "type": "string", "format": "date" },
                        "min_amount_cents": { "type": "integer" },
                        "max_amount_cents": { "type": "integer" },
                        "limit": { "type": "integer", "maximum": 25 }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoices": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string", "format": "uuid" },
                                    "invoice_number": { "type": "string" },
                                    "vendor_name": { "type": "string" },
                                    "status": { "type": "string" },
                                    "total_amount_cents": { "type": "integer" },
                                    "due_date": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "find_duplicate_invoice_candidates",
                description: "Find potential duplicate invoices for a given invoice. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})",
                class: AiToolClass::Invoice,
                required_permission: AiToolPermission::InvoiceRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["invoice_id"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "candidates": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string", "format": "uuid" },
                                    "invoice_number": { "type": "string" },
                                    "vendor_name": { "type": "string" },
                                    "match_reasons": { "type": "array", "items": { "type": "string" } }
                                }
                            }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "assess_invoice_payment_risk",
                description: "Assess payment risk for an invoice based on due date, processing status, duplicates, and payment/approval activity. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})",
                class: AiToolClass::Invoice,
                required_permission: AiToolPermission::InvoiceRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["invoice_id"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "risk_level": { "type": "string", "enum": ["LOW", "MEDIUM", "HIGH"] },
                        "risk_score": { "type": "integer" },
                        "risk_signals": { "type": "array", "items": { "type": "string" } },
                        "evidence": { "type": "array", "items": { "type": "string" } }
                    }
                }),
                mutates: false,
            },
        ]
    }

    /// Look up a single typed definition by tool name.
    pub fn get_tool_definition(tool_name: &str) -> Option<AiToolDefinition> {
        Self::tool_definitions()
            .into_iter()
            .find(|d| d.name == tool_name)
    }

    pub fn get_tool_descriptions(&self) -> String {
        Self::tool_definitions()
            .iter()
            .map(|d| format!("- {}: {}", d.name, d.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn execute_tool(
        &self,
        tool_name: &str,
        context: &AgentContext,
        args: &str,
    ) -> Result<String> {
        // Reject unknown tools early via the typed registry.
        if Self::get_tool_definition(tool_name).is_none() {
            anyhow::bail!("Tool '{}' not found", tool_name);
        }

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
            "search_known_issues" => {
                SearchKnownIssuesTool.execute(context, args).await
            }
            "summarize_release_changes" => {
                SummarizeReleaseChangesTool.execute(context, args).await
            }
            "explain_workflow_behavior" => {
                ExplainWorkflowBehaviorTool::new(self.pool.clone()).execute(context, args).await
            }
            "request_issue_creation" => {
                let request: super::issue_intake::IssueCreationRequest =
                    serde_json::from_str(args.trim()).context(
                        "Invalid JSON for request_issue_creation. \
                         Expected: {\"target\":\"github\", \"kind\":\"bug\", \"title\":\"...\", \"body\":\"...\"}",
                    )?;
                let envelope = super::issue_intake::prepare_issue_creation_for_approval(request)?;
                serde_json::to_string(&envelope).context("Failed to serialize approval envelope")
            }
            "search_invoices" => {
                SearchInvoicesTool::new(self.pool.clone()).execute(context, args).await
            }
            "find_duplicate_invoice_candidates" => {
                FindDuplicateInvoiceCandidatesTool::new(self.pool.clone()).execute(context, args).await
            }
            "assess_invoice_payment_risk" => {
                AssessInvoicePaymentRiskTool::new(self.pool.clone()).execute(context, args).await
            }
            _ => anyhow::bail!("Tool '{}' not found", tool_name),
        }
    }
}
