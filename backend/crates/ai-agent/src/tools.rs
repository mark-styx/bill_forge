//! AI Agent Tools - Invoice query capabilities

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

use billforge_core::{
    domain::{ApprovalStatus, ApprovalTarget, InvoiceId, ProcessingStatus},
    traits::{ApprovalRepository, InvoiceRepository, WorkQueueRepository, WorkflowRuleRepository},
    Role, TenantId, UserContext, UserId,
};
use billforge_db::repositories::{InvoiceRepositoryImpl, WorkflowRepositoryImpl};

use super::models::AgentContext;
use super::product_knowledge::{
    product_knowledge_context_for_query_with_limit, release_changes_context_for_query_with_limit,
    search_known_issues_context_for_query_with_limit,
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
        // Integration modules — Winston does not have integration-specific tools,
        // so these share a generic boundary message.
        m @ (billforge_core::Module::Quickbooks
        | billforge_core::Module::Xero
        | billforge_core::Module::NetSuite
        | billforge_core::Module::SageIntacct
        | billforge_core::Module::Salesforce
        | billforge_core::Module::Workday
        | billforge_core::Module::BillCom
        | billforge_core::Module::Edi) => ModuleInfo {
            key: m.as_str(),
            display_name: m.display_name(),
            enabled_help: "This integration module is enabled for your organization.",
            disabled_boundary: "This integration is not available for your organization. Contact your administrator to enable this module.",
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
    AdminAnalysis,
}

/// Permission required to invoke an AI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiToolPermission {
    InvoiceRead,
    VendorRead,
    ApprovalRead,
    ApprovalRespond,
    TenantModuleRead,
    ProductKnowledgeRead,
    WorkflowRead,
    IssueRequest,
    AdminAnalyticsRead,
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

/// Approval context required before executing mutating or high-risk AI tools.
#[derive(Debug, Clone)]
pub struct ToolProposalContext {
    pub proposal_id: Uuid,
    pub tool_name: String,
    pub approved: bool,
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
                    invoice_date
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    due_date
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    po_number.unwrap_or_else(|| "N/A".to_string()),
                    current_queue_id
                        .map(|q| q.to_string())
                        .unwrap_or_else(|| "None".to_string()),
                    assigned_to
                        .map(|a| a.to_string())
                        .unwrap_or_else(|| "Unassigned".to_string()),
                    created_at.format("%Y-%m-%d %H:%M"),
                    updated_at
                        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
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
            JOIN vendors v ON i.vendor_id = v.id AND v.tenant_id = i.tenant_id
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

/// Vendor summary tool - read-only tenant-scoped vendor profile with invoice metrics.
pub struct VendorSummaryTool {
    pool: PgPool,
}

impl VendorSummaryTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for VendorSummaryTool {
    fn name(&self) -> &str {
        "get_vendor_summary"
    }

    fn description(&self) -> &str {
        "Get a vendor summary including contact, payment terms, and invoice metrics. Args: vendor_id or vendor_name (raw text or JSON)"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "Please provide a vendor_id or vendor_name. \
                 Expected: vendor name, vendor UUID, or {{\"vendor_id\":\"<uuid>\"}} / {{\"vendor_name\":\"...\"}}."
            );
        }

        // Parse args: JSON or raw text
        let (vendor_id_opt, vendor_name_opt) = if trimmed.starts_with('{') {
            let val: serde_json::Value = serde_json::from_str(trimmed)
                .context("Invalid JSON input for get_vendor_summary.")?;
            let vid = val
                .get("vendor_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Uuid>().ok());
            let vn = val
                .get("vendor_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            (vid, vn)
        } else if let Ok(uid) = trimmed.parse::<Uuid>() {
            (Some(uid), None)
        } else {
            (None, Some(trimmed.to_string()))
        };

        // Look up vendor row
        let vendor = if let Some(vid) = vendor_id_opt {
            sqlx::query(
                r#"SELECT id, name, is_active, contact_email, contact_phone, payment_terms
                   FROM vendors WHERE id = $1 AND tenant_id = $2"#,
            )
            .bind(vid)
            .bind(&context.tenant_id)
            .fetch_optional(&self.pool)
            .await?
        } else if let Some(ref vn) = vendor_name_opt {
            sqlx::query(
                r#"SELECT id, name, is_active, contact_email, contact_phone, payment_terms
                   FROM vendors WHERE name ILIKE $1 AND tenant_id = $2
                   LIMIT 1"#,
            )
            .bind(format!("%{}%", vn))
            .bind(&context.tenant_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            anyhow::bail!("Please provide a vendor_id or vendor_name.");
        };

        let vendor = match vendor {
            Some(v) => v,
            None => {
                let label = vendor_name_opt
                    .clone()
                    .or_else(|| vendor_id_opt.map(|u| u.to_string()))
                    .unwrap_or_else(|| "specified".to_string());
                return Ok(format!("No vendor found matching '{}'.", label));
            }
        };

        let v_id: Uuid = vendor.try_get("id")?;
        let v_name: String = vendor.try_get("name")?;
        let v_active: bool = vendor.try_get("is_active").unwrap_or(true);
        let v_email: Option<String> = vendor.try_get("contact_email").ok().flatten();
        let v_phone: Option<String> = vendor.try_get("contact_phone").ok().flatten();
        let v_terms: Option<String> = vendor.try_get("payment_terms").ok().flatten();

        // Invoice metrics
        let metrics = sqlx::query(
            r#"
            SELECT
                COUNT(*) AS total_count,
                COUNT(*) FILTER (WHERE status IN ('pending','draft','processing','pending_approval')) AS open_count,
                COALESCE(SUM(total_amount_cents), 0) AS total_cents,
                MAX(invoice_date) AS latest_date,
                MAX(due_date) AS latest_due
            FROM invoices
            WHERE vendor_id = $1 AND tenant_id = $2
            "#,
        )
        .bind(v_id)
        .bind(&context.tenant_id)
        .fetch_one(&self.pool)
        .await?;

        let total_count: i64 = metrics.try_get("total_count").unwrap_or(0);
        let open_count: i64 = metrics.try_get("open_count").unwrap_or(0);
        let total_cents: i64 = metrics.try_get("total_cents").unwrap_or(0);
        let latest_date: Option<NaiveDate> = metrics.try_get("latest_date").ok().flatten();
        let latest_due: Option<NaiveDate> = metrics.try_get("latest_due").ok().flatten();

        // Recent invoices (capped at 5)
        let recent = sqlx::query(
            r#"
            SELECT id, invoice_number, status, total_amount_cents, currency, invoice_date, due_date
            FROM invoices
            WHERE vendor_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            LIMIT 5
            "#,
        )
        .bind(v_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let total_display = format!("{}.{:02}", total_cents / 100, (total_cents % 100).abs());

        let mut lines = vec![
            format!("**Vendor Summary**"),
            format!("ID: {}", v_id),
            format!("Name: {}", v_name),
            format!("Active: {}", if v_active { "Yes" } else { "No" }),
        ];

        if let Some(ref email) = v_email {
            lines.push(format!("Contact Email: {}", email));
        }
        if let Some(ref phone) = v_phone {
            lines.push(format!("Contact Phone: {}", phone));
        }
        if let Some(ref terms) = v_terms {
            lines.push(format!("Payment Terms: {}", terms));
        }

        lines.push(String::new());
        lines.push(format!("Total Invoices: {}", total_count));
        lines.push(format!("Open/Pending Invoices: {}", open_count));
        lines.push(format!("Total Invoiced: {}", total_display));
        lines.push(format!(
            "Latest Invoice Date: {}",
            latest_date
                .map(|d| d.to_string())
                .unwrap_or_else(|| "N/A".to_string())
        ));
        lines.push(format!(
            "Latest Due Date: {}",
            latest_due
                .map(|d| d.to_string())
                .unwrap_or_else(|| "N/A".to_string())
        ));

        if !recent.is_empty() {
            lines.push(String::new());
            lines.push(format!("Recent Invoices (up to 5):"));
            for row in &recent {
                let inv_num: String = row
                    .try_get("invoice_number")
                    .unwrap_or_else(|_| "N/A".to_string());
                let status: String = row.try_get("status").unwrap_or_else(|_| "N/A".to_string());
                let cents: Option<i64> = row.try_get("total_amount_cents").ok().flatten();
                let cur: String = row
                    .try_get("currency")
                    .unwrap_or_else(|_| "USD".to_string());
                let inv_date: Option<NaiveDate> = row.try_get("invoice_date").ok().flatten();
                let due: Option<NaiveDate> = row.try_get("due_date").ok().flatten();
                let amt = cents
                    .map(|c| format!("{}.{:02}", c / 100, (c % 100).abs()))
                    .unwrap_or_else(|| "N/A".to_string());
                lines.push(format!(
                    "  - {} | {} | {} {} | Date: {} | Due: {}",
                    inv_num,
                    status,
                    amt,
                    cur,
                    inv_date
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    due.map(|d| d.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                ));
            }
        }

        Ok(lines.join("\n"))
    }
}

/// Approval requirements query tool - read-only, queries approval_requests and workflow_rules.
pub struct ApprovalRequirementsTool {
    pool: PgPool,
}

impl ApprovalRequirementsTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Parse invoice_id from args: raw UUID text or JSON {"invoice_id":"..."}.
    fn parse_invoice_id(args: &str) -> Result<Uuid> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "Please provide an invoice_id to check approval requirements. \
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

    /// Render `requested_from` JSONB defensively for known serialized shapes.
    fn render_requested_from(val: &serde_json::Value) -> String {
        // Object forms: {"User":{...}}, {"Role":{...}}, etc.
        if let Some(obj) = val.as_object() {
            // Check for typed variants
            if let Some(user) = obj.get("User") {
                return format!("User: {}", Self::describe_user_value(user));
            }
            if let Some(role) = obj.get("Role") {
                return format!("Role: {}", Self::describe_role_value(role));
            }
            if let Some(any) = obj.get("AnyOf") {
                return format!("AnyOf: {}", any);
            }
            if let Some(all) = obj.get("AllOf") {
                return format!("AllOf: {}", all);
            }
            // Legacy / lowercase tolerance
            for key in &["user", "role", "any_of", "all_of", "anyof", "allof"] {
                if let Some(v) = obj.get(*key) {
                    return format!("{}: {}", key, v);
                }
            }
            // Fall back to raw JSON
            return format!("{}", val);
        }
        // String or other scalar
        format!("{}", val)
    }

    fn describe_user_value(val: &serde_json::Value) -> String {
        if let Some(obj) = val.as_object() {
            if let Some(email) = obj.get("email").and_then(|v| v.as_str()) {
                return email.to_string();
            }
            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                return name.to_string();
            }
            if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
                return id.to_string();
            }
        }
        format!("{}", val)
    }

    fn describe_role_value(val: &serde_json::Value) -> String {
        if let Some(obj) = val.as_object() {
            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                return name.to_string();
            }
        }
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        format!("{}", val)
    }
}

#[async_trait]
impl Tool for ApprovalRequirementsTool {
    fn name(&self) -> &str {
        "get_approval_requirements"
    }

    fn description(&self) -> &str {
        "Check approval requirements and current approval request status for an invoice. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_id = Self::parse_invoice_id(args)?;

        // Load tenant-scoped invoice
        let invoice = sqlx::query(
            r#"
            SELECT invoice_number, status, total_amount_cents, currency
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
            None => {
                return Ok(format!(
                    "Invoice {} not found in your organization.",
                    invoice_id
                ))
            }
        };

        let invoice_number: String = invoice
            .try_get("invoice_number")
            .unwrap_or_else(|_| "N/A".to_string());
        let inv_status: String = invoice
            .try_get("status")
            .unwrap_or_else(|_| "N/A".to_string());
        let total_cents: Option<i64> = invoice.try_get("total_amount_cents").ok().flatten();
        let currency: String = invoice
            .try_get("currency")
            .unwrap_or_else(|_| "USD".to_string());

        let amount_display = total_cents
            .map(|c| format!("{}.{:02}", c / 100, (c % 100).abs()))
            .unwrap_or_else(|| "N/A".to_string());

        // Read approval request evidence from approval_requests joined to workflow_rules and users
        let requests = sqlx::query(
            r#"
            SELECT
                ar.id AS ar_id,
                ar.status AS ar_status,
                ar.requested_from,
                ar.comments,
                ar.responded_by,
                ar.responded_at,
                ar.expires_at,
                ar.created_at AS ar_created,
                wr.name AS rule_name,
                wr.priority AS rule_priority,
                u.email AS responder_email
            FROM approval_requests ar
            LEFT JOIN workflow_rules wr ON wr.id = ar.rule_id
            LEFT JOIN users u ON u.id = ar.responded_by
            WHERE ar.invoice_id = $1 AND ar.tenant_id = $2
            ORDER BY ar.created_at DESC
            "#,
        )
        .bind(invoice_id)
        .bind(&context.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        if requests.is_empty() {
            return Ok(format!(
                "No approval requirements are currently recorded for invoice {} ({} | {} {} | Status: {}).",
                invoice_id, invoice_number, amount_display, currency, inv_status
            ));
        }

        let mut lines = vec![
            format!(
                "Approval Requirements for Invoice {} ({})",
                invoice_id, invoice_number
            ),
            format!(
                "Invoice Status: {} | Amount: {} {}",
                inv_status, amount_display, currency
            ),
            String::new(),
            format!("Approval Requests ({}):", requests.len()),
        ];

        for row in &requests {
            let ar_id: Uuid = row.try_get("ar_id")?;
            let ar_status: String = row
                .try_get("ar_status")
                .unwrap_or_else(|_| "Unknown".to_string());
            let req_from: serde_json::Value = row
                .try_get("requested_from")
                .unwrap_or(serde_json::Value::Null);
            let comments: Option<String> = row.try_get("comments").ok().flatten();
            let responder_email: Option<String> = row.try_get("responder_email").ok().flatten();
            let responded_at: Option<DateTime<Utc>> = row.try_get("responded_at").ok().flatten();
            let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at").ok().flatten();
            let rule_name: Option<String> = row.try_get("rule_name").ok().flatten();
            let rule_priority: Option<i32> = row.try_get("rule_priority").ok().flatten();

            lines.push(format!("- Request ID: {}", ar_id,));
            lines.push(format!("  Status: {}", ar_status));
            lines.push(format!(
                "  Requested From: {}",
                Self::render_requested_from(&req_from)
            ));
            if let Some(ref rn) = rule_name {
                lines.push(format!(
                    "  Rule: {}{}",
                    rn,
                    rule_priority
                        .map(|p| format!(" (priority {})", p))
                        .unwrap_or_default()
                ));
            }
            if let Some(ref c) = comments {
                lines.push(format!("  Comments: {}", c));
            }
            if let Some(ref email) = responder_email {
                lines.push(format!("  Responded By: {}", email));
            }
            if let Some(ra) = responded_at {
                lines.push(format!("  Responded At: {}", ra.format("%Y-%m-%d %H:%M")));
            }
            if let Some(ea) = expires_at {
                lines.push(format!("  Expires At: {}", ea.format("%Y-%m-%d %H:%M")));
            }
        }

        Ok(lines.join("\n"))
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
                    invoice_date
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    due_date
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    po_number.unwrap_or_else(|| "N/A".to_string()),
                    created_at.format("%Y-%m-%d"),
                    updated_at
                        .map(|t| t.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
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
            return Ok(
                "Please provide a search query to look up product documentation.".to_string(),
            );
        }

        let snippets = product_knowledge_context_for_query_with_limit(query, 5);

        if snippets.is_empty() {
            return Ok(format!(
                "No product documentation found for query: '{}'. Try different keywords.",
                query
            ));
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
            return Ok(
                "Please provide a feature name or question to get an explanation.".to_string(),
            );
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
        lines.push(
            "Note: This explanation is based solely on indexed product documentation.".to_string(),
        );

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

/// Read-only tool that explains the current workflow state for a single
/// invoice using tenant-scoped backend repositories.
pub struct WorkflowStateExplanationTool {
    pool: PgPool,
}

impl WorkflowStateExplanationTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn parse_invoice_id(args: &str) -> Result<Uuid> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "Please provide an invoice_id to explain workflow state for. Expected: invoice UUID or {{\"invoice_id\":\"<uuid>\"}}."
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

fn format_approval_target(target: &ApprovalTarget) -> String {
    match target {
        ApprovalTarget::User(user) => format!("user {}", user.0),
        ApprovalTarget::Role(role) => format!("role {}", role),
        ApprovalTarget::AnyOf(users) => format!(
            "any of {}",
            users
                .iter()
                .map(|u| u.0.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ApprovalTarget::AllOf(users) => format!(
            "all of {}",
            users
                .iter()
                .map(|u| u.0.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn workflow_state_reason(
    status: ProcessingStatus,
    pending_count: usize,
    rejected_count: usize,
    has_queue: bool,
) -> &'static str {
    if rejected_count > 0 || status == ProcessingStatus::Rejected {
        return "The invoice is blocked because at least one approval was rejected.";
    }
    if pending_count > 0 || status == ProcessingStatus::PendingApproval {
        return "The invoice is waiting on pending approval requests.";
    }
    match status {
        ProcessingStatus::Draft => {
            "The invoice is still in draft and has not entered active processing."
        }
        ProcessingStatus::Submitted => {
            if has_queue {
                "The invoice is in workflow and waiting in its current queue."
            } else {
                "The invoice was submitted but is not currently assigned to a workflow queue."
            }
        }
        ProcessingStatus::Approved => {
            "All required approvals appear complete; the invoice is approved."
        }
        ProcessingStatus::OnHold => {
            "The invoice is on hold and needs clarification before workflow can continue."
        }
        ProcessingStatus::ReadyForPayment => {
            "The invoice is complete from workflow review and is ready for payment."
        }
        ProcessingStatus::Paid => "The invoice workflow is complete and payment has been issued.",
        ProcessingStatus::Voided => "The invoice is voided and outside active workflow processing.",
        ProcessingStatus::PendingApproval | ProcessingStatus::Rejected => unreachable!(),
    }
}

#[async_trait]
impl Tool for WorkflowStateExplanationTool {
    fn name(&self) -> &str {
        "explain_workflow_state"
    }

    fn description(&self) -> &str {
        "Explain an invoice's current workflow state using tenant-scoped invoice, queue, approval, and rule data. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let invoice_uuid = Self::parse_invoice_id(args)?;
        let invoice_id = InvoiceId(invoice_uuid);
        let tenant_id: TenantId = context
            .tenant_id
            .parse()
            .context("Invalid tenant_id in agent context. Expected UUID tenant id.")?;

        let pool = Arc::new(self.pool.clone());
        let invoice_repo = InvoiceRepositoryImpl::new(pool.clone());
        let workflow_repo = WorkflowRepositoryImpl::new(pool);

        let invoice = match invoice_repo.get_by_id(&tenant_id, &invoice_id).await? {
            Some(invoice) => invoice,
            None => {
                return Ok(format!(
                    "Invoice {} not found in your organization.",
                    invoice_uuid
                ))
            }
        };

        let current_item = workflow_repo
            .get_current_item_for_invoice(&tenant_id, &invoice_id)
            .await?;
        let queue = if let Some(item) = current_item.as_ref() {
            WorkQueueRepository::get_by_id(&workflow_repo, &tenant_id, &item.queue_id).await?
        } else {
            None
        };
        let approvals = workflow_repo
            .list_for_invoice(&tenant_id, &invoice_id)
            .await?;

        let pending_count = approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Pending)
            .count();
        let approved_count = approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Approved)
            .count();
        let rejected_count = approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Rejected)
            .count();
        let expired_count = approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Expired)
            .count();
        let cancelled_count = approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Cancelled)
            .count();

        let mut lines = vec![
            format!("Invoice {} ({})", invoice.invoice_number, invoice.id),
            format!("Vendor: {}", invoice.vendor_name),
            format!(
                "Amount: {:.2} {}",
                invoice.total_amount.as_decimal(),
                invoice.currency
            ),
            format!("Processing Status: {}", invoice.processing_status.as_str()),
            format!("Capture Status: {}", invoice.capture_status.as_str()),
        ];

        if let Some(item) = current_item.as_ref() {
            if let Some(queue) = queue.as_ref() {
                lines.push(format!(
                    "Current Queue: {} ({:?})",
                    queue.name, queue.queue_type
                ));
            } else {
                lines.push(format!("Current Queue: {}", item.queue_id));
            }
            lines.push(format!(
                "Queue Assignment: {}",
                item.assigned_to
                    .as_ref()
                    .map(|u| u.0.to_string())
                    .unwrap_or_else(|| "unassigned".to_string())
            ));
            lines.push(format!(
                "Entered Queue: {}",
                item.entered_at.format("%Y-%m-%d %H:%M")
            ));
            lines.push(format!(
                "Due: {}",
                item.due_at
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "not set".to_string())
            ));
        } else {
            lines.push("Current Queue: none".to_string());
        }

        lines.push(format!(
            "Approval Requests: pending={}, approved={}, rejected={}, expired={}, cancelled={}",
            pending_count, approved_count, rejected_count, expired_count, cancelled_count
        ));

        if !approvals.is_empty() {
            lines.push("Approval Breakdown:".to_string());
            for approval in &approvals {
                let rule_name = match WorkflowRuleRepository::get_by_id(
                    &workflow_repo,
                    &tenant_id,
                    &approval.rule_id,
                )
                .await?
                {
                    Some(rule) => rule.name,
                    None => format!("rule {}", approval.rule_id.0),
                };
                lines.push(format!(
                    "- {}: {:?} requested from {}{}{}",
                    rule_name,
                    approval.status,
                    format_approval_target(&approval.requested_from),
                    approval
                        .responded_at
                        .map(|d| format!(", responded {}", d.format("%Y-%m-%d %H:%M")))
                        .unwrap_or_default(),
                    approval
                        .expires_at
                        .map(|d| format!(", expires {}", d.format("%Y-%m-%d %H:%M")))
                        .unwrap_or_default()
                ));
            }
        }

        lines.push(format!(
            "Explanation: {}",
            workflow_state_reason(
                invoice.processing_status,
                pending_count,
                rejected_count,
                current_item.is_some()
            )
        ));

        Ok(lines.join("\n"))
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
            None => {
                return Ok(format!(
                    "Invoice {} not found in your organization.",
                    invoice_id
                ))
            }
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
                let q_name: String = qi
                    .try_get("queue_name")
                    .unwrap_or_else(|_| "Unknown".to_string());
                let q_type: String = qi
                    .try_get("queue_type")
                    .unwrap_or_else(|_| "Unknown".to_string());
                let qi_status: String = qi
                    .try_get("qi_status")
                    .unwrap_or_else(|_| "Unknown".to_string());
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
                "## Approval Requests\nNo approval requests found for this invoice.".to_string(),
            );
        } else {
            let mut lines = vec!["## Approval Requests".to_string()];
            for ar in &approvals {
                let ar_status: String = ar
                    .try_get("ar_status")
                    .unwrap_or_else(|_| "Unknown".to_string());
                let req_from: serde_json::Value = ar
                    .try_get("requested_from")
                    .unwrap_or(serde_json::Value::Null);
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
                let evt: String = row
                    .try_get("event_type")
                    .unwrap_or_else(|_| "?".to_string());
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
                let etype: String = row
                    .try_get("entity_type")
                    .unwrap_or_else(|_| "?".to_string());
                let action: String = row.try_get("action").unwrap_or_else(|_| "?".to_string());
                let atype: String = row
                    .try_get("actor_type")
                    .unwrap_or_else(|_| "?".to_string());
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
                    let rtype: String = row
                        .try_get("resource_type")
                        .unwrap_or_else(|_| "?".to_string());
                    let uid: Option<Uuid> = row.try_get("user_id").ok().flatten();
                    let at: DateTime<Utc> =
                        row.try_get("created_at").unwrap_or_else(|_| Utc::now());
                    audit_lines.push(format!(
                        "  {} on {} by {} at {}",
                        action,
                        rtype,
                        uid.map(|u| u.to_string())
                            .unwrap_or_else(|| "system".to_string()),
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
            serde_json::from_str(trimmed).context("Invalid JSON input for search_invoices.")?
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
            let dt: NaiveDate = db
                .parse()
                .context("Invalid due_before date format. Use YYYY-MM-DD.")?;
            q = q.bind(dt);
        }
        if let Some(da) = due_after {
            let dt: NaiveDate = da
                .parse()
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
            let inv_num: String = row
                .try_get("invoice_number")
                .unwrap_or_else(|_| "N/A".to_string());
            let vn: Option<String> = row.try_get("vendor_name").ok().flatten();
            let st: String = row.try_get("status").unwrap_or_else(|_| "N/A".to_string());
            let cents: Option<i64> = row.try_get("total_amount_cents").ok().flatten();
            let cur: String = row
                .try_get("currency")
                .unwrap_or_else(|_| "USD".to_string());
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
                dd.map(|d| d.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
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
            None => {
                return Ok(format!(
                    "Invoice {} not found in your organization.",
                    invoice_id
                ))
            }
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
                                reasons.push(format!(
                                    "same vendor + invoice_date within {} day(s)",
                                    diff
                                ));
                            }
                        }
                        if let (Some(td), Some(cd)) = (t_due_date, c_due_date) {
                            let diff = (td - cd).num_days().abs();
                            if diff <= 3 {
                                reasons
                                    .push(format!("same vendor + due_date within {} day(s)", diff));
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
                c_inv_date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                c_due_date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
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
            None => {
                return Ok(format!(
                    "Invoice {} not found in your organization.",
                    invoice_id
                ))
            }
        };

        let inv_num: Option<String> = invoice.try_get("invoice_number").ok().flatten();
        let vendor: Option<String> = invoice.try_get("vendor_name").ok().flatten();
        let amount_cents: Option<i64> = invoice.try_get("total_amount_cents").ok().flatten();
        let currency: String = invoice
            .try_get("currency")
            .unwrap_or_else(|_| "USD".to_string());
        let status: String = invoice
            .try_get("status")
            .unwrap_or_else(|_| "N/A".to_string());
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
                    risk_signals.push(format!(
                        "Invoice is {} day(s) overdue (due: {}).",
                        days_overdue, dd
                    ));
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
                risk_signals.push(format!(
                    "High-value invoice: {}.{:02} {}.",
                    cents / 100,
                    cents % 100,
                    currency
                ));
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
                    evidence.push(format!(
                        "payment_requests found: {} (all resolved)",
                        rows.len()
                    ));
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
            format!(
                "Invoice: {} | Vendor: {} | Amount: {} {}",
                inv_num.unwrap_or_else(|| "N/A".to_string()),
                vendor.unwrap_or_else(|| "Unknown".to_string()),
                amt_display,
                currency
            ),
            format!(
                "Status: {} | Processing: {}",
                status,
                proc_status.unwrap_or_else(|| "N/A".to_string())
            ),
            format!(
                "Due Date: {}",
                due_date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            ),
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

// ── Admin analysis tools ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct AdminAnalysisArgs {
    window_days: i32,
    limit: i64,
}

impl AdminAnalysisArgs {
    fn parse(args: &str) -> Result<Self> {
        let trimmed = args.trim();
        let mut parsed = Self {
            window_days: 30,
            limit: 5,
        };

        if trimmed.is_empty() {
            return Ok(parsed);
        }

        let val: serde_json::Value =
            serde_json::from_str(trimmed).context("Invalid JSON for admin analysis tool args")?;

        if let Some(days) = val.get("window_days").and_then(|v| v.as_i64()) {
            parsed.window_days = days.clamp(1, 365) as i32;
        }
        if let Some(limit) = val.get("limit").and_then(|v| v.as_i64()) {
            parsed.limit = limit.clamp(1, 25);
        }

        Ok(parsed)
    }
}

fn ensure_admin_context(context: &AgentContext) -> Result<()> {
    let role = context.user_role.to_ascii_lowercase();
    let is_admin_role = matches!(
        role.as_str(),
        "admin" | "tenant_admin" | "owner" | "super_admin"
    );
    let has_admin_permission = context.permissions.iter().any(|p| {
        let normalized = p.to_ascii_lowercase();
        matches!(
            normalized.as_str(),
            "settings:write" | "tenant_admin" | "admin" | "owner"
        )
    });

    if is_admin_role || has_admin_permission {
        Ok(())
    } else {
        anyhow::bail!(
            "Forbidden: this admin-only analysis tool requires tenant administrator access"
        )
    }
}

fn cents_to_currency(cents: i64) -> String {
    format!("${:.2}", cents as f64 / 100.0)
}

pub struct TenantUsageAnalysisTool {
    pool: PgPool,
}

impl TenantUsageAnalysisTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for TenantUsageAnalysisTool {
    fn name(&self) -> &str {
        "get_tenant_usage_analysis"
    }

    fn description(&self) -> &str {
        "Admin-only tenant usage analysis. Args: optional JSON {\"window_days\":30}"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        ensure_admin_context(context)?;
        let args = AdminAnalysisArgs::parse(args)?;

        let row = sqlx::query(
            r#"
            SELECT
                (SELECT COUNT(*) FROM invoices WHERE tenant_id::text = $1 AND created_at >= NOW() - make_interval(days => $2::int)) AS invoice_count,
                (SELECT COUNT(*) FROM vendors WHERE tenant_id::text = $1) AS vendor_count,
                (SELECT COUNT(*) FROM users WHERE tenant_id::text = $1) AS user_count,
                (SELECT COALESCE(SUM(size_bytes), 0)::BIGINT FROM documents WHERE tenant_id::text = $1) AS document_storage_bytes
            "#,
        )
        .bind(&context.tenant_id)
        .bind(args.window_days)
        .fetch_one(&self.pool)
        .await?;

        let invoice_count: i64 = row.try_get("invoice_count")?;
        let vendor_count: i64 = row.try_get("vendor_count")?;
        let user_count: i64 = row.try_get("user_count")?;
        let storage_bytes: i64 = row.try_get("document_storage_bytes")?;

        Ok(format!(
            "Tenant usage analysis (last {} day(s)):\n- Invoices created: {}\n- Vendors: {}\n- Users: {}\n- Document storage: {} bytes\nThis is read-only and tenant-scoped.",
            args.window_days, invoice_count, vendor_count, user_count, storage_bytes
        ))
    }
}

pub struct WorkflowBottlenecksTool {
    pool: PgPool,
}

impl WorkflowBottlenecksTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for WorkflowBottlenecksTool {
    fn name(&self) -> &str {
        "get_workflow_bottlenecks"
    }

    fn description(&self) -> &str {
        "Admin-only workflow bottleneck analysis. Args: optional JSON {\"window_days\":30,\"limit\":5}"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        ensure_admin_context(context)?;
        let args = AdminAnalysisArgs::parse(args)?;

        let approval_rows = sqlx::query(
            r#"
            SELECT status, COUNT(*) AS count, COALESCE(MAX(EXTRACT(EPOCH FROM (NOW() - created_at)) / 86400), 0)::DOUBLE PRECISION AS oldest_days
            FROM approval_requests
            WHERE tenant_id::text = $1
              AND created_at >= NOW() - make_interval(days => $2::int)
              AND status IN ('pending', 'requested')
            GROUP BY status
            ORDER BY count DESC
            LIMIT $3
            "#,
        )
        .bind(&context.tenant_id)
        .bind(args.window_days)
        .bind(args.limit)
        .fetch_all(&self.pool)
        .await?;

        let queue_rows = sqlx::query(
            r#"
            SELECT wq.name, COUNT(qi.id) AS backlog, COALESCE(MAX(EXTRACT(EPOCH FROM (NOW() - qi.entered_at)) / 86400), 0)::DOUBLE PRECISION AS oldest_days
            FROM queue_items qi
            JOIN work_queues wq ON wq.id = qi.queue_id
            WHERE qi.tenant_id::text = $1
              AND qi.status IN ('pending', 'in_progress', 'claimed')
            GROUP BY wq.name
            ORDER BY backlog DESC, oldest_days DESC
            LIMIT $2
            "#,
        )
        .bind(&context.tenant_id)
        .bind(args.limit)
        .fetch_all(&self.pool)
        .await?;

        let mut lines = vec![format!(
            "Workflow bottlenecks (last {} day(s), top {}):",
            args.window_days, args.limit
        )];

        if approval_rows.is_empty() {
            lines.push("- Pending approvals: none found".to_string());
        } else {
            lines.push("- Pending approvals:".to_string());
            for row in approval_rows {
                let status: String = row.try_get("status")?;
                let count: i64 = row.try_get("count")?;
                let oldest_days: f64 = row.try_get("oldest_days")?;
                lines.push(format!(
                    "  - {}: {} request(s), oldest {:.1} day(s)",
                    status, count, oldest_days
                ));
            }
        }

        if queue_rows.is_empty() {
            lines.push("- Queue backlog: none found".to_string());
        } else {
            lines.push("- Queue backlog:".to_string());
            for row in queue_rows {
                let queue_name: String = row.try_get("name")?;
                let backlog: i64 = row.try_get("backlog")?;
                let oldest_days: f64 = row.try_get("oldest_days")?;
                lines.push(format!(
                    "  - {}: {} item(s), oldest {:.1} day(s)",
                    queue_name, backlog, oldest_days
                ));
            }
        }

        lines.push("This is read-only and tenant-scoped.".to_string());
        Ok(lines.join("\n"))
    }
}

pub struct RuleRecommendationsTool {
    pool: PgPool,
}

impl RuleRecommendationsTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for RuleRecommendationsTool {
    fn name(&self) -> &str {
        "get_rule_recommendations"
    }

    fn description(&self) -> &str {
        "Admin-only read-only workflow rule recommendations. Args: optional JSON {\"window_days\":30,\"limit\":5}"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        ensure_admin_context(context)?;
        let args = AdminAnalysisArgs::parse(args)?;

        let row = sqlx::query(
            r#"
            SELECT
                (SELECT COUNT(*) FROM workflow_rules WHERE tenant_id::text = $1 AND is_active = true) AS active_rules,
                (SELECT COUNT(*) FROM invoices WHERE tenant_id::text = $1 AND created_at >= NOW() - make_interval(days => $2::int) AND assigned_to IS NULL) AS unassigned_invoices,
                (SELECT COUNT(*) FROM invoices WHERE tenant_id::text = $1 AND created_at >= NOW() - make_interval(days => $2::int) AND processing_status IN ('draft', 'needs_review', 'pending')) AS unclassified_invoices,
                (SELECT COUNT(*) FROM approval_requests WHERE tenant_id::text = $1 AND status = 'pending' AND created_at < NOW() - INTERVAL '7 days') AS stale_approvals
            "#,
        )
        .bind(&context.tenant_id)
        .bind(args.window_days)
        .fetch_one(&self.pool)
        .await?;

        let active_rules: i64 = row.try_get("active_rules")?;
        let unassigned_invoices: i64 = row.try_get("unassigned_invoices")?;
        let unclassified_invoices: i64 = row.try_get("unclassified_invoices")?;
        let stale_approvals: i64 = row.try_get("stale_approvals")?;

        let mut recommendations = Vec::new();
        if active_rules == 0 {
            recommendations.push(
                "No active workflow rules found; review whether standard approval/routing rules should be configured.".to_string(),
            );
        }
        if stale_approvals > 0 {
            recommendations.push(format!(
                "{} pending approval(s) are older than 7 days; consider escalation or reminder rules.",
                stale_approvals
            ));
        }
        if unassigned_invoices > 0 {
            recommendations.push(format!(
                "{} recent invoice(s) are unassigned; consider assignment rules for intake queues.",
                unassigned_invoices
            ));
        }
        if unclassified_invoices > 0 {
            recommendations.push(format!(
                "{} recent invoice(s) are still draft/needs-review/pending; consider validation or auto-routing rules.",
                unclassified_invoices
            ));
        }
        if recommendations.is_empty() {
            recommendations.push(
                "No obvious workflow-rule recommendations from the current read-only heuristics."
                    .to_string(),
            );
        }

        let mut lines = vec![format!(
            "Workflow rule recommendations (last {} day(s)):",
            args.window_days
        )];
        for recommendation in recommendations.into_iter().take(args.limit as usize) {
            lines.push(format!("- {}", recommendation));
        }
        lines.push("This tool does not create or mutate workflow rules.".to_string());
        Ok(lines.join("\n"))
    }
}

pub struct SpendAnalysisTool {
    pool: PgPool,
}

impl SpendAnalysisTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for SpendAnalysisTool {
    fn name(&self) -> &str {
        "get_spend_analysis"
    }

    fn description(&self) -> &str {
        "Admin-only tenant spend analysis. Args: optional JSON {\"window_days\":30,\"limit\":5}"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        ensure_admin_context(context)?;
        let args = AdminAnalysisArgs::parse(args)?;

        let summary = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (
                    WHERE created_at >= NOW() - make_interval(days => $1::int)
                ) AS invoice_count,
                COALESCE(SUM(total_amount_cents) FILTER (
                    WHERE created_at >= NOW() - make_interval(days => $1::int)
                ), 0)::BIGINT AS total_cents,
                COALESCE(AVG(total_amount_cents) FILTER (
                    WHERE created_at >= NOW() - make_interval(days => $1::int)
                ), 0)::DOUBLE PRECISION AS average_cents,
                COALESCE(SUM(total_amount_cents) FILTER (
                    WHERE created_at >= NOW() - make_interval(days => ($1::int * 2))
                      AND created_at < NOW() - make_interval(days => $1::int)
                ), 0)::BIGINT AS prior_total_cents
            FROM invoices
            WHERE tenant_id::text = $2
              AND created_at >= NOW() - make_interval(days => ($1::int * 2))
            "#,
        )
        .bind(args.window_days)
        .bind(&context.tenant_id)
        .fetch_one(&self.pool)
        .await?;

        let top_vendors = sqlx::query(
            r#"
            SELECT COALESCE(vendor_name, 'Unknown') AS vendor_name, COUNT(*) AS invoice_count, COALESCE(SUM(total_amount_cents), 0)::BIGINT AS total_cents
            FROM invoices
            WHERE tenant_id::text = $1
              AND created_at >= NOW() - make_interval(days => $2::int)
            GROUP BY COALESCE(vendor_name, 'Unknown')
            ORDER BY total_cents DESC
            LIMIT $3
            "#,
        )
        .bind(&context.tenant_id)
        .bind(args.window_days)
        .bind(args.limit)
        .fetch_all(&self.pool)
        .await?;

        let invoice_count: i64 = summary.try_get("invoice_count")?;
        let total_cents: i64 = summary.try_get("total_cents")?;
        let average_cents: f64 = summary.try_get("average_cents")?;
        let prior_total_cents: i64 = summary.try_get("prior_total_cents")?;
        let delta_cents = total_cents - prior_total_cents;

        let mut lines = vec![
            format!("Spend analysis (last {} day(s)):", args.window_days),
            format!("- Invoice count: {}", invoice_count),
            format!("- Total spend: {}", cents_to_currency(total_cents)),
            format!(
                "- Average invoice amount: {}",
                cents_to_currency(average_cents.round() as i64)
            ),
            format!(
                "- Prior-window comparison: {} ({:+})",
                cents_to_currency(prior_total_cents),
                cents_to_currency(delta_cents)
            ),
        ];

        if top_vendors.is_empty() {
            lines.push("- Top vendors: none found".to_string());
        } else {
            lines.push("- Top vendors:".to_string());
            for row in top_vendors {
                let vendor_name: String = row.try_get("vendor_name")?;
                let count: i64 = row.try_get("invoice_count")?;
                let cents: i64 = row.try_get("total_cents")?;
                lines.push(format!(
                    "  - {}: {} across {} invoice(s)",
                    vendor_name,
                    cents_to_currency(cents),
                    count
                ));
            }
        }

        lines.push("This is read-only and tenant-scoped.".to_string());
        Ok(lines.join("\n"))
    }
}

// ── Respond-to-approval-request tool (first mutating AP-action tool) ─────────

/// Arguments for responding to an approval request.
#[derive(serde::Deserialize)]
struct RespondToApprovalRequestArgs {
    approval_request_id: Uuid,
    decision: String,
    comments: Option<String>,
}

/// Tool that approves or rejects a pending approval request on an invoice.
pub struct RespondToApprovalRequestTool {
    pool: PgPool,
}

impl RespondToApprovalRequestTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for RespondToApprovalRequestTool {
    fn name(&self) -> &str {
        "respond_to_approval_request"
    }

    fn description(&self) -> &str {
        "Approve or reject a pending approval request on an invoice. \
         Args: JSON { \"approval_request_id\": \"<uuid>\", \"decision\": \"approve\"|\"reject\", \"comments\": \"...optional...\" }"
    }

    async fn execute(&self, context: &AgentContext, args: &str) -> Result<String> {
        let parsed: RespondToApprovalRequestArgs = serde_json::from_str(args.trim()).context(
            "Invalid JSON for respond_to_approval_request. \
             Expected: {\"approval_request_id\":\"<uuid>\",\"decision\":\"approve\"|\"reject\",\"comments\":\"...\"}",
        )?;

        let approval_status = match parsed.decision.to_lowercase().as_str() {
            "approve" => ApprovalStatus::Approved,
            "reject" => ApprovalStatus::Rejected,
            other => {
                anyhow::bail!(
                    "Unknown decision '{}'. Must be 'approve' or 'reject'.",
                    other
                )
            }
        };

        let tenant_id: TenantId = context
            .tenant_id
            .parse()
            .context("Invalid tenant_id in agent context. Expected UUID tenant id.")?;
        let user_id = billforge_core::UserId::from_uuid(context.user_id);

        let pool = Arc::new(self.pool.clone());
        let approval_repo = WorkflowRepositoryImpl::new(pool);

        // Verify the request exists and belongs to this tenant
        let existing =
            ApprovalRepository::get_by_id(&approval_repo, &tenant_id, parsed.approval_request_id)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Approval request {} not found in your organization.",
                        parsed.approval_request_id
                    )
                })?;

        if existing.status != ApprovalStatus::Pending {
            anyhow::bail!(
                "Approval request {} is already {}, not Pending. Only pending requests can be responded to.",
                parsed.approval_request_id,
                match existing.status {
                    ApprovalStatus::Approved => "Approved",
                    ApprovalStatus::Rejected => "Rejected",
                    ApprovalStatus::Expired => "Expired",
                    ApprovalStatus::Cancelled => "Cancelled",
                    ApprovalStatus::Pending => unreachable!(),
                }
            );
        }

        let updated = ApprovalRepository::respond(
            &approval_repo,
            &tenant_id,
            parsed.approval_request_id,
            approval_status,
            parsed.comments,
            &user_id,
        )
        .await?;

        let status_str = match updated.status {
            ApprovalStatus::Approved => "Approved",
            ApprovalStatus::Rejected => "Rejected",
            _ => unreachable!(),
        };

        Ok(format!(
            "Approval request {} has been {}. Invoice: {}.",
            parsed.approval_request_id, status_str, updated.invoice_id.0
        ))
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
        let definitions = vec![
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
                description: "Check approval requirements and current approval request status for an invoice. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})",
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
                        "approval_requests": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "request_id": { "type": "string", "format": "uuid" },
                                    "status": { "type": "string" },
                                    "requested_from": { "type": "string" },
                                    "rule_name": { "type": "string" },
                                    "rule_priority": { "type": "integer" },
                                    "comments": { "type": "string" },
                                    "responded_by": { "type": "string" },
                                    "responded_at": { "type": "string" },
                                    "expires_at": { "type": "string" }
                                }
                            }
                        },
                        "invoice_status": { "type": "string" },
                        "invoice_amount": { "type": "string" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_vendor_summary",
                description: "Get a vendor summary including contact info, payment terms, and invoice metrics. Args: vendor_id or vendor_name (UUID, name, or JSON with vendor_id/vendor_name)",
                class: AiToolClass::Vendor,
                required_permission: AiToolPermission::VendorRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "vendor_id": { "type": "string", "format": "uuid" },
                        "vendor_name": { "type": "string" }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "vendor_id": { "type": "string", "format": "uuid" },
                        "vendor_name": { "type": "string" },
                        "is_active": { "type": "boolean" },
                        "contact_email": { "type": "string" },
                        "contact_phone": { "type": "string" },
                        "payment_terms": { "type": "string" },
                        "total_invoices": { "type": "integer" },
                        "open_pending_invoices": { "type": "integer" },
                        "total_invoiced": { "type": "string" },
                        "latest_invoice_date": { "type": "string" },
                        "latest_due_date": { "type": "string" },
                        "recent_invoices": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "invoice_number": { "type": "string" },
                                    "status": { "type": "string" },
                                    "amount": { "type": "string" },
                                    "invoice_date": { "type": "string" },
                                    "due_date": { "type": "string" }
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
                name: "explain_workflow_state",
                description: "Explain an invoice's current workflow state using tenant-scoped backend services. Args: invoice_id (UUID or JSON {\"invoice_id\":\"<uuid>\"})",
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
                        "invoice": { "type": "object" },
                        "current_queue": { "type": "object" },
                        "approval_breakdown": { "type": "object" },
                        "explanation": { "type": "string" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_tenant_usage_analysis",
                description: "Admin-only read-only tenant usage analysis. Args: optional JSON {\"window_days\":30}. Tenant scope comes from authenticated context; do not pass tenant_id.",
                class: AiToolClass::AdminAnalysis,
                required_permission: AiToolPermission::AdminAnalyticsRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_days": { "type": "integer", "minimum": 1, "maximum": 365 }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_count": { "type": "integer" },
                        "vendor_count": { "type": "integer" },
                        "user_count": { "type": "integer" },
                        "document_storage_bytes": { "type": "integer" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_workflow_bottlenecks",
                description: "Admin-only read-only workflow bottleneck analysis. Args: optional JSON {\"window_days\":30,\"limit\":5}. Tenant scope comes from authenticated context; do not pass tenant_id.",
                class: AiToolClass::AdminAnalysis,
                required_permission: AiToolPermission::AdminAnalyticsRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_days": { "type": "integer", "minimum": 1, "maximum": 365 },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 25 }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pending_approvals": { "type": "array" },
                        "queue_backlog": { "type": "array" }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_rule_recommendations",
                description: "Admin-only read-only workflow rule recommendations. Args: optional JSON {\"window_days\":30,\"limit\":5}. Tenant scope comes from authenticated context; do not pass tenant_id.",
                class: AiToolClass::AdminAnalysis,
                required_permission: AiToolPermission::AdminAnalyticsRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_days": { "type": "integer", "minimum": 1, "maximum": 365 },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 25 }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "recommendations": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }),
                mutates: false,
            },
            AiToolDefinition {
                name: "get_spend_analysis",
                description: "Admin-only read-only tenant spend analysis. Args: optional JSON {\"window_days\":30,\"limit\":5}. Tenant scope comes from authenticated context; do not pass tenant_id.",
                class: AiToolClass::AdminAnalysis,
                required_permission: AiToolPermission::AdminAnalyticsRead,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_days": { "type": "integer", "minimum": 1, "maximum": 365 },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 25 }
                    }
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "invoice_count": { "type": "integer" },
                        "total_spend": { "type": "string" },
                        "average_invoice_amount": { "type": "string" },
                        "top_vendors": { "type": "array" }
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
            AiToolDefinition {
                name: "respond_to_approval_request",
                description: "Approve or reject a pending approval request on an invoice. Args: JSON { \"approval_request_id\": \"<uuid>\", \"decision\": \"approve\"|\"reject\", \"comments\": \"...optional...\" }",
                class: AiToolClass::Approval,
                required_permission: AiToolPermission::ApprovalRespond,
                risk_level: AiToolRiskLevel::High,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "approval_request_id": { "type": "string", "format": "uuid" },
                        "decision": { "type": "string", "enum": ["approve", "reject"] },
                        "comments": { "type": "string" }
                    },
                    "required": ["approval_request_id", "decision"]
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["Approved", "Rejected"] },
                        "approval_request_id": { "type": "string", "format": "uuid" },
                        "invoice_id": { "type": "string", "format": "uuid" },
                        "message": { "type": "string" }
                    }
                }),
                mutates: true,
            },
        ];

        #[cfg(test)]
        {
            let mut definitions = definitions;
            definitions.push(AiToolDefinition {
                name: "synthetic_mutating_test_tool",
                description: "Test-only mutating tool used to verify execution guards.",
                class: AiToolClass::IssueIntake,
                required_permission: AiToolPermission::IssueRequest,
                risk_level: AiToolRiskLevel::Low,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                output_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                mutates: true,
            });
            definitions
        }

        #[cfg(not(test))]
        {
            definitions
        }
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

    pub fn provider_tool_definitions(&self) -> Vec<super::models::ProviderToolDefinition> {
        Self::tool_definitions()
            .into_iter()
            .map(|d| super::models::ProviderToolDefinition {
                name: d.name.to_string(),
                description: Some(d.description.to_string()),
                parameters: d.input_schema,
            })
            .collect()
    }

    /// Parse effective `Role` values from the `user_role` and `permissions`
    /// fields of an [`AgentContext`]. Strings that do not map to a known role
    /// variant are silently skipped so that non-role permission strings (e.g.
    /// `"read"`) do not cause errors.
    fn parse_effective_roles(context: &AgentContext) -> Vec<Role> {
        fn parse_role_name(name: &str) -> Option<Role> {
            match name.to_ascii_lowercase().as_str() {
                "tenant_admin" | "admin" | "owner" | "super_admin" => Some(Role::TenantAdmin),
                "ap_user" => Some(Role::ApUser),
                "approver" => Some(Role::Approver),
                "vendor_manager" => Some(Role::VendorManager),
                "report_viewer" => Some(Role::ReportViewer),
                _ => None,
            }
        }

        let mut roles = Vec::new();
        if let Some(role) = parse_role_name(&context.user_role) {
            roles.push(role);
        }
        for perm in &context.permissions {
            if let Some(role) = parse_role_name(perm) {
                if !roles.contains(&role) {
                    roles.push(role);
                }
            }
        }
        roles
    }

    /// Build a lightweight [`UserContext`] from an [`AgentContext`] so the
    /// shared permission helper in `proposals.rs` can evaluate roles.
    fn build_user_context_from_agent(context: &AgentContext) -> UserContext {
        let roles = Self::parse_effective_roles(context);
        let tenant_id = TenantId(
            Uuid::parse_str(&context.tenant_id).unwrap_or(Uuid::nil()),
        );
        UserContext {
            user_id: UserId(context.user_id),
            tenant_id,
            email: String::new(),
            name: String::new(),
            roles,
        }
    }

    /// Central execution guard for every tool dispatch path.
    ///
    /// 1. Resolves the caller's roles from `context` and checks them against
    ///    `def.required_permission` using the same helper the proposal path
    ///    uses (`user_roles_grant_tool_permission`).
    /// 2. For mutating / high-risk tools, additionally requires an approved
    ///    [`ToolProposalContext`].
    pub fn validate_tool_execution_guard(
        def: &AiToolDefinition,
        context: &AgentContext,
        proposal_context: Option<&ToolProposalContext>,
    ) -> Result<()> {
        // Every tool requires its declared permission — even read-only / low-risk ones.
        let user = Self::build_user_context_from_agent(context);
        if !super::proposals::user_roles_grant_tool_permission(&user, def.required_permission) {
            anyhow::bail!(
                "Permission denied: caller does not have the '{}' permission required for tool '{}'",
                format!("{:?}", def.required_permission),
                def.name
            );
        }

        // Mutating or high-risk tools additionally require an approved proposal.
        if !def.mutates && def.risk_level != AiToolRiskLevel::High {
            return Ok(());
        }

        match proposal_context {
            Some(ctx) if ctx.approved && ctx.tool_name == def.name => Ok(()),
            _ => anyhow::bail!(
                "Tool '{}' requires an approved proposal context before execution",
                def.name
            ),
        }
    }

    pub async fn execute_tool(
        &self,
        tool_name: &str,
        context: &AgentContext,
        args: &str,
    ) -> Result<String> {
        self.execute_tool_with_proposal_context(tool_name, context, args, None)
            .await
    }

    pub async fn execute_tool_with_proposal_context(
        &self,
        tool_name: &str,
        context: &AgentContext,
        args: &str,
        proposal_context: Option<&ToolProposalContext>,
    ) -> Result<String> {
        // Reject unknown tools early via the typed registry.
        let def = Self::get_tool_definition(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", tool_name))?;
        Self::validate_tool_execution_guard(&def, context, proposal_context)?;

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
            "get_module_capabilities" => ModuleCapabilitiesTool.execute(context, args).await,
            "search_product_docs" => SearchProductDocsTool.execute(context, args).await,
            "explain_feature" => ExplainFeatureTool.execute(context, args).await,
            "search_known_issues" => SearchKnownIssuesTool.execute(context, args).await,
            "summarize_release_changes" => SummarizeReleaseChangesTool.execute(context, args).await,
            "explain_workflow_behavior" => {
                ExplainWorkflowBehaviorTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "explain_workflow_state" => {
                WorkflowStateExplanationTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_tenant_usage_analysis" => {
                TenantUsageAnalysisTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_workflow_bottlenecks" => {
                WorkflowBottlenecksTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_rule_recommendations" => {
                RuleRecommendationsTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_spend_analysis" => {
                SpendAnalysisTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
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
                SearchInvoicesTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "find_duplicate_invoice_candidates" => {
                FindDuplicateInvoiceCandidatesTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "assess_invoice_payment_risk" => {
                AssessInvoicePaymentRiskTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "get_vendor_summary" => {
                VendorSummaryTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            "respond_to_approval_request" => {
                RespondToApprovalRequestTool::new(self.pool.clone())
                    .execute(context, args)
                    .await
            }
            _ => anyhow::bail!("Tool '{}' not found", tool_name),
        }
    }
}

#[cfg(test)]
mod permission_guard_tests {
    use super::*;
    use crate::models::AgentContext;

    /// Helper: build a minimal `AgentContext` with the given role string and
    /// permission strings.
    fn agent_context(user_role: &str, permissions: Vec<&str>) -> AgentContext {
        AgentContext {
            tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
            user_id: Uuid::new_v4(),
            user_role: user_role.to_string(),
            permissions: permissions.into_iter().map(|s| s.to_string()).collect(),
            enabled_modules: vec![],
        }
    }

    /// Helper: build a read-only tool definition that requires admin-level
    /// analytics permission.
    fn admin_analytics_read_tool() -> AiToolDefinition {
        AiToolDefinition {
            name: "test_admin_analytics_read",
            description: "Test-only admin analytics read tool.",
            class: AiToolClass::AdminAnalysis,
            required_permission: AiToolPermission::AdminAnalyticsRead,
            risk_level: AiToolRiskLevel::Low,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            output_schema: serde_json::json!({"type": "object", "properties": {}}),
            mutates: false,
        }
    }

    /// Helper: build a mutating tool definition (matches the existing
    /// synthetic test tool pattern).
    fn mutating_test_tool() -> AiToolDefinition {
        AiToolDefinition {
            name: "test_mutating_tool",
            description: "Test-only mutating tool.",
            class: AiToolClass::IssueIntake,
            required_permission: AiToolPermission::IssueRequest,
            risk_level: AiToolRiskLevel::Low,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            output_schema: serde_json::json!({"type": "object", "properties": {}}),
            mutates: true,
        }
    }

    #[test]
    fn read_tool_with_insufficient_role_is_rejected() {
        let def = admin_analytics_read_tool();
        // report_viewer does NOT grant AdminAnalyticsRead
        let context = agent_context("report_viewer", vec!["read"]);

        let result = ToolRegistry::validate_tool_execution_guard(&def, &context, None);
        assert!(
            result.is_err(),
            "Expected permission-denied error for non-admin caller, got Ok"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Permission denied"),
            "Error should mention 'Permission denied', got: {err_msg}"
        );
    }

    #[test]
    fn read_tool_with_sufficient_role_passes() {
        let def = admin_analytics_read_tool();
        // TenantAdmin grants every permission including AdminAnalyticsRead
        let context = agent_context("tenant_admin", vec!["read", "write"]);

        let result = ToolRegistry::validate_tool_execution_guard(&def, &context, None);
        assert!(
            result.is_ok(),
            "TenantAdmin caller should pass the permission guard for a read tool"
        );
    }

    #[test]
    fn mutating_tool_still_requires_approved_proposal() {
        let def = mutating_test_tool();
        // ApUser grants IssueRequest permission
        let context = agent_context("ap_user", vec!["read"]);

        // Without a proposal context → should fail (proposal requirement, not permission)
        let result = ToolRegistry::validate_tool_execution_guard(&def, &context, None);
        assert!(
            result.is_err(),
            "Mutating tool without proposal context should be rejected"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("approved proposal context"),
            "Error should mention 'approved proposal context', got: {err_msg}"
        );

        // With an approved proposal context → should succeed
        let proposal = ToolProposalContext {
            proposal_id: Uuid::new_v4(),
            tool_name: "test_mutating_tool".to_string(),
            approved: true,
        };
        let result =
            ToolRegistry::validate_tool_execution_guard(&def, &context, Some(&proposal));
        assert!(
            result.is_ok(),
            "Mutating tool with approved proposal context should pass"
        );
    }
}
