//! Two-week implementation wizard orchestration.

use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthUser, InvoiceCaptureAccess, InvoiceProcessingAccess, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Multipart, State},
    routing::{get, patch, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{
        ActionType, CreatePOLineItemInput, CreatePurchaseOrderInput, CreateWorkflowTemplateInput,
        Invoice, InvoiceFilters, StageType, WorkflowRule, WorkflowRuleType, WorkflowTemplateStage,
    },
    traits::{
        InvoiceRepository, PurchaseOrderRepository, WorkflowRuleRepository,
        WorkflowTemplateRepository,
    },
    types::{Money, Pagination, TenantId, UserId},
    Error, Role,
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
use std::collections::HashSet;
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
        .route(
            "/backtest",
            get(get_readiness_backtest).post(run_readiness_backtest),
        )
        .route("/configuration/privacy-mode", put(update_privacy_mode))
        .route(
            "/configuration/capture-channels",
            put(update_capture_channels),
        )
        .route(
            "/configuration/capture-channels/email/verify",
            post(verify_email_forwarding),
        )
        .route(
            "/configuration/module-entitlements/ack",
            put(acknowledge_module_entitlements),
        )
        .route(
            "/configuration/notification-approvals",
            put(update_notification_approvals),
        )
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
    #[serde(default)]
    pub measured_accuracy: Option<f64>,
    #[serde(default = "default_accuracy_threshold")]
    pub accuracy_threshold: f64,
    #[serde(default)]
    pub total_extractions: i64,
    #[serde(default)]
    pub sufficient_sample: bool,
}

fn default_accuracy_threshold() -> f64 {
    billforge_invoice_capture::OCR_FIRST_PASS_ACCURACY_THRESHOLD
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoLiveChecks {
    pub confirm_cutover_date: bool,
    #[serde(default)]
    pub forwarding_email_verified: bool,
    #[serde(default)]
    pub sample_invoice_routed: bool,
    #[serde(default)]
    pub notifications_acknowledged: bool,
    #[serde(default)]
    pub privacy_mode_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoLivePhase {
    pub status: PhaseStatus,
    pub checks: GoLiveChecks,
    /// Cached go-live readiness backtest result. Populated by
    /// `POST /implementation/backtest` and surfaced in the getting-started UI.
    /// `#[serde(default)]` keeps existing wizard state rows deserializable.
    #[serde(default)]
    pub backtest_scorecard: Option<ReadinessScorecard>,
}

/// Go-live readiness scorecard produced by backtesting configured workflow
/// and categorization rules against the ERP-synced historical bill set.
///
/// `readiness_score` is a weighted average of the three coverage signals
/// (0.4 * auto_route + 0.4 * auto_approve + 0.2 * vendor_map). A score of
/// >= 0.75 marks the tenant ready for cutover.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadinessScorecard {
    /// Fraction of sample bills a workflow routing rule matched.
    pub auto_route_coverage: f32,
    /// Fraction of sample bills an auto-approval rule (or routing rule with
    /// an AutoApprove action) would have eligible.
    pub auto_approve_coverage: f32,
    /// Fraction of sample bills whose vendor was mapped in the ERP sync.
    pub vendor_map_coverage: f32,
    /// Number of historical bills the scorecard was computed over.
    pub sample_size: u32,
    /// Weighted readiness score in [0.0, 1.0].
    pub readiness_score: f32,
    /// Whether `readiness_score` clears the 0.75 go-live threshold.
    pub passes_threshold: bool,
    /// When the backtest was last run. `None` until the first run.
    #[serde(default)]
    pub run_at: Option<DateTime<Utc>>,
}

/// Weight applied to each coverage signal in the readiness score.
pub const READINESS_WEIGHT_AUTO_ROUTE: f32 = 0.4;
pub const READINESS_WEIGHT_AUTO_APPROVE: f32 = 0.4;
pub const READINESS_WEIGHT_VENDOR_MAP: f32 = 0.2;
/// Minimum weighted score a tenant must reach to pass the readiness gate.
pub const READINESS_THRESHOLD: f32 = 0.75;
/// Maximum number of historical bills replayed per backtest run.
pub const READINESS_BACKTEST_SAMPLE_CAP: u32 = 250;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PrivacyModeConfig {
    pub enabled: bool,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailForwardingConfig {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CaptureChannelsConfig {
    pub email_forwarding: EmailForwardingConfig,
    #[serde(default)]
    pub manual_upload_enabled: bool,
    #[serde(default)]
    pub erp_sync_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModuleEntitlement {
    pub module_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotificationApprovalsConfig {
    #[serde(default)]
    pub ap_team_distribution: Vec<String>,
    #[serde(default)]
    pub escalation_distribution: Vec<String>,
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationSection {
    pub privacy_mode: PrivacyModeConfig,
    pub capture_channels: CaptureChannelsConfig,
    pub module_entitlements: Vec<ModuleEntitlement>,
    pub notification_approvals: NotificationApprovalsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationPhase {
    pub status: PhaseStatus,
    pub configuration: ConfigurationSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImplementationPhases {
    pub erp: ErpPhase,
    pub approvals: ApprovalsPhase,
    pub ocr: OcrPhase,
    pub configuration: ConfigurationPhase,
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePrivacyModeRequest {
    pub enabled: bool,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCaptureChannelsRequest {
    pub email_forwarding_address: Option<String>,
    #[serde(default)]
    pub manual_upload_enabled: Option<bool>,
    #[serde(default)]
    pub erp_sync_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailForwardingRequest {
    pub verified: bool,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AckModuleEntitlementsRequest {
    pub entitlements: Vec<ModuleEntitlement>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNotificationApprovalsRequest {
    pub ap_team_distribution: Vec<String>,
    pub escalation_distribution: Vec<String>,
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
    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
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
    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
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
    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
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
    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
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
        let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
        return Ok(Json(SampleInvoiceUploadResponse {
            uploaded: Vec::new(),
            status: status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
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
            std::time::Instant::now(),
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
    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    Ok(Json(SampleInvoiceUploadResponse {
        uploaded,
        status: status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
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
    wizard.phases.go_live.checks.confirm_cutover_date = request.checks.confirm_cutover_date;

    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/implementation/backtest",
    tag = "Implementation",
    responses((status = 200, description = "Readiness backtest executed", body = ReadinessScorecard))
)]
pub async fn run_readiness_backtest(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<ReadinessScorecard>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;

    // Replay configured workflow rules against the historical bill set the
    // tenant already pulled during ERP sync. Cap the sample so a large tenant
    // does not stall the wizard. If there is nothing to replay, surface an
    // explicit zero-score card that fails the threshold rather than erroring.
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let invoices = invoice_repo
        .list(
            &tenant.tenant_id,
            &InvoiceFilters::default(),
            &Pagination {
                page: 1,
                per_page: READINESS_BACKTEST_SAMPLE_CAP,
            },
        )
        .await?;

    let rule_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let routing_rules = WorkflowRuleRepository::get_active_rules(
        &rule_repo,
        &tenant.tenant_id,
        WorkflowRuleType::Routing,
    )
    .await?;
    let auto_approval_rules = WorkflowRuleRepository::get_active_rules(
        &rule_repo,
        &tenant.tenant_id,
        WorkflowRuleType::AutoApproval,
    )
    .await?;

    let mapped_vendor_ids = load_mapped_vendor_ids(&pool, &tenant.tenant_id).await?;

    let scorecard = compute_readiness_scorecard(
        &invoices.data,
        &routing_rules,
        &auto_approval_rules,
        &mapped_vendor_ids,
        Some(Utc::now()),
    );

    wizard.phases.go_live.backtest_scorecard = Some(scorecard.clone());
    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;

    tracing::info!(
        tenant_id = %tenant.tenant_id,
        sample_size = scorecard.sample_size,
        auto_route_coverage = scorecard.auto_route_coverage,
        auto_approve_coverage = scorecard.auto_approve_coverage,
        vendor_map_coverage = scorecard.vendor_map_coverage,
        readiness_score = scorecard.readiness_score,
        passes_threshold = scorecard.passes_threshold,
        "Implementation wizard: readiness backtest executed"
    );

    Ok(Json(scorecard))
}

#[utoipa::path(
    get,
    path = "/api/v1/implementation/backtest",
    tag = "Implementation",
    responses((status = 200, description = "Cached readiness backtest scorecard", body = ReadinessScorecard))
)]
pub async fn get_readiness_backtest(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<ReadinessScorecard>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;
    Ok(Json(
        wizard
            .phases
            .go_live
            .backtest_scorecard
            .clone()
            .unwrap_or_else(empty_scorecard),
    ))
}

/// Compute the readiness scorecard by replaying routing / auto-approval
/// rules and vendor-map coverage across a sample of historical bills.
///
/// Pure function so unit tests can synthesize fake bills with stubbed rule
/// outcomes without touching the database.
pub fn compute_readiness_scorecard(
    invoices: &[Invoice],
    routing_rules: &[WorkflowRule],
    auto_approval_rules: &[WorkflowRule],
    mapped_vendor_ids: &HashSet<Uuid>,
    run_at: Option<DateTime<Utc>>,
) -> ReadinessScorecard {
    let sample_size = invoices.len() as u32;

    if invoices.is_empty() {
        return empty_scorecard_with(sample_size, run_at);
    }

    let mut auto_route_hits = 0u32;
    let mut auto_approve_hits = 0u32;
    let mut vendor_map_hits = 0u32;

    for invoice in invoices {
        let routed = routing_rules
            .iter()
            .any(|rule| evaluate_rule_conditions(invoice, rule));
        if routed {
            auto_route_hits += 1;
        }

        // Auto-approve eligible if an explicit auto-approval rule matches, or
        // a matched routing rule carries an AutoApprove action.
        let auto_approve_eligible = auto_approval_rules
            .iter()
            .any(|rule| evaluate_rule_conditions(invoice, rule))
            || routing_rules.iter().any(|rule| {
                rule.actions
                    .iter()
                    .any(|action| action.action_type == ActionType::AutoApprove)
                    && evaluate_rule_conditions(invoice, rule)
            });
        if auto_approve_eligible {
            auto_approve_hits += 1;
        }

        if invoice
            .vendor_id
            .map(|id| mapped_vendor_ids.contains(&id))
            .unwrap_or(false)
        {
            vendor_map_hits += 1;
        }
    }

    let total = sample_size as f32;
    let auto_route_coverage = auto_route_hits as f32 / total;
    let auto_approve_coverage = auto_approve_hits as f32 / total;
    let vendor_map_coverage = vendor_map_hits as f32 / total;
    let readiness_score = READINESS_WEIGHT_AUTO_ROUTE * auto_route_coverage
        + READINESS_WEIGHT_AUTO_APPROVE * auto_approve_coverage
        + READINESS_WEIGHT_VENDOR_MAP * vendor_map_coverage;

    ReadinessScorecard {
        auto_route_coverage,
        auto_approve_coverage,
        vendor_map_coverage,
        sample_size,
        readiness_score,
        passes_threshold: readiness_score >= READINESS_THRESHOLD,
        run_at,
    }
}

/// Evaluate a workflow rule's conditions against an invoice using the shared
/// `workflow_evaluator`. Empty conditions count as a match so a catch-all
/// rule is still exercised by the backtest, mirroring the engine behavior.
fn evaluate_rule_conditions(invoice: &Invoice, rule: &WorkflowRule) -> bool {
    if rule.conditions.is_empty() {
        return true;
    }
    billforge_core::workflow_evaluator::evaluate_conditions(invoice, &rule.conditions)
}

/// Scorecard returned when there is no historical sample to replay.
fn empty_scorecard() -> ReadinessScorecard {
    empty_scorecard_with(0, None)
}

fn empty_scorecard_with(sample_size: u32, run_at: Option<DateTime<Utc>>) -> ReadinessScorecard {
    ReadinessScorecard {
        auto_route_coverage: 0.0,
        auto_approve_coverage: 0.0,
        vendor_map_coverage: 0.0,
        sample_size,
        readiness_score: 0.0,
        passes_threshold: false,
        run_at,
    }
}

/// Load the union of ERP-mapped `billforge_vendor_id` values for the tenant.
/// Covers both QuickBooks (`quickbooks_vendor_mappings`) and Xero
/// (`xero_contact_mappings`) so the scorecard reflects whichever ERP the
/// tenant connected during the wizard.
async fn load_mapped_vendor_ids(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<HashSet<Uuid>, billforge_core::Error> {
    let mut ids = HashSet::new();
    for query in [
        "SELECT billforge_vendor_id FROM quickbooks_vendor_mappings WHERE tenant_id = $1",
        "SELECT billforge_vendor_id FROM xero_contact_mappings WHERE tenant_id = $1",
    ] {
        let rows: Vec<(Option<Uuid>,)> = sqlx::query_as(query)
            .bind(tenant_id.as_uuid())
            .fetch_all(pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to load vendor mappings: {}", e))
            })?;
        for (vendor_id,) in rows {
            if let Some(id) = vendor_id {
                ids.insert(id);
            }
        }
    }
    Ok(ids)
}

#[utoipa::path(
    put,
    path = "/api/v1/implementation/configuration/privacy-mode",
    tag = "Implementation",
    request_body = UpdatePrivacyModeRequest,
    responses((status = 200, description = "Privacy mode configuration updated", body = ImplementationStatusResponse))
)]
pub async fn update_privacy_mode(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<UpdatePrivacyModeRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    // Only tenant admins may change the privacy toggle
    if !user.has_role(Role::TenantAdmin) {
        return Err(ApiError(Error::Forbidden(
            "Only administrators can change privacy settings".to_string(),
        )));
    }

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;

    // Write through to the existing tenant privacy setting
    let mut settings = tenant.settings.clone();
    settings.features.local_ocr_required = request.enabled;
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| billforge_core::Error::Internal("DATABASE_URL missing".into()))?;
    let metadata_db = billforge_db::MetadataDatabase::new(&database_url)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to connect to metadata DB: {}", e))
        })?;
    metadata_db
        .update_tenant_settings(&tenant.tenant_id, &settings)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to persist privacy settings: {}", e))
        })?;

    wizard.phases.configuration.configuration.privacy_mode = PrivacyModeConfig {
        enabled: request.enabled,
        scope: request.scope,
        confirmed_at: Some(Utc::now()),
    };

    tracing::info!(
        tenant_id = %tenant.tenant_id,
        enabled = request.enabled,
        "Implementation wizard: privacy mode configured"
    );

    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
}

#[utoipa::path(
    put,
    path = "/api/v1/implementation/configuration/capture-channels",
    tag = "Implementation",
    request_body = UpdateCaptureChannelsRequest,
    responses((status = 200, description = "Capture channels updated", body = ImplementationStatusResponse))
)]
pub async fn update_capture_channels(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<UpdateCaptureChannelsRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;

    if let Some(address) = request.email_forwarding_address {
        wizard
            .phases
            .configuration
            .configuration
            .capture_channels
            .email_forwarding
            .address = address;
    }
    if let Some(manual_upload_enabled) = request.manual_upload_enabled {
        wizard
            .phases
            .configuration
            .configuration
            .capture_channels
            .manual_upload_enabled = manual_upload_enabled;
    }
    if let Some(erp_sync_enabled) = request.erp_sync_enabled {
        wizard
            .phases
            .configuration
            .configuration
            .capture_channels
            .erp_sync_enabled = erp_sync_enabled;
    }

    tracing::info!(
        tenant_id = %tenant.tenant_id,
        "Implementation wizard: capture channels updated"
    );

    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/implementation/configuration/capture-channels/email/verify",
    tag = "Implementation",
    request_body = VerifyEmailForwardingRequest,
    responses((status = 200, description = "Email forwarding verified", body = ImplementationStatusResponse))
)]
pub async fn verify_email_forwarding(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(_request): Json<VerifyEmailForwardingRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let metadata_pool = state.db.metadata();
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;

    // Query metadata DB for an actual inbound message for this tenant
    let inbound: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, received_at FROM inbound_email_messages WHERE tenant_id = $1 ORDER BY received_at DESC LIMIT 1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*metadata_pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query inbound messages: {}", e)))?;

    let Some((message_id, received_at)) = inbound else {
        return Err(billforge_core::Error::Validation(
            "No test email received yet. Send a test email to your forwarding address and try again.".to_string(),
        ).into());
    };

    wizard
        .phases
        .configuration
        .configuration
        .capture_channels
        .email_forwarding
        .verified_at = Some(Utc::now());

    tracing::info!(
        tenant_id = %tenant.tenant_id,
        inbound_message_id = %message_id,
        received_at = %received_at,
        "Implementation wizard: email forwarding verified via inbound message evidence"
    );

    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
}

#[utoipa::path(
    put,
    path = "/api/v1/implementation/configuration/module-entitlements/ack",
    tag = "Implementation",
    request_body = AckModuleEntitlementsRequest,
    responses((status = 200, description = "Module entitlements acknowledged", body = ImplementationStatusResponse))
)]
pub async fn acknowledge_module_entitlements(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(_request): Json<AckModuleEntitlementsRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;

    // Derive entitlements from the tenant's metadata DB enabled_modules,
    // ignoring any client-supplied values.
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| billforge_core::Error::Internal("DATABASE_URL missing".into()))?;
    let metadata_db = billforge_db::MetadataDatabase::new(&database_url)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to connect to metadata DB: {}", e))
        })?;

    let tenant_record = metadata_db
        .get_tenant(&tenant.tenant_id)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to load tenant: {}", e)))?;

    let server_entitlements: Vec<ModuleEntitlement> = if let Some(record) = tenant_record {
        let modules: Vec<billforge_core::Module> =
            serde_json::from_value(record.enabled_modules.0.clone()).unwrap_or_default();
        modules
            .into_iter()
            .map(|m| ModuleEntitlement {
                module_key: m.as_str().to_string(),
                enabled: true,
            })
            .collect()
    } else {
        Vec::new()
    };

    wizard
        .phases
        .configuration
        .configuration
        .module_entitlements = server_entitlements;

    tracing::info!(
        tenant_id = %tenant.tenant_id,
        "Implementation wizard: module entitlements acknowledged (server-derived)"
    );

    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
}

#[utoipa::path(
    put,
    path = "/api/v1/implementation/configuration/notification-approvals",
    tag = "Implementation",
    request_body = UpdateNotificationApprovalsRequest,
    responses((status = 200, description = "Notification approvals updated", body = ImplementationStatusResponse))
)]
pub async fn update_notification_approvals(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<UpdateNotificationApprovalsRequest>,
) -> ApiResult<Json<ImplementationStatusResponse>> {
    // Validate distributions are non-empty with at least one syntactically valid email each
    let validate_email =
        |s: &str| s.contains('@') && !s.trim().is_empty() && !s.chars().any(|c| c.is_whitespace());

    if request.ap_team_distribution.is_empty()
        || !request
            .ap_team_distribution
            .iter()
            .any(|e| validate_email(e))
    {
        return Err(billforge_core::Error::Validation(
            "At least one valid AP team distribution email is required.".to_string(),
        )
        .into());
    }
    if request.escalation_distribution.is_empty()
        || !request
            .escalation_distribution
            .iter()
            .any(|e| validate_email(e))
    {
        return Err(billforge_core::Error::Validation(
            "At least one valid escalation distribution email is required.".to_string(),
        )
        .into());
    }

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let mut wizard = load_or_create_state(&pool, &tenant.tenant_id).await?;

    wizard
        .phases
        .configuration
        .configuration
        .notification_approvals = NotificationApprovalsConfig {
        ap_team_distribution: request.ap_team_distribution,
        escalation_distribution: request.escalation_distribution,
        approved_at: Some(Utc::now()),
    };

    tracing::info!(
        tenant_id = %tenant.tenant_id,
        "Implementation wizard: notification approvals configured"
    );

    let routed = has_routed_invoice(&pool, &tenant.tenant_id).await;
    recompute_statuses(&mut wizard, routed);
    save_state(&pool, &tenant.tenant_id, &wizard).await?;
    Ok(Json(
        status_response_with_accuracy(&pool, &tenant.tenant_id, wizard, routed).await,
    ))
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
        recompute_statuses(&mut state, false);
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

pub(crate) async fn quickbooks_client(
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

pub fn default_state(started_at: DateTime<Utc>) -> ImplementationWizardState {
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
                measured_accuracy: None,
                accuracy_threshold: billforge_invoice_capture::OCR_FIRST_PASS_ACCURACY_THRESHOLD,
                total_extractions: 0,
                sufficient_sample: false,
            },
            configuration: ConfigurationPhase {
                status: PhaseStatus::NotStarted,
                configuration: ConfigurationSection {
                    privacy_mode: PrivacyModeConfig {
                        enabled: false,
                        scope: None,
                        confirmed_at: None,
                    },
                    capture_channels: CaptureChannelsConfig {
                        email_forwarding: EmailForwardingConfig {
                            address: String::new(),
                            verified_at: None,
                        },
                        manual_upload_enabled: false,
                        erp_sync_enabled: false,
                    },
                    module_entitlements: Vec::new(),
                    notification_approvals: NotificationApprovalsConfig {
                        ap_team_distribution: Vec::new(),
                        escalation_distribution: Vec::new(),
                        approved_at: None,
                    },
                },
            },
            go_live: GoLivePhase {
                status: PhaseStatus::NotStarted,
                checks: GoLiveChecks {
                    confirm_cutover_date: false,
                    forwarding_email_verified: false,
                    sample_invoice_routed: false,
                    notifications_acknowledged: false,
                    privacy_mode_confirmed: false,
                },
                backtest_scorecard: None,
            },
        },
    }
}

/// Check whether at least one invoice in the tenant has reached an approved/posted
/// status via the approval routing system. This is the measurable readiness signal
/// for the go-live `sample_invoice_routed` check.
pub async fn has_routed_invoice(pool: &sqlx::PgPool, tenant_id: &TenantId) -> bool {
    let result: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS(
            SELECT 1 FROM invoices i
            INNER JOIN approval_requests ar ON ar.invoice_id = i.id AND ar.tenant_id = i.tenant_id
            WHERE i.tenant_id = $1
            AND i.processing_status IN ('approved', 'ready_for_payment')
            AND ar.status = 'approved'
        )",
    )
    .bind(tenant_id.as_uuid())
    .fetch_one(pool)
    .await
    .ok();

    result.map_or(false, |(exists,)| exists)
}

pub fn recompute_statuses(state: &mut ImplementationWizardState, sample_invoice_routed: bool) {
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
    state.phases.ocr.status = if state.phases.ocr.count >= 10
        && state.phases.ocr.sufficient_sample
        && state.phases.ocr.measured_accuracy.unwrap_or(0.0) >= state.phases.ocr.accuracy_threshold
    {
        PhaseStatus::Complete
    } else if state.phases.ocr.count > 0 {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };

    // Configuration phase: derived from sub-sections
    let config = &state.phases.configuration.configuration;
    let privacy_done = config.privacy_mode.confirmed_at.is_some();
    let channels_done = config
        .capture_channels
        .email_forwarding
        .verified_at
        .is_some()
        || config.capture_channels.manual_upload_enabled
        || config.capture_channels.erp_sync_enabled;
    let modules_done = !config.module_entitlements.is_empty();
    let notifications_done = config.notification_approvals.approved_at.is_some();
    let config_done = privacy_done && channels_done && modules_done && notifications_done;
    let config_started = privacy_done || channels_done || modules_done || notifications_done;
    state.phases.configuration.status = if config_done {
        PhaseStatus::Complete
    } else if config_started {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };

    // Derive measurable go-live signals from observable state
    state.phases.go_live.checks.forwarding_email_verified = state
        .phases
        .configuration
        .configuration
        .capture_channels
        .email_forwarding
        .verified_at
        .is_some();
    state.phases.go_live.checks.notifications_acknowledged = state
        .phases
        .configuration
        .configuration
        .notification_approvals
        .approved_at
        .is_some();
    state.phases.go_live.checks.privacy_mode_confirmed = state
        .phases
        .configuration
        .configuration
        .privacy_mode
        .confirmed_at
        .is_some();
    state.phases.go_live.checks.sample_invoice_routed = sample_invoice_routed;

    state.phases.go_live.status = if go_live_complete(&state.phases.go_live.checks) {
        PhaseStatus::Complete
    } else if go_live_started(&state.phases.go_live.checks) {
        PhaseStatus::InProgress
    } else {
        PhaseStatus::NotStarted
    };
}

async fn status_response_with_accuracy(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    mut state: ImplementationWizardState,
    sample_invoice_routed: bool,
) -> ImplementationStatusResponse {
    // Fetch measured first-pass accuracy from calibration data
    let fpa = billforge_invoice_capture::tenant_first_pass_accuracy(pool, tenant_id, None)
        .await
        .unwrap_or_default();
    state.phases.ocr.measured_accuracy = fpa.accuracy;
    state.phases.ocr.total_extractions = fpa.total_extractions;
    state.phases.ocr.sufficient_sample = fpa.sufficient_sample;

    recompute_statuses(&mut state, sample_invoice_routed);
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

pub fn percent_complete(state: &ImplementationWizardState) -> u8 {
    let complete = [
        state.phases.erp.status,
        state.phases.approvals.status,
        state.phases.ocr.status,
        state.phases.configuration.status,
        state.phases.go_live.status,
    ]
    .into_iter()
    .filter(|status| *status == PhaseStatus::Complete)
    .count();
    ((complete * 100) / 5) as u8
}

fn go_live_started(checks: &GoLiveChecks) -> bool {
    checks.confirm_cutover_date
        || checks.forwarding_email_verified
        || checks.sample_invoice_routed
        || checks.notifications_acknowledged
        || checks.privacy_mode_confirmed
}

fn go_live_complete(checks: &GoLiveChecks) -> bool {
    checks.confirm_cutover_date
        && checks.forwarding_email_verified
        && checks.sample_invoice_routed
        && checks.notifications_acknowledged
        && checks.privacy_mode_confirmed
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
        recompute_statuses(&mut state, false);

        // 2 of 5 phases complete = 40%
        assert_eq!(percent_complete(&state), 40);
    }

    #[test]
    fn ocr_phase_remains_in_progress_without_accuracy_gate() {
        let mut state = default_state(Utc::now());
        state.phases.ocr.sample_invoice_ids = (0..10).map(|_| Uuid::new_v4()).collect();
        // No calibration data => sufficient_sample == false, measured_accuracy == None
        recompute_statuses(&mut state, false);

        assert_eq!(state.phases.ocr.count, 10);
        assert_eq!(state.phases.ocr.status, PhaseStatus::InProgress);
        assert!(!state.phases.ocr.sufficient_sample);
        assert!(state.phases.ocr.measured_accuracy.is_none());
    }

    #[test]
    fn ocr_phase_completes_when_accuracy_gate_met() {
        let mut state = default_state(Utc::now());
        state.phases.ocr.sample_invoice_ids = (0..10).map(|_| Uuid::new_v4()).collect();
        // Simulate sufficient extractions with high accuracy (>= 90%)
        state.phases.ocr.sufficient_sample = true;
        state.phases.ocr.measured_accuracy = Some(0.95);
        state.phases.ocr.total_extractions = 50;
        recompute_statuses(&mut state, false);

        assert_eq!(state.phases.ocr.status, PhaseStatus::Complete);
    }

    #[test]
    fn ocr_phase_blocked_by_low_accuracy() {
        let mut state = default_state(Utc::now());
        state.phases.ocr.sample_invoice_ids = (0..10).map(|_| Uuid::new_v4()).collect();
        // Sufficient extractions but accuracy below 90%
        state.phases.ocr.sufficient_sample = true;
        state.phases.ocr.measured_accuracy = Some(0.85);
        state.phases.ocr.total_extractions = 40;
        recompute_statuses(&mut state, false);

        assert_eq!(state.phases.ocr.status, PhaseStatus::InProgress);
        assert_eq!(state.phases.ocr.measured_accuracy, Some(0.85));
        assert!(state.phases.ocr.measured_accuracy.unwrap() < state.phases.ocr.accuracy_threshold);
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

    #[test]
    fn backtest_with_no_history_returns_zero_score_and_fails_threshold() {
        // No historical bills at all => zero coverage on every signal and the
        // tenant cannot pass the readiness gate, regardless of rules.
        let routing = vec![sample_routing_rule(10_000)];
        let auto_approval = vec![sample_auto_approval_rule(5_000)];
        let mapped: HashSet<Uuid> = HashSet::new();

        let scorecard =
            compute_readiness_scorecard(&[], &routing, &auto_approval, &mapped, Some(Utc::now()));

        assert_eq!(scorecard.sample_size, 0);
        assert_eq!(scorecard.auto_route_coverage, 0.0);
        assert_eq!(scorecard.auto_approve_coverage, 0.0);
        assert_eq!(scorecard.vendor_map_coverage, 0.0);
        assert_eq!(scorecard.readiness_score, 0.0);
        assert!(!scorecard.passes_threshold);
    }

    #[test]
    fn backtest_weighted_score_passes_when_three_signals_high() {
        // Synthesize ten small bills (2500 cents). All match both the routing
        // rule (amount < 10000) and the auto-approval rule (amount < 5000).
        // Eight have a mapped vendor, two do not. Expected weighted score:
        //   0.4 * 1.0 + 0.4 * 1.0 + 0.2 * 0.8 = 0.96 -> passes the 0.75 gate.
        let mapped_vendor = Uuid::new_v4();
        let unmapped_vendor = Uuid::new_v4();
        let mut mapped: HashSet<Uuid> = HashSet::new();
        mapped.insert(mapped_vendor);

        let mut invoices = Vec::new();
        for idx in 0..10 {
            let vendor_id = if idx < 8 {
                mapped_vendor
            } else {
                unmapped_vendor
            };
            invoices.push(sample_invoice(vendor_id, 2_500));
        }

        let routing = vec![sample_routing_rule(10_000)];
        let auto_approval = vec![sample_auto_approval_rule(5_000)];

        let scorecard = compute_readiness_scorecard(
            &invoices,
            &routing,
            &auto_approval,
            &mapped,
            Some(Utc::now()),
        );

        assert_eq!(scorecard.sample_size, 10);
        assert_eq!(scorecard.auto_route_coverage, 1.0);
        assert_eq!(scorecard.auto_approve_coverage, 1.0);
        assert!((scorecard.vendor_map_coverage - 0.8).abs() < 1e-6);
        let expected_score = READINESS_WEIGHT_AUTO_ROUTE * 1.0
            + READINESS_WEIGHT_AUTO_APPROVE * 1.0
            + READINESS_WEIGHT_VENDOR_MAP * 0.8;
        assert!((scorecard.readiness_score - expected_score).abs() < 1e-6);
        assert!(
            scorecard.passes_threshold,
            "expected score {} to clear threshold {}",
            scorecard.readiness_score, READINESS_THRESHOLD
        );
    }

    /// Build a workflow routing rule that matches invoices below an amount
    /// threshold (in cents) and routes them to a queue.
    fn sample_routing_rule(amount_cents: i64) -> WorkflowRule {
        use billforge_core::domain::{
            ConditionField, ConditionOperator, RuleAction, RuleCondition, WorkflowRuleId,
        };
        WorkflowRule {
            id: WorkflowRuleId(Uuid::new_v4()),
            tenant_id: TenantId::new(),
            name: "Backtest route-below-amount".to_string(),
            description: None,
            priority: 10,
            is_active: true,
            rule_type: WorkflowRuleType::Routing,
            conditions: vec![RuleCondition {
                field: ConditionField::Amount,
                operator: ConditionOperator::LessThan,
                value: serde_json::json!(amount_cents),
            }],
            actions: vec![RuleAction {
                action_type: ActionType::RouteToQueue,
                params: serde_json::json!({ "queue_id": "ap-review" }),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Build an auto-approval rule that matches invoices below an amount
    /// threshold (in cents).
    fn sample_auto_approval_rule(amount_cents: i64) -> WorkflowRule {
        use billforge_core::domain::{
            ConditionField, ConditionOperator, RuleAction, RuleCondition, WorkflowRuleId,
        };
        WorkflowRule {
            id: WorkflowRuleId(Uuid::new_v4()),
            tenant_id: TenantId::new(),
            name: "Backtest small-invoice auto-approve".to_string(),
            description: None,
            priority: 20,
            is_active: true,
            rule_type: WorkflowRuleType::AutoApproval,
            conditions: vec![RuleCondition {
                field: ConditionField::Amount,
                operator: ConditionOperator::LessThan,
                value: serde_json::json!(amount_cents),
            }],
            actions: vec![RuleAction {
                action_type: ActionType::AutoApprove,
                params: serde_json::json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Build a minimal invoice for the backtest tests. Only the fields the
    /// rule evaluator reads (total amount + vendor id) are populated.
    fn sample_invoice(vendor_id: Uuid, amount_cents: i64) -> Invoice {
        use billforge_core::domain::{CaptureStatus, InvoiceId, ProcessingStatus};
        Invoice {
            id: InvoiceId::new(),
            tenant_id: TenantId::new(),
            vendor_id: Some(vendor_id),
            vendor_name: format!("Vendor {}", vendor_id),
            invoice_number: Uuid::new_v4().to_string(),
            invoice_date: Some(Utc::now().date_naive()),
            due_date: None,
            po_number: None,
            subtotal: Some(Money::new(amount_cents, "USD".to_string())),
            tax_amount: None,
            total_amount: Money::new(amount_cents, "USD".to_string()),
            currency: "USD".to_string(),
            line_items: Vec::new(),
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::new_v4(),
            supporting_documents: Vec::new(),
            ocr_confidence: None,
            categorization_confidence: None,
            department: None,
            gl_code: None,
            cost_center: None,
            notes: None,
            tags: Vec::new(),
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
