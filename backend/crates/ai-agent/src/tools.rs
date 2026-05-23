//! AI Agent Tools - Invoice query capabilities

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
            ("search_known_issues", "Search the known issue register for relevant issues. Args: query (plain text)"),
            ("summarize_release_changes", "Summarize release changes from release notes. Args: query or version (optional plain text)"),
            ("explain_workflow_behavior", "Explain workflow behavior for an invoice using workflow state and audit logs. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})"),
            ("request_issue_creation", "Prepare an issue creation request for approval. Does NOT create a GitHub, Linear, Jira, or internal feedback record. Args: JSON {\"target\":\"github|linear|jira|internal_feedback_table\",\"kind\":\"bug|feature_request|support_request|other\",\"title\":\"...\",\"body\":\"...\",\"labels\":[...],\"source_conversation_id\":\"...\",\"source_conversation_link\":\"...\",\"metadata\":{}}"),
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
            _ => anyhow::bail!("Tool '{}' not found", tool_name),
        }
    }
}
