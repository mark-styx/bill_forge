//! Two-week implementation wizard orchestration.

use crate::error::ApiResult;
use crate::extractors::{AuthUser, InvoiceCaptureAccess, InvoiceProcessingAccess, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Multipart, State},
    routing::{get, patch, post},
    Json, Router,
};
use billforge_core::{
    domain::{
        CreatePOLineItemInput, CreatePurchaseOrderInput, CreateWorkflowTemplateInput, StageType,
        WorkflowTemplateStage,
    },
    traits::{PurchaseOrderRepository, WorkflowTemplateRepository},
    types::{Money, TenantId, UserId},
};
use billforge_quickbooks::{
    QBAccount, QBPurchaseOrder, QBPurchaseOrderLine, QBVendor, QuickBooksClient,
    QuickBooksEnvironment, QuickBooksOAuth, QuickBooksOAuthConfig,
};
use billforge_xero::{
    XeroAccount, XeroClient, XeroContact, XeroEnvironment, XeroOAuth, XeroOAuthConfig,
    XeroPurchaseOrder,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/erp/sync", post(sync_erp))
        .route("/erp/sub-items", patch(update_erp_sub_items))
        .route("/approval-template", post(select_approval_template))
        .route("/sample-invoices", post(upload_sample_invoices))
        .route("/checklist", patch(update_checklist))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    NotStarted,
    InProgress,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErpProvider {
    Quickbooks,
    Xero,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpSubItems {
    pub chart_of_accounts: bool,
    pub vendors: bool,
    pub open_pos: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpSyncSummary {
    pub provider: ErpProvider,
    pub connected: bool,
    pub account_mappings: i64,
    pub vendor_mappings: i64,
    pub open_purchase_orders: i64,
    pub message: String,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpPhase {
    pub status: PhaseStatus,
    pub provider: Option<ErpProvider>,
    pub sub_items: ErpSubItems,
    pub last_sync: Option<ErpSyncSummary>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApprovalsPhase {
    pub status: PhaseStatus,
    pub template: Option<String>,
    pub template_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OcrPhase {
    pub status: PhaseStatus,
    pub count: u8,
    pub sample_invoice_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoLiveChecks {
    pub notify_ap_team: bool,
    pub set_email_forwarding: bool,
    pub enable_approval_routing: bool,
    pub confirm_cutover_date: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoLivePhase {
    pub status: PhaseStatus,
    pub checks: GoLiveChecks,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImplementationPhases {
    pub erp: ErpPhase,
    pub approvals: ApprovalsPhase,
    pub ocr: OcrPhase,
    pub go_live: GoLivePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImplementationWizardState {
    pub started_at: DateTime<Utc>,
    pub phases: ImplementationPhases,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImplementationStatusResponse {
    pub started_at: DateTime<Utc>,
    pub day_number: u8,
    pub percent_complete: u8,
    pub phases: ImplementationPhases,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ErpSyncRequest {
    pub provider: ErpProvider,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateErpSubItemsRequest {
    pub sub_items: ErpSubItems,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SelectApprovalTemplateRequest {
    pub template: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SampleInvoiceUploadResponse {
    pub uploaded: Vec<crate::routes::invoices::UploadResponse>,
    pub status: ImplementationStatusResponse,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateChecklistRequest {
    pub checks: GoLiveChecks,
}

#[utoipa::path(
    get,
    path = "/api/v1/implementation/status",
    tag = "Implementation",
    responses((status = 200, description = "Implementation wizard status", body = ImplementationStatusResponse))
)]
pub async fn get_status(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    Ok(Json(status_response(wizard)))
}

#[utoipa::path(
    post,
    path = "/api/v1/implementation/erp/sync",
    tag = "Implementation",
    request_body = ErpSyncRequest,
    responses((status = 200, description = "ERP sync state recorded", body = ImplementationStatusResponse))
)]
pub async fn sync_erp(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ErpSyncRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    let summary = run_erp_sync(&state, &tenant.tenant_id, &user.user_id, request.provider).await?;

    wizard.phases.erp.provider = Some(request.provider);
    wizard.phases.erp.sub_items.chart_of_accounts = summary.account_mappings > 0;
    wizard.phases.erp.sub_items.vendors = summary.vendor_mappings > 0;
    wizard.phases.erp.sub_items.open_pos = summary.open_purchase_orders > 0;
    wizard.phases.erp.last_error = if summary.connected {
        None
    } else {
        Some(summary.message.clone())
    };
    wizard.phases.erp.last_sync = Some(summary);
    recompute_statuses(&mut wizard);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    Ok(Json(status_response(wizard)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/implementation/erp/sub-items",
    tag = "Implementation",
    request_body = UpdateErpSubItemsRequest,
    responses((status = 200, description = "ERP checklist updated", body = ImplementationStatusResponse))
)]
pub async fn update_erp_sub_items(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<UpdateErpSubItemsRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    wizard.phases.erp.sub_items = request.sub_items;
    recompute_statuses(&mut wizard);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(status_response(wizard)))
}

#[utoipa::path(
    post,
    path = "/api/v1/implementation/approval-template",
    tag = "Implementation",
    request_body = SelectApprovalTemplateRequest,
    responses((status = 200, description = "Approval template selected", body = ImplementationStatusResponse))
)]
pub async fn select_approval_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Json(request): Json<SelectApprovalTemplateRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let input = approval_template_input(&request.template)?;

    let existing = WorkflowTemplateRepository::list(&repo, &tenant.tenant_id)
        .await?
        .into_iter()
        .find(|template| template.name == input.name);

    let template = if let Some(existing) = existing {
        WorkflowTemplateRepository::update(&repo, &tenant.tenant_id, &existing.id, input).await?
    } else {
        WorkflowTemplateRepository::create(&repo, &tenant.tenant_id, input).await?
    };

    wizard.phases.approvals.template = Some(request.template);
    wizard.phases.approvals.template_id = Some(template.id.0);
    recompute_statuses(&mut wizard);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    Ok(Json(status_response(wizard)))
}

#[utoipa::path(
    post,
    path = "/api/v1/implementation/sample-invoices",
    tag = "Implementation",
    responses((status = 200, description = "Sample invoices uploaded", body = SampleInvoiceUploadResponse))
)]
pub async fn upload_sample_invoices(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    mut multipart: Multipart,
) -> ApiResult<Json<SampleInvoiceUploadResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    let remaining = 10usize.saturating_sub(wizard.phases.ocr.sample_invoice_ids.len());
    if remaining == 0 {
        return Ok(Json(SampleInvoiceUploadResponse {
            uploaded: Vec::new(),
            status: status_response(wizard),
        }));
    }

    let mut uploaded = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Upload error: {}", e)))?
    {
        if field.name().unwrap_or("") != "files" && field.name().unwrap_or("") != "file" {
            continue;
        }
        if uploaded.len() >= remaining {
            break;
        }

        let file_name = field
            .file_name()
            .unwrap_or("sample-invoice.pdf")
            .to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/pdf")
            .to_string();
        let data = field.bytes().await.map_err(|e| {
            billforge_core::Error::Validation(format!("Failed to read file: {}", e))
        })?;
        let response = crate::routes::invoices::upload_invoice_file(
            &state,
            &user,
            &tenant,
            file_name,
            content_type,
            &data,
        )
        .await?;
        let invoice_id = response.invoice_id.parse().map_err(|_| {
            billforge_core::Error::Internal("Invoice upload returned invalid ID".to_string())
        })?;
        wizard.phases.ocr.sample_invoice_ids.push(invoice_id);
        uploaded.push(response);
    }

    if uploaded.is_empty() {
        return Err(billforge_core::Error::Validation("No files provided".to_string()).into());
    }

    wizard.phases.ocr.count = wizard.phases.ocr.sample_invoice_ids.len().min(10) as u8;
    recompute_statuses(&mut wizard);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    Ok(Json(SampleInvoiceUploadResponse {
        uploaded,
        status: status_response(wizard),
    }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/implementation/checklist",
    tag = "Implementation",
    request_body = UpdateChecklistRequest,
    responses((status = 200, description = "Go-live checklist updated", body = ImplementationStatusResponse))
)]
pub async fn update_checklist(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<UpdateChecklistRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    wizard.phases.go_live.checks = request.checks;
    recompute_statuses(&mut wizard);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(status_response(wizard)))
}

async fn load_or_create_state(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<ImplementationWizardState, billforge_core::Error> {
    let row: Option<(DateTime<Utc>, serde_json::Value)> = sqlx::query_as(
        "SELECT started_at, state FROM implementation_wizard_states WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to load wizard state: {}", e)))?;

    if let Some((started_at, state_value)) = row {
        let mut state = serde_json::from_value::<ImplementationWizardState>(state_value)
            .unwrap_or_else(|_| default_state(started_at));
        state.started_at = started_at;
        recompute_statuses(&mut state);
        return Ok(state);
    }

    let state = default_state(Utc::now());
    sqlx::query(
        "INSERT INTO implementation_wizard_states (tenant_id, started_at, state, created_at, updated_at)
         VALUES ($1, $2, $3, NOW(), NOW())
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(tenant_id.as_uuid())
    .bind(state.started_at)
    .bind(serde_json::to_value(&state).map_err(|e| {
        billforge_core::Error::Internal(format!("Failed to serialize wizard state: {}", e))
    })?)
    .execute(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to create wizard state: {}", e))
    })?;

    Ok(state)
}

async fn save_state(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    state: &ImplementationWizardState,
) -> Result<(), billforge_core::Error> {
    sqlx::query(
        "UPDATE implementation_wizard_states
         SET state = $2, updated_at = NOW()
         WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .bind(serde_json::to_value(state).map_err(|e| {
        billforge_core::Error::Internal(format!("Failed to serialize wizard state: {}", e))
    })?)
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to save wizard state: {}", e)))?;
    Ok(())
}

async fn run_erp_sync(
    state: &AppState,
    tenant_id: &TenantId,
    user_id: &UserId,
    provider: ErpProvider,
) -> Result<ErpSyncSummary, billforge_core::Error> {
    let pool = state.db.tenant(tenant_id).await?;
    let import_stats = match provider {
        ErpProvider::Quickbooks => {
            sync_quickbooks_implementation(state, &pool, tenant_id, user_id).await?
        }
        ErpProvider::Xero => sync_xero_implementation(state, &pool, tenant_id, user_id).await?,
    };

    erp_sync_summary(&pool, tenant_id, provider, Some(import_stats)).await
}

#[derive(Debug, Clone, Copy, Default)]
struct PurchaseOrderImportStats {
    imported: i64,
    skipped: i64,
    errors: i64,
}

async fn erp_sync_summary(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    provider: ErpProvider,
    import_stats: Option<PurchaseOrderImportStats>,
) -> Result<ErpSyncSummary, billforge_core::Error> {
    let (connected, account_mappings, vendor_mappings) =
        provider_counts(pool, tenant_id, provider).await?;

    let open_purchase_orders = open_purchase_order_count(pool, tenant_id).await?;
    let message = match (connected, import_stats) {
        (true, Some(stats)) => format!(
            "Synced ERP setup. Found {} account mappings, {} vendor mappings, and {} open purchase orders. Imported {} POs, skipped {}, errors {}.",
            account_mappings,
            vendor_mappings,
            open_purchase_orders,
            stats.imported,
            stats.skipped,
            stats.errors
        ),
        (true, None) => format!(
            "Found {} account mappings, {} vendor mappings, and {} open purchase orders.",
            account_mappings, vendor_mappings, open_purchase_orders
        ),
        (false, _) => format!("{:?} is not connected or sync is disabled.", provider),
    };

    Ok(ErpSyncSummary {
        provider,
        connected,
        account_mappings,
        vendor_mappings,
        open_purchase_orders,
        message,
        synced_at: Utc::now(),
    })
}

async fn provider_counts(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    provider: ErpProvider,
) -> Result<(bool, i64, i64), billforge_core::Error> {
    match provider {
        ErpProvider::Quickbooks => {
            let connected = scalar_count(
                pool,
                "SELECT COUNT(*) FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true",
                tenant_id,
            )
            .await?
                > 0;
            let accounts = scalar_count(
                pool,
                "SELECT COUNT(*) FROM quickbooks_account_mappings WHERE tenant_id = $1",
                tenant_id,
            )
            .await?;
            let vendors = scalar_count(
                pool,
                "SELECT COUNT(*) FROM quickbooks_vendor_mappings WHERE tenant_id = $1",
                tenant_id,
            )
            .await?;
            Ok((connected, accounts, vendors))
        }
        ErpProvider::Xero => {
            let connected = scalar_count(
                pool,
                "SELECT COUNT(*) FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true",
                tenant_id,
            )
            .await?
                > 0;
            let accounts = scalar_count(
                pool,
                "SELECT COUNT(*) FROM xero_account_mappings WHERE tenant_id = $1",
                tenant_id,
            )
            .await?;
            let vendors = scalar_count(
                pool,
                "SELECT COUNT(*) FROM xero_contact_mappings WHERE tenant_id = $1",
                tenant_id,
            )
            .await?;
            Ok((connected, accounts, vendors))
        }
    }
}

async fn open_purchase_order_count(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<i64, billforge_core::Error> {
    scalar_count(
        pool,
        "SELECT COUNT(*) FROM purchase_orders WHERE tenant_id = $1 AND status IN ('open', 'partially_fulfilled')",
        tenant_id,
    )
    .await
}

async fn sync_quickbooks_implementation(
    state: &AppState,
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: &UserId,
) -> Result<PurchaseOrderImportStats, billforge_core::Error> {
    let client = quickbooks_client(state, pool, tenant_id).await?;
    sync_quickbooks_accounts(pool, tenant_id, &client).await?;
    sync_quickbooks_vendors(pool, tenant_id, &client).await?;
    let stats = import_quickbooks_open_purchase_orders(pool, tenant_id, user_id, &client).await?;

    sqlx::query("UPDATE quickbooks_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update QuickBooks sync time: {}", e))
        })?;

    Ok(stats)
}

async fn sync_xero_implementation(
    state: &AppState,
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: &UserId,
) -> Result<PurchaseOrderImportStats, billforge_core::Error> {
    let client = xero_client(state, pool, tenant_id).await?;
    sync_xero_accounts(pool, tenant_id, &client).await?;
    sync_xero_contacts(pool, tenant_id, &client).await?;
    let stats = import_xero_open_purchase_orders(pool, tenant_id, user_id, &client).await?;

    sqlx::query("UPDATE xero_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update Xero sync time: {}", e))
        })?;

    Ok(stats)
}

async fn quickbooks_client(
    state: &AppState,
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<QuickBooksClient, billforge_core::Error> {
    let connection: Option<(String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT company_id, access_token, refresh_token, access_token_expires_at
         FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to read QuickBooks connection: {}", e))
    })?;

    let (company_id, mut access_token, refresh_token, expires_at) =
        connection.ok_or_else(|| {
            billforge_core::Error::Validation(
                "QuickBooks not connected or sync disabled".to_string(),
            )
        })?;

    let config = state.config.quickbooks.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("QuickBooks integration not configured".to_string())
    })?;
    let environment = quickbooks_environment(config);

    if expires_at <= Utc::now() + Duration::minutes(5) {
        let oauth = QuickBooksOAuth::new(QuickBooksOAuthConfig {
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            redirect_uri: config.redirect_uri.clone(),
            environment,
        });
        let tokens = oauth.refresh_token(&refresh_token).await.map_err(|e| {
            billforge_core::Error::Validation(format!(
                "QuickBooks token refresh failed: {}. Please reconnect.",
                e
            ))
        })?;
        let now = Utc::now();
        sqlx::query(
            "UPDATE quickbooks_connections
             SET access_token = $2, refresh_token = $3, access_token_expires_at = $4,
                 refresh_token_expires_at = $5, updated_at = NOW()
             WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&tokens.access_token)
        .bind(&tokens.refresh_token)
        .bind(now + Duration::seconds(tokens.expires_in))
        .bind(now + Duration::seconds(tokens.x_refresh_token_expires_in))
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to save QuickBooks tokens: {}", e))
        })?;
        access_token = tokens.access_token;
    }

    Ok(QuickBooksClient::new(access_token, company_id, environment))
}

fn quickbooks_environment(config: &crate::config::QuickBooksConfig) -> QuickBooksEnvironment {
    match config.environment {
        crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
        crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
    }
}

async fn xero_client(
    state: &AppState,
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<XeroClient, billforge_core::Error> {
    let connection: Option<(String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT xero_tenant_id, access_token, refresh_token, access_token_expires_at
         FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to read Xero connection: {}", e))
    })?;

    let (xero_tenant_id, mut access_token, refresh_token, expires_at) =
        connection.ok_or_else(|| {
            billforge_core::Error::Validation("Xero not connected or sync disabled".to_string())
        })?;

    let config = state.config.xero.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("Xero integration not configured".to_string())
    })?;
    let environment = xero_environment(config);

    if expires_at <= Utc::now() + Duration::minutes(5) {
        let oauth = XeroOAuth::new(XeroOAuthConfig {
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            redirect_uri: config.redirect_uri.clone(),
            environment,
        });
        let tokens = oauth.refresh_token(&refresh_token).await.map_err(|e| {
            billforge_core::Error::Validation(format!(
                "Xero token refresh failed: {}. Please reconnect.",
                e
            ))
        })?;
        let now = Utc::now();
        sqlx::query(
            "UPDATE xero_connections
             SET access_token = $2, refresh_token = $3, access_token_expires_at = $4,
                 updated_at = NOW()
             WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&tokens.access_token)
        .bind(&tokens.refresh_token)
        .bind(now + Duration::seconds(tokens.expires_in))
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to save Xero tokens: {}", e))
        })?;
        access_token = tokens.access_token;
    }

    Ok(XeroClient::new(access_token, xero_tenant_id, environment))
}

fn xero_environment(config: &crate::config::XeroConfig) -> XeroEnvironment {
    match config.environment {
        crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
        crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
    }
}

async fn sync_quickbooks_accounts(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    client: &QuickBooksClient,
) -> Result<(), billforge_core::Error> {
    let mut start_position = 1;
    let max_results = 100;

    loop {
        let accounts = client
            .query_accounts(start_position, max_results)
            .await
            .map_err(|e| {
                billforge_core::Error::Validation(format!("QuickBooks API error: {}", e))
            })?;
        if accounts.is_empty() {
            break;
        }

        for account in accounts
            .into_iter()
            .filter(|account| quickbooks_expense_account(account))
        {
            sqlx::query(
                "INSERT INTO quickbooks_account_mappings
                 (tenant_id, quickbooks_account_id, quickbooks_account_name, quickbooks_account_type, billforge_gl_code, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $2, NOW(), NOW())
                 ON CONFLICT (tenant_id, quickbooks_account_id) DO UPDATE SET
                    quickbooks_account_name = $3,
                    quickbooks_account_type = $4,
                    updated_at = NOW()",
            )
            .bind(tenant_id.as_uuid())
            .bind(&account.Id)
            .bind(&account.Name)
            .bind(&account.AccountType)
            .execute(pool)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to sync QuickBooks account: {}", e)))?;
        }

        start_position += max_results;
    }

    Ok(())
}

fn quickbooks_expense_account(account: &QBAccount) -> bool {
    account.Active && account.Classification.eq_ignore_ascii_case("expense")
}

async fn sync_quickbooks_vendors(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    client: &QuickBooksClient,
) -> Result<(), billforge_core::Error> {
    let mut start_position = 1;
    let max_results = 100;

    loop {
        let vendors = client
            .query_vendors(start_position, max_results)
            .await
            .map_err(|e| {
                billforge_core::Error::Validation(format!("QuickBooks API error: {}", e))
            })?;
        if vendors.is_empty() {
            break;
        }

        for vendor in vendors {
            ensure_quickbooks_vendor(pool, tenant_id, &vendor).await?;
        }

        start_position += max_results;
    }

    Ok(())
}

async fn ensure_quickbooks_vendor(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor: &QBVendor,
) -> Result<Uuid, billforge_core::Error> {
    if let Some(vendor_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT billforge_vendor_id FROM quickbooks_vendor_mappings
         WHERE tenant_id = $1 AND quickbooks_vendor_id = $2",
    )
    .bind(tenant_id.as_uuid())
    .bind(&vendor.Id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to read QuickBooks vendor mapping: {}", e))
    })? {
        sqlx::query(
            "UPDATE vendors SET name = $2, email = $3, phone = $4, status = $5, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(vendor_id)
        .bind(&vendor.DisplayName)
        .bind(
            vendor
                .PrimaryEmailAddr
                .as_ref()
                .map(|email| email.Address.as_str()),
        )
        .bind(
            vendor
                .PrimaryPhone
                .as_ref()
                .map(|phone| phone.FreeFormNumber.as_str()),
        )
        .bind(if vendor.Active { "active" } else { "inactive" })
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update QuickBooks vendor: {}", e))
        })?;

        sqlx::query(
            "UPDATE quickbooks_vendor_mappings
             SET quickbooks_vendor_name = $3, sync_token = $4, last_synced_at = NOW(), updated_at = NOW()
             WHERE tenant_id = $1 AND quickbooks_vendor_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&vendor.Id)
        .bind(&vendor.DisplayName)
        .bind(&vendor.SyncToken)
        .execute(pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to update QuickBooks vendor mapping: {}", e)))?;

        return Ok(vendor_id);
    }

    let vendor_id = upsert_vendor_by_name(
        pool,
        tenant_id,
        &vendor.DisplayName,
        vendor
            .PrimaryEmailAddr
            .as_ref()
            .map(|email| email.Address.as_str()),
        vendor
            .PrimaryPhone
            .as_ref()
            .map(|phone| phone.FreeFormNumber.as_str()),
        vendor.Active,
    )
    .await?;

    sqlx::query(
        "INSERT INTO quickbooks_vendor_mappings
         (tenant_id, quickbooks_vendor_id, billforge_vendor_id, quickbooks_vendor_name, sync_token, last_synced_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())
         ON CONFLICT (tenant_id, quickbooks_vendor_id) DO UPDATE SET
            billforge_vendor_id = $3,
            quickbooks_vendor_name = $4,
            sync_token = $5,
            last_synced_at = NOW(),
            updated_at = NOW()",
    )
    .bind(tenant_id.as_uuid())
    .bind(&vendor.Id)
    .bind(vendor_id)
    .bind(&vendor.DisplayName)
    .bind(&vendor.SyncToken)
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to save QuickBooks vendor mapping: {}", e)))?;

    Ok(vendor_id)
}

async fn sync_xero_accounts(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    client: &XeroClient,
) -> Result<(), billforge_core::Error> {
    let mut page = 1;
    let page_size = 100;

    loop {
        let accounts = client
            .query_accounts(page, page_size)
            .await
            .map_err(|e| billforge_core::Error::Validation(format!("Xero API error: {}", e)))?;
        if accounts.is_empty() {
            break;
        }

        for account in accounts
            .into_iter()
            .filter(|account| xero_expense_account(account))
        {
            sqlx::query(
                "INSERT INTO xero_account_mappings
                 (tenant_id, xero_account_id, xero_account_code, xero_account_name, xero_account_type, billforge_gl_code, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $3, NOW(), NOW())
                 ON CONFLICT (tenant_id, xero_account_id) DO UPDATE SET
                    xero_account_code = $3,
                    xero_account_name = $4,
                    xero_account_type = $5,
                    updated_at = NOW()",
            )
            .bind(tenant_id.as_uuid())
            .bind(&account.AccountID)
            .bind(&account.Code)
            .bind(&account.Name)
            .bind(&account.AccountType)
            .execute(pool)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to sync Xero account: {}", e)))?;
        }

        page += 1;
    }

    Ok(())
}

fn xero_expense_account(account: &XeroAccount) -> bool {
    account.Status.eq_ignore_ascii_case("active") && account.Class.eq_ignore_ascii_case("expense")
}

async fn sync_xero_contacts(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    client: &XeroClient,
) -> Result<(), billforge_core::Error> {
    let mut page = 1;
    let page_size = 100;

    loop {
        let contacts = client
            .query_contacts(page, page_size)
            .await
            .map_err(|e| billforge_core::Error::Validation(format!("Xero API error: {}", e)))?;
        if contacts.is_empty() {
            break;
        }

        for contact in contacts
            .into_iter()
            .filter(|contact| contact.IsSupplier.unwrap_or(false))
        {
            ensure_xero_contact(pool, tenant_id, &contact).await?;
        }

        page += 1;
    }

    Ok(())
}

async fn ensure_xero_contact(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    contact: &XeroContact,
) -> Result<Uuid, billforge_core::Error> {
    if let Some(vendor_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT billforge_vendor_id FROM xero_contact_mappings
         WHERE tenant_id = $1 AND xero_contact_id = $2",
    )
    .bind(tenant_id.as_uuid())
    .bind(&contact.ContactID)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to read Xero contact mapping: {}", e))
    })? {
        sqlx::query(
            "UPDATE vendors SET name = $2, email = $3, status = $4, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(vendor_id)
        .bind(&contact.Name)
        .bind(&contact.EmailAddress)
        .bind(if contact.ContactStatus.eq_ignore_ascii_case("active") {
            "active"
        } else {
            "inactive"
        })
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update Xero contact: {}", e))
        })?;

        sqlx::query(
            "UPDATE xero_contact_mappings
             SET xero_contact_name = $3, last_synced_at = NOW(), updated_at = NOW()
             WHERE tenant_id = $1 AND xero_contact_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&contact.ContactID)
        .bind(&contact.Name)
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update Xero contact mapping: {}", e))
        })?;

        return Ok(vendor_id);
    }

    let vendor_id = upsert_vendor_by_name(
        pool,
        tenant_id,
        &contact.Name,
        contact.EmailAddress.as_deref(),
        None,
        contact.ContactStatus.eq_ignore_ascii_case("active"),
    )
    .await?;

    sqlx::query(
        "INSERT INTO xero_contact_mappings
         (tenant_id, xero_contact_id, billforge_vendor_id, xero_contact_name, last_synced_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())
         ON CONFLICT (tenant_id, xero_contact_id) DO UPDATE SET
            billforge_vendor_id = $3,
            xero_contact_name = $4,
            last_synced_at = NOW(),
            updated_at = NOW()",
    )
    .bind(tenant_id.as_uuid())
    .bind(&contact.ContactID)
    .bind(vendor_id)
    .bind(&contact.Name)
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to save Xero contact mapping: {}", e)))?;

    Ok(vendor_id)
}

async fn upsert_vendor_by_name(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    active: bool,
) -> Result<Uuid, billforge_core::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, is_active, created_at, updated_at)
         VALUES ($1, $2, $3, 'business', $4, $5, $6, $7, NOW(), NOW())
         ON CONFLICT (tenant_id, name) DO UPDATE SET
            email = COALESCE(EXCLUDED.email, vendors.email),
            phone = COALESCE(EXCLUDED.phone, vendors.phone),
            status = EXCLUDED.status,
            is_active = EXCLUDED.is_active,
            updated_at = NOW()
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.as_uuid())
    .bind(name)
    .bind(email)
    .bind(phone)
    .bind(if active { "active" } else { "inactive" })
    .bind(active)
    .fetch_one(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to upsert vendor: {}", e)))
}

async fn import_quickbooks_open_purchase_orders(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: &UserId,
    client: &QuickBooksClient,
) -> Result<PurchaseOrderImportStats, billforge_core::Error> {
    let mut stats = PurchaseOrderImportStats::default();
    let mut start_position = 1;
    let max_results = 100;

    loop {
        let purchase_orders = client
            .query_open_purchase_orders(start_position, max_results)
            .await
            .map_err(|e| {
                billforge_core::Error::Validation(format!("QuickBooks API error: {}", e))
            })?;
        if purchase_orders.is_empty() {
            break;
        }

        for purchase_order in purchase_orders {
            match import_quickbooks_purchase_order(pool, tenant_id, user_id, purchase_order).await {
                Ok(true) => stats.imported += 1,
                Ok(false) => stats.skipped += 1,
                Err(_) => stats.errors += 1,
            }
        }

        start_position += max_results;
    }

    Ok(stats)
}

async fn import_quickbooks_purchase_order(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: &UserId,
    purchase_order: QBPurchaseOrder,
) -> Result<bool, billforge_core::Error> {
    let po_number = purchase_order
        .DocNumber
        .clone()
        .filter(|number| !number.trim().is_empty())
        .unwrap_or_else(|| purchase_order.Id.clone());

    let repo = billforge_db::repositories::PurchaseOrderRepositoryImpl::new(std::sync::Arc::new(
        pool.clone(),
    ));
    if repo
        .find_by_po_number(tenant_id, &po_number)
        .await?
        .is_some()
    {
        return Ok(false);
    }

    let vendor_name = purchase_order
        .VendorRef
        .name
        .clone()
        .unwrap_or_else(|| format!("QuickBooks Vendor {}", purchase_order.VendorRef.value));
    let vendor_id = upsert_vendor_by_name(pool, tenant_id, &vendor_name, None, None, true).await?;
    let currency = purchase_order
        .CurrencyRef
        .as_ref()
        .map(|currency| currency.value.clone())
        .unwrap_or_else(|| "USD".to_string());
    let total = money_from_major(purchase_order.TotalAmt.unwrap_or(0.0), &currency);

    let input = CreatePurchaseOrderInput {
        po_number: po_number.clone(),
        vendor_id,
        vendor_name,
        order_date: parse_date(purchase_order.TxnDate.as_deref()),
        expected_delivery: None,
        line_items: quickbooks_po_lines(
            &po_number,
            purchase_order.Line.as_deref(),
            &currency,
            &total,
        ),
        total_amount: total,
        ship_to_address: None,
        notes: Some(format!(
            "Imported from QuickBooks purchase order {}",
            purchase_order.Id
        )),
    };

    repo.create(tenant_id, input, user_id).await?;
    Ok(true)
}

fn quickbooks_po_lines(
    po_number: &str,
    lines: Option<&[QBPurchaseOrderLine]>,
    currency: &str,
    total: &Money,
) -> Vec<CreatePOLineItemInput> {
    let mapped: Vec<_> = lines
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let quantity = line
                .ItemBasedExpenseLineDetail
                .as_ref()
                .and_then(|detail| detail.Qty)
                .unwrap_or(1.0);
            let amount = line.Amount.unwrap_or(0.0);
            let unit_price = line
                .ItemBasedExpenseLineDetail
                .as_ref()
                .and_then(|detail| detail.UnitPrice)
                .unwrap_or_else(|| {
                    if quantity > 0.0 {
                        amount / quantity
                    } else {
                        amount
                    }
                });

            CreatePOLineItemInput {
                line_number: line
                    .LineNum
                    .and_then(|line_num| u32::try_from(line_num).ok())
                    .or(Some((idx + 1) as u32)),
                description: line
                    .Description
                    .clone()
                    .unwrap_or_else(|| format!("Purchase order {} line {}", po_number, idx + 1)),
                quantity,
                unit_of_measure: "EA".to_string(),
                unit_price: money_from_major(unit_price, currency),
                total: money_from_major(amount, currency),
                product_id: line
                    .ItemBasedExpenseLineDetail
                    .as_ref()
                    .and_then(|detail| detail.ItemRef.as_ref())
                    .map(|item| item.value.clone()),
            }
        })
        .collect();

    if mapped.is_empty() {
        vec![summary_po_line(po_number, total)]
    } else {
        mapped
    }
}

async fn import_xero_open_purchase_orders(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: &UserId,
    client: &XeroClient,
) -> Result<PurchaseOrderImportStats, billforge_core::Error> {
    let mut stats = PurchaseOrderImportStats::default();
    let mut page = 1;
    let page_size = 100;

    loop {
        let purchase_orders = client
            .query_purchase_orders(page, page_size)
            .await
            .map_err(|e| billforge_core::Error::Validation(format!("Xero API error: {}", e)))?;
        if purchase_orders.is_empty() {
            break;
        }

        for purchase_order in purchase_orders {
            if !xero_purchase_order_open(&purchase_order) {
                stats.skipped += 1;
                continue;
            }

            match import_xero_purchase_order(pool, tenant_id, user_id, purchase_order).await {
                Ok(true) => stats.imported += 1,
                Ok(false) => stats.skipped += 1,
                Err(_) => stats.errors += 1,
            }
        }

        page += 1;
    }

    Ok(stats)
}

fn xero_purchase_order_open(purchase_order: &XeroPurchaseOrder) -> bool {
    matches!(
        purchase_order.Status.as_deref(),
        Some("AUTHORISED") | Some("SUBMITTED") | Some("DRAFT")
    )
}

async fn import_xero_purchase_order(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: &UserId,
    purchase_order: XeroPurchaseOrder,
) -> Result<bool, billforge_core::Error> {
    let po_number = purchase_order
        .PurchaseOrderNumber
        .clone()
        .or(purchase_order.PurchaseOrderID.clone())
        .filter(|number| !number.trim().is_empty())
        .ok_or_else(|| {
            billforge_core::Error::Validation("Xero purchase order missing number".to_string())
        })?;

    let repo = billforge_db::repositories::PurchaseOrderRepositoryImpl::new(std::sync::Arc::new(
        pool.clone(),
    ));
    if repo
        .find_by_po_number(tenant_id, &po_number)
        .await?
        .is_some()
    {
        return Ok(false);
    }

    let contact = purchase_order.Contact.as_ref();
    let vendor_name = contact
        .and_then(|contact| contact.Name.clone())
        .unwrap_or_else(|| "Xero Vendor".to_string());
    let vendor_id = upsert_vendor_by_name(pool, tenant_id, &vendor_name, None, None, true).await?;
    let currency = purchase_order.CurrencyCode.as_deref().unwrap_or("USD");
    let total = money_from_major(purchase_order.Total.unwrap_or(0.0), currency);

    let input = CreatePurchaseOrderInput {
        po_number: po_number.clone(),
        vendor_id,
        vendor_name,
        order_date: parse_date(purchase_order.Date.as_deref()),
        expected_delivery: parse_optional_date(purchase_order.DeliveryDate.as_deref()),
        line_items: xero_po_lines(
            &po_number,
            purchase_order.LineItems.as_deref(),
            currency,
            &total,
        ),
        total_amount: total,
        ship_to_address: None,
        notes: purchase_order
            .PurchaseOrderID
            .map(|id| format!("Imported from Xero purchase order {}", id)),
    };

    repo.create(tenant_id, input, user_id).await?;
    Ok(true)
}

fn xero_po_lines(
    po_number: &str,
    lines: Option<&[billforge_xero::XeroLineItem]>,
    currency: &str,
    total: &Money,
) -> Vec<CreatePOLineItemInput> {
    let mapped: Vec<_> = lines
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let quantity = line.Quantity.unwrap_or(1.0);
            let amount = line.LineAmount.or(line.UnitAmount).unwrap_or(0.0);
            let unit_price = line.UnitAmount.unwrap_or_else(|| {
                if quantity > 0.0 {
                    amount / quantity
                } else {
                    amount
                }
            });

            CreatePOLineItemInput {
                line_number: Some((idx + 1) as u32),
                description: line
                    .Description
                    .clone()
                    .unwrap_or_else(|| format!("Purchase order {} line {}", po_number, idx + 1)),
                quantity,
                unit_of_measure: "EA".to_string(),
                unit_price: money_from_major(unit_price, currency),
                total: money_from_major(amount, currency),
                product_id: line.LineItemID.clone(),
            }
        })
        .collect();

    if mapped.is_empty() {
        vec![summary_po_line(po_number, total)]
    } else {
        mapped
    }
}

fn summary_po_line(po_number: &str, total: &Money) -> CreatePOLineItemInput {
    CreatePOLineItemInput {
        line_number: Some(1),
        description: format!("Purchase order {}", po_number),
        quantity: 1.0,
        unit_of_measure: "EA".to_string(),
        unit_price: total.clone(),
        total: total.clone(),
        product_id: None,
    }
}

fn money_from_major(amount: f64, currency: &str) -> Money {
    Money::new((amount * 100.0).round() as i64, currency.to_string())
}

fn parse_date(value: Option<&str>) -> NaiveDate {
    parse_optional_date(value).unwrap_or_else(|| Utc::now().date_naive())
}

fn parse_optional_date(value: Option<&str>) -> Option<NaiveDate> {
    let value = value?;
    let date_part = value.get(0..10).unwrap_or(value);
    NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()
}

async fn scalar_count(
    pool: &sqlx::PgPool,
    sql: &str,
    tenant_id: &TenantId,
) -> Result<i64, billforge_core::Error> {
    sqlx::query_scalar::<_, i64>(sql)
        .bind(tenant_id.as_uuid())
        .fetch_one(pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to read count: {}", e)))
}

fn approval_template_input(
    template: &str,
) -> Result<CreateWorkflowTemplateInput, billforge_core::Error> {
    let (name, description, approval_stage) = match template {
        "amount" => (
            "Implementation - By amount threshold",
            "Routes invoices through approval stages based on invoice amount.",
            "Amount Approval",
        ),
        "department" => (
            "Implementation - By department",
            "Routes invoices through department-specific approval ownership.",
            "Department Approval",
        ),
        "gl" => (
            "Implementation - By GL code",
            "Routes invoices through approval ownership based on GL coding.",
            "GL Code Approval",
        ),
        _ => {
            return Err(billforge_core::Error::Validation(
                "Unknown approval template".to_string(),
            ))
        }
    };

    Ok(CreateWorkflowTemplateInput {
        name: name.to_string(),
        description: Some(description.to_string()),
        is_default: true,
        stages: vec![
            stage(0, "Intake", StageType::Intake, false, Some(4)),
            stage(1, "AP Review", StageType::Review, true, Some(24)),
            stage(2, approval_stage, StageType::Approval, true, Some(48)),
            stage(3, "Ready for Payment", StageType::Payment, true, Some(24)),
        ],
    })
}

fn stage(
    order: i32,
    name: &str,
    stage_type: StageType,
    requires_action: bool,
    sla_hours: Option<i32>,
) -> WorkflowTemplateStage {
    WorkflowTemplateStage {
        order,
        name: name.to_string(),
        stage_type,
        queue_id: None,
        sla_hours,
        escalation_hours: sla_hours.map(|hours| hours * 2),
        requires_action,
        skip_conditions: Vec::new(),
        auto_advance_conditions: Vec::new(),
    }
}

fn default_state(started_at: DateTime<Utc>) -> ImplementationWizardState {
    ImplementationWizardState {
        started_at,
        phases: ImplementationPhases {
            erp: ErpPhase {
                status: PhaseStatus::NotStarted,
                provider: None,
                sub_items: ErpSubItems {
                    chart_of_accounts: false,
                    vendors: false,
                    open_pos: false,
                },
                last_sync: None,
                last_error: None,
            },
            approvals: ApprovalsPhase {
                status: PhaseStatus::NotStarted,
                template: None,
                template_id: None,
            },
            ocr: OcrPhase {
                status: PhaseStatus::NotStarted,
                count: 0,
                sample_invoice_ids: Vec::new(),
            },
            go_live: GoLivePhase {
                status: PhaseStatus::NotStarted,
                checks: GoLiveChecks {
                    notify_ap_team: false,
                    set_email_forwarding: false,
                    enable_approval_routing: false,
                    confirm_cutover_date: false,
                },
            },
        },
    }
}

fn recompute_statuses(state: &mut ImplementationWizardState) {
    let erp = &state.phases.erp.sub_items;
    state.phases.erp.status = if erp.chart_of_accounts && erp.vendors && erp.open_pos {
        PhaseStatus::Complete
    } else if erp.chart_of_accounts
        || erp.vendors
        || erp.open_pos
        || state.phases.erp.provider.is_some()
    {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };

    state.phases.approvals.status = if state.phases.approvals.template_id.is_some() {
        PhaseStatus::Complete
    } else if state.phases.approvals.template.is_some() {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };

    state.phases.ocr.count = state.phases.ocr.sample_invoice_ids.len().min(10) as u8;
    state.phases.ocr.status = if state.phases.ocr.count >= 10 {
        PhaseStatus::Complete
    } else if state.phases.ocr.count > 0 {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };

    state.phases.go_live.status = if go_live_complete(&state.phases.go_live.checks) {
        PhaseStatus::Complete
    } else if go_live_started(&state.phases.go_live.checks) {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };
}

fn status_response(mut state: ImplementationWizardState) -> ImplementationStatusResponse {
    recompute_statuses(&mut state);
    ImplementationStatusResponse {
        day_number: day_number(state.started_at),
        percent_complete: percent_complete(&state),
        started_at: state.started_at,
        phases: state.phases,
    }
}

fn day_number(started_at: DateTime<Utc>) -> u8 {
    let elapsed = Utc::now().signed_duration_since(started_at).num_days() + 1;
    elapsed.clamp(1, 14) as u8
}

fn percent_complete(state: &ImplementationWizardState) -> u8 {
    let complete = [
        state.phases.erp.status,
        state.phases.approvals.status,
        state.phases.ocr.status,
        state.phases.go_live.status,
    ]
    .into_iter()
    .filter(|status| *status == PhaseStatus::Complete)
    .count();
    ((complete * 100) / 4) as u8
}

fn go_live_started(checks: &GoLiveChecks) -> bool {
    checks.notify_ap_team
        || checks.set_email_forwarding
        || checks.enable_approval_routing
        || checks.confirm_cutover_date
}

fn go_live_complete(checks: &GoLiveChecks) -> bool {
    checks.notify_ap_team
        && checks.set_email_forwarding
        && checks.enable_approval_routing
        && checks.confirm_cutover_date
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_complete_tracks_completed_phases() {
        let mut state = default_state(Utc::now());
        state.phases.erp.sub_items = ErpSubItems {
            chart_of_accounts: true,
            vendors: true,
            open_pos: true,
        };
        state.phases.approvals.template_id = Some(Uuid::new_v4());
        recompute_statuses(&mut state);

        assert_eq!(percent_complete(&state), 50);
    }

    #[test]
    fn ocr_phase_completes_at_ten_samples() {
        let mut state = default_state(Utc::now());
        state.phases.ocr.sample_invoice_ids = (0..10).map(|_| Uuid::new_v4()).collect();
        recompute_statuses(&mut state);

        assert_eq!(state.phases.ocr.count, 10);
        assert_eq!(state.phases.ocr.status, PhaseStatus::Complete);
    }

    #[test]
    fn quickbooks_po_lines_map_amounts_to_cents() {
        let lines = quickbooks_po_lines(
            "PO-100",
            Some(&[QBPurchaseOrderLine {
                Id: Some("1".to_string()),
                LineNum: Some(3),
                Description: Some("Office chairs".to_string()),
                Amount: Some(250.75),
                ItemBasedExpenseLineDetail: Some(
                    billforge_quickbooks::QBItemBasedExpenseLineDetail {
                        ItemRef: None,
                        Qty: Some(5.0),
                        UnitPrice: Some(50.15),
                        TaxCodeRef: None,
                    },
                ),
                AccountBasedExpenseLineDetail: None,
            }]),
            "USD",
            &Money::new(25075, "USD"),
        );

        assert_eq!(lines[0].line_number, Some(3));
        assert_eq!(lines[0].quantity, 5.0);
        assert_eq!(lines[0].unit_price.amount, 5015);
        assert_eq!(lines[0].total.amount, 25075);
    }

    #[test]
    fn xero_purchase_order_open_filters_terminal_statuses() {
        let mut purchase_order = XeroPurchaseOrder {
            PurchaseOrderID: Some("po-id".to_string()),
            PurchaseOrderNumber: Some("PO-100".to_string()),
            Contact: None,
            Date: None,
            DeliveryDate: None,
            Status: Some("AUTHORISED".to_string()),
            LineItems: None,
            CurrencyCode: Some("USD".to_string()),
            Total: Some(10.0),
        };

        assert!(xero_purchase_order_open(&purchase_order));

        purchase_order.Status = Some("BILLED".to_string());
        assert!(!xero_purchase_order_open(&purchase_order));
    }
}
