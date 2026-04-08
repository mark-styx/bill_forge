//! OpenAPI documentation for the BillForge API

use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// OpenAPI documentation for BillForge API
#[derive(OpenApi)]
#[openapi(
    info(
        title = "BillForge API",
        version = "1.0.0",
        description = "BillForge - Modern Invoice Processing Platform API

## Overview

BillForge is a comprehensive accounts payable automation platform that helps businesses:
- Capture and digitize invoices with OCR
- Route invoices through customizable approval workflows
- Manage vendor relationships
- Generate financial reports and analytics

## Authentication

All API endpoints (except /health) require authentication using JWT Bearer tokens.

1. Obtain tokens via POST /api/v1/auth/login
2. Include the access token in the Authorization header: `Authorization: Bearer <token>`
3. Refresh tokens before expiry using POST /api/v1/auth/refresh

## Multi-Tenancy

BillForge is a multi-tenant system. Include tenant_id in authentication requests.
",
        contact(
            name = "BillForge Support",
            email = "support@billforge.app"
        ),
        license(
            name = "Proprietary"
        )
    ),
    servers(
        (url = "/api/v1", description = "API Server")
    ),
    tags(
        (name = "Authentication", description = "User authentication and token management"),
        (name = "Invoices", description = "Invoice capture and management"),
        (name = "Vendors", description = "Vendor management"),
        (name = "Workflows", description = "Approval workflows, queues, and rules"),
        (name = "Reports", description = "Financial reporting and analytics"),
        (name = "Health", description = "System health and monitoring"),
        (name = "Dashboard", description = "Dashboard metrics and KPIs"),
        (name = "QuickBooks", description = "QuickBooks Online integration"),
        (name = "Xero", description = "Xero accounting integration"),
        (name = "Sage Intacct", description = "Sage Intacct ERP integration"),
        (name = "Salesforce", description = "Salesforce CRM integration"),
        (name = "Workday", description = "Workday Financial Management integration"),
        (name = "Bill.com", description = "Bill.com AP payments integration"),
        (name = "EDI", description = "Electronic Data Interchange"),
        (name = "Purchase Orders", description = "Purchase order management and 3-way matching"),
        (name = "Payment Requests", description = "Payment request batching and submission"),
        (name = "Vendor Statements", description = "Vendor statement reconciliation"),
        (name = "Export", description = "Data export (CSV, JSON)"),
        (name = "Documents", description = "Document upload and storage"),
        (name = "Audit", description = "Audit trail and compliance logs"),
        (name = "Sandbox", description = "Sandbox persona management"),
        (name = "Notifications", description = "Slack and Teams notification integrations"),
        (name = "Predictive Analytics", description = "Forecasting and anomaly detection"),
        (name = "Mobile", description = "Mobile app backend and sync"),
        (name = "Settings", description = "Tenant configuration"),
        (name = "Feedback", description = "User feedback"),
        (name = "Theme", description = "Organization and user theme customization"),
        (name = "Email Actions", description = "Email-based invoice actions"),
        (name = "AI Assistant", description = "Winston AI assistant"),
        (name = "Billing", description = "Billing plans and subscriptions"),
        (name = "Routing", description = "Intelligent routing and workload balancing"),
    ),
    paths(
        // Authentication endpoints
        crate::routes::auth::login,
        crate::routes::auth::register,
        crate::routes::auth::provision,
        crate::routes::auth::refresh,
        crate::routes::auth::logout,
        crate::routes::auth::me,
        // Invoice endpoints
        crate::routes::invoices::list_invoices,
        crate::routes::invoices::create_invoice,
        crate::routes::invoices::upload_invoice,
        crate::routes::invoices::get_invoice,
        crate::routes::invoices::update_invoice,
        crate::routes::invoices::delete_invoice,
        crate::routes::invoices::rerun_ocr,
        crate::routes::invoices::submit_for_processing,
        crate::routes::invoices::suggest_categories,
        // Dashboard endpoints
        crate::routes::dashboard::get_dashboard_metrics,
        crate::routes::dashboard::get_invoice_metrics,
        crate::routes::dashboard::get_approval_metrics,
        crate::routes::dashboard::get_vendor_metrics,
        crate::routes::dashboard::get_team_metrics,
        // QuickBooks integration
        crate::routes::quickbooks::quickbooks_connect,
        crate::routes::quickbooks::quickbooks_callback,
        crate::routes::quickbooks::quickbooks_disconnect,
        crate::routes::quickbooks::quickbooks_status,
        crate::routes::quickbooks::sync_vendors,
        crate::routes::quickbooks::sync_accounts,
        crate::routes::quickbooks::export_invoice_to_quickbooks,
        crate::routes::quickbooks::get_account_mappings,
        crate::routes::quickbooks::update_account_mappings,
        // Xero integration
        crate::routes::xero::xero_connect,
        crate::routes::xero::xero_callback,
        crate::routes::xero::xero_disconnect,
        crate::routes::xero::xero_status,
        crate::routes::xero::sync_contacts,
        crate::routes::xero::sync_accounts,
        crate::routes::xero::export_invoice_to_xero,
        crate::routes::xero::get_account_mappings,
        crate::routes::xero::update_account_mappings,
        // Sage Intacct integration
        crate::routes::sage_intacct::sage_intacct_connect,
        crate::routes::sage_intacct::sage_intacct_disconnect,
        crate::routes::sage_intacct::sage_intacct_status,
        crate::routes::sage_intacct::sync_vendors,
        crate::routes::sage_intacct::sync_accounts,
        crate::routes::sage_intacct::export_invoice_to_sage,
        crate::routes::sage_intacct::get_account_mappings,
        crate::routes::sage_intacct::update_account_mappings,
        crate::routes::sage_intacct::list_entities,
        // Salesforce integration
        crate::routes::salesforce::salesforce_connect,
        crate::routes::salesforce::salesforce_callback,
        crate::routes::salesforce::salesforce_disconnect,
        crate::routes::salesforce::salesforce_status,
        crate::routes::salesforce::sync_accounts,
        crate::routes::salesforce::sync_contacts,
        crate::routes::salesforce::get_account_mappings,
        crate::routes::salesforce::update_account_mappings,
        // Workday integration
        crate::routes::workday::workday_connect,
        crate::routes::workday::workday_callback,
        crate::routes::workday::workday_disconnect,
        crate::routes::workday::workday_status,
        crate::routes::workday::sync_suppliers,
        crate::routes::workday::sync_accounts,
        crate::routes::workday::export_invoice_to_workday,
        crate::routes::workday::get_account_mappings,
        crate::routes::workday::update_account_mappings,
        crate::routes::workday::list_companies,
        // Bill.com integration
        crate::routes::bill_com::bill_com_connect,
        crate::routes::bill_com::bill_com_disconnect,
        crate::routes::bill_com::bill_com_status,
        crate::routes::bill_com::sync_vendors,
        crate::routes::bill_com::push_bill_to_bill_com,
        crate::routes::bill_com::pay_bill,
        crate::routes::bill_com::pay_bulk,
        crate::routes::bill_com::list_payments,
        crate::routes::bill_com::list_funding_accounts,
        // Vendors
        crate::routes::vendors::list_vendors,
        crate::routes::vendors::create_vendor,
        crate::routes::vendors::get_vendor,
        crate::routes::vendors::update_vendor,
        crate::routes::vendors::delete_vendor,
        crate::routes::vendors::add_contact,
        crate::routes::vendors::remove_contact,
        crate::routes::vendors::list_tax_documents,
        crate::routes::vendors::upload_tax_document,
        crate::routes::vendors::list_messages,
        crate::routes::vendors::send_message,
        // Workflows
        crate::routes::workflows::list_rules,
        crate::routes::workflows::get_rule,
        crate::routes::workflows::create_rule,
        crate::routes::workflows::update_rule,
        crate::routes::workflows::delete_rule,
        crate::routes::workflows::activate_rule,
        crate::routes::workflows::deactivate_rule,
        crate::routes::workflows::list_queues,
        crate::routes::workflows::get_queue,
        crate::routes::workflows::create_queue,
        crate::routes::workflows::update_queue,
        crate::routes::workflows::delete_queue,
        crate::routes::workflows::list_queue_items,
        crate::routes::workflows::claim_item,
        crate::routes::workflows::complete_item,
        crate::routes::workflows::list_assignment_rules,
        crate::routes::workflows::get_assignment_rule,
        crate::routes::workflows::create_assignment_rule,
        crate::routes::workflows::update_assignment_rule,
        crate::routes::workflows::delete_assignment_rule,
        crate::routes::workflows::list_pending_approvals,
        crate::routes::workflows::get_approval,
        crate::routes::workflows::approve,
        crate::routes::workflows::reject,
        crate::routes::workflows::list_templates,
        crate::routes::workflows::get_template,
        crate::routes::workflows::create_template,
        crate::routes::workflows::update_template,
        crate::routes::workflows::delete_template,
        crate::routes::workflows::activate_template,
        crate::routes::workflows::deactivate_template,
        crate::routes::workflows::bulk_operation,
        crate::routes::workflows::put_on_hold,
        crate::routes::workflows::release_from_hold,
        crate::routes::workflows::void_invoice,
        crate::routes::workflows::mark_ready_for_payment,
        crate::routes::workflows::move_to_queue,
        crate::routes::workflows::list_delegations,
        crate::routes::workflows::get_delegation,
        crate::routes::workflows::create_delegation,
        crate::routes::workflows::update_delegation,
        crate::routes::workflows::delete_delegation,
        crate::routes::workflows::list_approval_limits,
        crate::routes::workflows::get_approval_limit,
        crate::routes::workflows::create_approval_limit,
        crate::routes::workflows::update_approval_limit,
        crate::routes::workflows::delete_approval_limit,
        // Reports
        crate::routes::reports::dashboard_summary,
        crate::routes::reports::invoices_by_vendor,
        crate::routes::reports::invoices_by_status,
        crate::routes::reports::invoice_aging,
        crate::routes::reports::vendor_spend,
        crate::routes::reports::workflow_metrics,
        crate::routes::reports::custom_report,
        crate::routes::reports::spend_trends,
        crate::routes::reports::category_breakdown,
        crate::routes::reports::vendor_performance,
        crate::routes::reports::approval_analytics,
        crate::routes::reports::list_digests,
        crate::routes::reports::create_digest,
        crate::routes::reports::delete_digest,
        // Export
        crate::routes::export::export_invoices_csv,
        crate::routes::export::export_invoices_json,
        crate::routes::export::export_vendors_csv,
        // Documents
        crate::routes::documents::upload_document,
        crate::routes::documents::download_document,
        crate::routes::documents::get_document_metadata,
        crate::routes::documents::delete_document,
        crate::routes::documents::list_invoice_documents,
        crate::routes::documents::upload_invoice_document,
        // Audit
        crate::routes::audit::list_audit_logs,
        // Sandbox
        crate::routes::sandbox::list_personas,
        crate::routes::sandbox::get_current_persona,
        crate::routes::sandbox::switch_persona,
        crate::routes::sandbox::get_tenant_context,
        // EDI
        crate::routes::edi::webhook_inbound,
        crate::routes::edi::edi_connect,
        crate::routes::edi::edi_disconnect,
        crate::routes::edi::edi_status,
        crate::routes::edi::list_documents,
        crate::routes::edi::get_document,
        crate::routes::edi::send_remittance,
        crate::routes::edi::list_outbound,
        crate::routes::edi::get_ack_timeouts,
        crate::routes::edi::list_partners,
        crate::routes::edi::create_partner,
        crate::routes::edi::update_partner,
        crate::routes::edi::delete_partner,
        // Purchase Orders
        crate::routes::purchase_orders::list_purchase_orders,
        crate::routes::purchase_orders::create_purchase_order,
        crate::routes::purchase_orders::get_purchase_order,
        crate::routes::purchase_orders::delete_purchase_order,
        crate::routes::purchase_orders::run_match,
        // Notifications
        crate::routes::notifications::install_slack,
        crate::routes::notifications::slack_callback,
        crate::routes::notifications::get_slack_status,
        crate::routes::notifications::disconnect_slack,
        crate::routes::notifications::configure_teams,
        crate::routes::notifications::get_teams_status,
        crate::routes::notifications::disconnect_teams,
        crate::routes::notifications::get_notification_preferences,
        crate::routes::notifications::update_notification_preferences,
        // Predictive Analytics
        crate::routes::predictive::get_forecasts,
        crate::routes::predictive::generate_forecast,
        crate::routes::predictive::get_forecast_by_id,
        crate::routes::predictive::get_anomalies,
        crate::routes::predictive::acknowledge_anomaly,
        crate::routes::predictive::detect_anomalies,
        crate::routes::predictive::get_budget_alerts,
        crate::routes::predictive::dismiss_alert,
        crate::routes::predictive::get_anomaly_rules,
        crate::routes::predictive::configure_anomaly_rule,
        crate::routes::predictive::get_anomaly_rule,
        crate::routes::predictive::update_anomaly_rule,
        // Mobile
        crate::routes::mobile::register_device,
        crate::routes::mobile::list_devices,
        crate::routes::mobile::update_device,
        crate::routes::mobile::unregister_device,
        crate::routes::mobile::get_dashboard,
        crate::routes::mobile::list_invoices,
        crate::routes::mobile::get_invoice,
        crate::routes::mobile::list_approvals,
        crate::routes::mobile::approve_invoice,
        crate::routes::mobile::reject_invoice,
        crate::routes::mobile::search,
        crate::routes::mobile::sync_invoices,
        crate::routes::mobile::sync_bulk,
        // Settings
        crate::routes::settings::get_settings,
        crate::routes::settings::update_settings,
        crate::routes::settings::list_invoice_statuses,
        crate::routes::settings::update_invoice_statuses,
        crate::routes::settings::seed_default_statuses,
        crate::routes::settings::delete_invoice_status,
        // Feedback
        crate::routes::feedback::submit_feedback,
        crate::routes::feedback::list_feedback,
        // Theme
        crate::routes::theme::get_org_theme,
        crate::routes::theme::create_org_theme,
        crate::routes::theme::update_org_theme,
        crate::routes::theme::delete_org_theme,
        crate::routes::theme::upload_logo,
        crate::routes::theme::delete_logo,
        crate::routes::theme::preview_theme,
        crate::routes::theme::export_theme,
        crate::routes::theme::import_theme,
        crate::routes::theme::get_user_theme,
        crate::routes::theme::create_user_theme,
        crate::routes::theme::update_user_theme,
        crate::routes::theme::delete_user_theme,
        crate::routes::theme::get_effective_theme,
        // Email Actions
        crate::routes::email_actions::handle_approve,
        crate::routes::email_actions::handle_reject,
        crate::routes::email_actions::handle_hold,
        crate::routes::email_actions::handle_view,
        // AI Assistant
        crate::routes::ai::chat_handler,
        crate::routes::ai::list_conversations_handler,
        crate::routes::ai::continue_conversation_handler,
        // Billing
        crate::routes::billing::list_plans,
        crate::routes::billing::get_subscription,
        // Routing
        crate::routes::routing::route_invoice,
        crate::routes::routing::get_workload_stats,
        crate::routes::routing::set_availability,
        crate::routes::routing::get_routing_config,
        crate::routes::routing::update_routing_config,
        // Payment Requests
        crate::routes::payment_requests::create_payment_request,
        crate::routes::payment_requests::list_payment_requests,
        crate::routes::payment_requests::get_payment_request,
        crate::routes::payment_requests::add_invoices,
        crate::routes::payment_requests::submit_request,
        // Vendor Statements
        crate::routes::vendor_statements::create_statement,
        crate::routes::vendor_statements::list_statements,
        crate::routes::vendor_statements::get_statement,
        crate::routes::vendor_statements::run_auto_match,
        crate::routes::vendor_statements::update_line,
        crate::routes::vendor_statements::reconcile_statement,
    ),
    components(
        schemas(
            LoginRequest,
            LoginResponse,
            RegisterRequest,
            RefreshRequest,
            UserInfo,
            Invoice,
            InvoiceList,
            Vendor,
            VendorList,
            HealthResponse,
            ErrorResponse,
            PaginationInfo,
            crate::routes::auth::MeResponse,
            crate::routes::invoices::UploadResponse,
        )
    )
)]
pub struct ApiDoc;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// Authentication Schemas
// ============================================================================

/// Login request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Tenant ID (UUID format)
    #[schema(example = "11111111-1111-1111-1111-111111111111")]
    pub tenant_id: String,
    /// User email address
    #[schema(example = "admin@sandbox.local")]
    pub email: String,
    /// User password
    #[schema(example = "sandbox123")]
    pub password: String,
}

/// Login response with JWT tokens
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    /// JWT access token for API authentication
    pub access_token: String,
    /// JWT refresh token for obtaining new access tokens
    pub refresh_token: String,
    /// Token type (always "Bearer")
    #[schema(example = "Bearer")]
    pub token_type: String,
    /// Access token expiration in seconds
    #[schema(example = 86400)]
    pub expires_in: u64,
}

/// User registration request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// Tenant ID to register under
    pub tenant_id: String,
    /// User email address
    pub email: String,
    /// Password (min 8 chars, requires uppercase, lowercase, number)
    pub password: String,
    /// User display name
    pub name: String,
}

/// Token refresh request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RefreshRequest {
    /// Refresh token from login response
    pub refresh_token: String,
}

/// Authenticated user information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserInfo {
    /// User ID (UUID)
    pub id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// User email
    pub email: String,
    /// User display name
    pub name: String,
    /// User roles (e.g., tenant_admin, ap_user, approver)
    pub roles: Vec<String>,
}

// ============================================================================
// Invoice Schemas
// ============================================================================

/// Invoice record
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Invoice {
    /// Invoice ID (UUID)
    pub id: String,
    /// Vendor ID (optional, UUID)
    pub vendor_id: Option<String>,
    /// Vendor name
    pub vendor_name: String,
    /// Invoice number
    #[schema(example = "INV-2024-0001")]
    pub invoice_number: String,
    /// Invoice date (YYYY-MM-DD)
    #[schema(example = "2024-01-15")]
    pub invoice_date: Option<String>,
    /// Due date (YYYY-MM-DD)
    #[schema(example = "2024-02-14")]
    pub due_date: Option<String>,
    /// Total amount in cents
    #[schema(example = 125000)]
    pub total_amount: i64,
    /// Currency code
    #[schema(example = "USD")]
    pub currency: String,
    /// Capture status (draft, pending_ocr, ready_for_review, reviewed, failed)
    pub capture_status: String,
    /// Processing status (submitted, pending_approval, approved, rejected, paid, voided)
    pub processing_status: String,
    /// Creation timestamp
    pub created_at: String,
}

/// Paginated list of invoices
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InvoiceList {
    /// Invoice records
    pub data: Vec<Invoice>,
    /// Pagination information
    pub pagination: PaginationInfo,
}

// ============================================================================
// Vendor Schemas
// ============================================================================

/// Vendor record
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Vendor {
    /// Vendor ID (UUID)
    pub id: String,
    /// Vendor name
    #[schema(example = "Acme Corporation")]
    pub name: String,
    /// Vendor type (business, contractor, individual)
    pub vendor_type: String,
    /// Contact email
    pub email: Option<String>,
    /// Contact phone
    pub phone: Option<String>,
    /// Vendor status (active, inactive)
    pub status: String,
    /// Creation timestamp
    pub created_at: String,
}

/// Paginated list of vendors
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VendorList {
    /// Vendor records
    pub data: Vec<Vendor>,
    /// Pagination information
    pub pagination: PaginationInfo,
}

// ============================================================================
// Common Schemas
// ============================================================================

/// Health check response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Health status (healthy, degraded, unhealthy)
    #[schema(example = "healthy")]
    pub status: String,
    /// Service version
    #[schema(example = "1.0.0")]
    pub version: String,
}

/// API error response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Error type
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Additional error details (optional)
    pub details: Option<serde_json::Value>,
}

/// Pagination information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginationInfo {
    /// Current page number (1-indexed)
    #[schema(example = 1)]
    pub page: u32,
    /// Items per page
    #[schema(example = 25)]
    pub per_page: u32,
    /// Total number of items
    #[schema(example = 100)]
    pub total: u64,
    /// Total number of pages
    #[schema(example = 4)]
    pub total_pages: u32,
}

/// Create the Swagger UI router
pub fn swagger_ui() -> Router {
    SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into()
}
