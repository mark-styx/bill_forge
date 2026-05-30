//! Intelligent routing API endpoints
//!
//! Provides endpoints for:
//! - Getting a routing decision for an invoice
//! - Viewing workload distribution stats
//! - Setting approver availability
//! - Updating routing configuration

use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    intelligent_routing::{
        IntelligentRoutingEngine, RoutingConfig, RoutingDecision, SimulationInput,
        SimulationSummary, SimulatedOutcome, simulate_routing,
    },
    workload_balancer::{WorkloadBalancer, WorkloadBalancerConfig, WorkloadDistributionStats},
};
use billforge_db::routing_repository::{AvailabilityStatusInput, SetAvailabilityInput};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/invoices/:invoice_id/route", post(route_invoice))
        .route("/workload", get(get_workload_stats))
        .route("/availability", post(set_availability))
        .route(
            "/config",
            get(get_routing_config).put(update_routing_config),
        )
        .route("/simulate", post(simulate_routing_handler))
}

/// Request body for routing an invoice
#[derive(Debug, Deserialize)]
struct RouteInvoiceRequest {
    queue_id: Uuid,
}

/// Response for a routing decision
#[derive(Debug, Serialize)]
struct RouteInvoiceResponse {
    decision: RoutingDecision,
}

/// Get a routing decision for an invoice
#[utoipa::path(post, path = "/api/v1/routing/invoices/{invoice_id}/route", tag = "Routing", request_body = serde_json::Value,
    params(("invoice_id" = String, Path, description = "Invoice ID")),
    responses((status = 200, description = "Routing decision"), (status = 404, description = "Invoice not found")))]
async fn route_invoice(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, _tenant): InvoiceProcessingAccess,
    Path(invoice_id): Path<Uuid>,
    Json(body): Json<RouteInvoiceRequest>,
) -> Result<Json<RouteInvoiceResponse>, StatusCode> {
    let tenant_id = &user.tenant_id;

    // Get the tenant DB pool
    let tenant_pool = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let routing_repo = billforge_db::RoutingRepository::new((*tenant_pool).clone());

    // Load routing config
    let config = routing_repo
        .get_routing_config(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get routing config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Load routing context (workloads, availability, expertise)
    let context = routing_repo
        .get_routing_context(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get routing context: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Fetch the invoice
    let invoice_repo = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Query the invoice directly
    let invoice_row = sqlx::query_as::<_, InvoiceMinRow>(
        "SELECT id, vendor_id, vendor_name, total_amount_cents, department, gl_code FROM invoices WHERE id = $1",
    )
    .bind(invoice_id)
    .fetch_optional(&*invoice_repo)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch invoice: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let invoice_row = invoice_row.ok_or(StatusCode::NOT_FOUND)?;

    // Build a minimal invoice for routing
    let invoice = invoice_row.into_invoice(tenant_id);

    // Run the routing engine
    let engine = IntelligentRoutingEngine::new(config);
    let decision = context.route(&engine, &invoice);

    // Log the routing decision
    if let Err(e) = routing_repo
        .log_routing_decision(tenant_id, invoice_id, body.queue_id, &decision)
        .await
    {
        tracing::warn!("Failed to log routing decision: {}", e);
    }

    Ok(Json(RouteInvoiceResponse { decision }))
}

/// Workload stats response
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkloadResponse {
    stats: WorkloadDistributionStatsResponse,
    approvers: Vec<ApproverWorkloadSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkloadDistributionStatsResponse {
    average_workload: f64,
    max_workload: f64,
    min_workload: f64,
    std_deviation: f64,
    variance_coefficient: f64,
    overloaded_count: i32,
    underloaded_count: i32,
}

impl From<WorkloadDistributionStats> for WorkloadDistributionStatsResponse {
    fn from(stats: WorkloadDistributionStats) -> Self {
        Self {
            average_workload: stats.average_workload,
            max_workload: stats.max_workload,
            min_workload: stats.min_workload,
            std_deviation: stats.std_deviation,
            variance_coefficient: stats.variance_coefficient,
            overloaded_count: stats.overloaded_count,
            underloaded_count: stats.underloaded_count,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ApproverWorkloadSummary {
    user_id: Uuid,
    active_approvals: i32,
    pending_approvals: i32,
    completed_this_week: i32,
    workload_score: f64,
}

/// Get workload distribution statistics
#[utoipa::path(get, path = "/api/v1/routing/workload", tag = "Routing",
    responses((status = 200, description = "Workload stats", body = WorkloadResponse)))]
async fn get_workload_stats(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, _tenant): InvoiceProcessingAccess,
) -> Result<Json<WorkloadResponse>, StatusCode> {
    let tenant_id = &user.tenant_id;

    let tenant_pool = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let routing_repo = billforge_db::RoutingRepository::new((*tenant_pool).clone());
    let workloads = routing_repo.get_workloads(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get workloads: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let balancer = WorkloadBalancer::new(WorkloadBalancerConfig::default());
    let stats = balancer.calculate_distribution_stats(&workloads);

    let approvers: Vec<ApproverWorkloadSummary> = workloads
        .iter()
        .map(|w| ApproverWorkloadSummary {
            user_id: w.user_id.0,
            active_approvals: w.active_approvals,
            pending_approvals: w.pending_approvals,
            completed_this_week: w.completed_this_week,
            workload_score: w.workload_score,
        })
        .collect();

    Ok(Json(WorkloadResponse {
        stats: stats.into(),
        approvers,
    }))
}

/// Request body for setting availability
#[derive(Debug, Deserialize)]
struct SetAvailabilityRequest {
    user_id: Uuid,
    status: AvailabilityStatusInput,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    delegate_id: Option<Uuid>,
    reason: Option<String>,
}

/// Set approver availability (admin or self only)
#[utoipa::path(post, path = "/api/v1/routing/availability", tag = "Routing", request_body = serde_json::Value,
    responses((status = 204, description = "Availability set"), (status = 403, description = "Forbidden")))]
async fn set_availability(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, _tenant): InvoiceProcessingAccess,
    Json(body): Json<SetAvailabilityRequest>,
) -> Result<StatusCode, StatusCode> {
    // Users can set their own availability; admins can set anyone's
    if body.user_id != user.user_id.0 && !user.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let tenant_id = &user.tenant_id;

    let tenant_pool = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let routing_repo = billforge_db::RoutingRepository::new((*tenant_pool).clone());

    let input = SetAvailabilityInput {
        user_id: body.user_id,
        status: body.status,
        start_at: body.start_at,
        end_at: body.end_at,
        delegate_id: body.delegate_id,
        reason: body.reason,
    };

    routing_repo
        .set_availability(tenant_id, &input)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set availability: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Routing config response
#[derive(Debug, Serialize, Deserialize)]
struct RoutingConfigResponse {
    workload_weight: f64,
    expertise_weight: f64,
    availability_weight: f64,
    max_workload_score: f64,
    min_expertise_score: f64,
    enable_auto_delegation: bool,
    enable_pattern_learning: bool,
    enable_calendar_sync: bool,
    working_hours_start: String,
    working_hours_end: String,
    working_timezone: String,
    working_days: Vec<i32>,
}

/// Get routing configuration
#[utoipa::path(get, path = "/api/v1/routing/config", tag = "Routing",
    responses((status = 200, description = "Routing configuration")))]
async fn get_routing_config(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, _tenant): InvoiceProcessingAccess,
) -> Result<Json<RoutingConfigResponse>, StatusCode> {
    let tenant_id = &user.tenant_id;

    let tenant_pool = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let routing_repo = billforge_db::RoutingRepository::new((*tenant_pool).clone());
    let config = routing_repo
        .get_routing_config(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get routing config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(RoutingConfigResponse {
        workload_weight: config.workload_weight,
        expertise_weight: config.expertise_weight,
        availability_weight: config.availability_weight,
        max_workload_score: config.max_workload_score,
        min_expertise_score: config.min_expertise_score,
        enable_auto_delegation: config.enable_auto_delegation,
        enable_pattern_learning: config.enable_pattern_learning,
        enable_calendar_sync: config.enable_calendar_sync,
        working_hours_start: config.working_hours_start.format("%H:%M:%S").to_string(),
        working_hours_end: config.working_hours_end.format("%H:%M:%S").to_string(),
        working_timezone: config.working_timezone,
        working_days: config.working_days,
    }))
}

/// Request body for updating routing configuration
#[derive(Debug, Deserialize)]
struct UpdateRoutingConfigRequest {
    workload_weight: Option<f64>,
    expertise_weight: Option<f64>,
    availability_weight: Option<f64>,
    max_workload_score: Option<f64>,
    min_expertise_score: Option<f64>,
    enable_auto_delegation: Option<bool>,
    enable_pattern_learning: Option<bool>,
    enable_calendar_sync: Option<bool>,
    working_hours_start: Option<String>,
    working_hours_end: Option<String>,
    working_timezone: Option<String>,
    working_days: Option<Vec<i32>>,
}

/// Update routing configuration (admin only)
#[utoipa::path(put, path = "/api/v1/routing/config", tag = "Routing", request_body = serde_json::Value,
    responses((status = 200, description = "Configuration updated"), (status = 403, description = "Admin only")))]
async fn update_routing_config(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, _tenant): InvoiceProcessingAccess,
    Json(body): Json<UpdateRoutingConfigRequest>,
) -> Result<Json<RoutingConfigResponse>, StatusCode> {
    if !user.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let tenant_id = &user.tenant_id;

    let tenant_pool = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let routing_repo = billforge_db::RoutingRepository::new((*tenant_pool).clone());

    // Load existing config as base
    let mut config = routing_repo
        .get_routing_config(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get routing config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Apply updates
    if let Some(w) = body.workload_weight {
        config.workload_weight = w;
    }
    if let Some(w) = body.expertise_weight {
        config.expertise_weight = w;
    }
    if let Some(w) = body.availability_weight {
        config.availability_weight = w;
    }
    if let Some(w) = body.max_workload_score {
        config.max_workload_score = w;
    }
    if let Some(w) = body.min_expertise_score {
        config.min_expertise_score = w;
    }
    if let Some(v) = body.enable_auto_delegation {
        config.enable_auto_delegation = v;
    }
    if let Some(v) = body.enable_pattern_learning {
        config.enable_pattern_learning = v;
    }
    if let Some(v) = body.enable_calendar_sync {
        config.enable_calendar_sync = v;
    }
    if let Some(ref t) = body.working_hours_start {
        config.working_hours_start = t.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    }
    if let Some(ref t) = body.working_hours_end {
        config.working_hours_end = t.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    }
    if let Some(ref tz) = body.working_timezone {
        config.working_timezone = tz.clone();
    }
    if let Some(ref days) = body.working_days {
        config.working_days = days.clone();
    }

    // Save
    routing_repo
        .upsert_routing_config(&config)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update routing config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(RoutingConfigResponse {
        workload_weight: config.workload_weight,
        expertise_weight: config.expertise_weight,
        availability_weight: config.availability_weight,
        max_workload_score: config.max_workload_score,
        min_expertise_score: config.min_expertise_score,
        enable_auto_delegation: config.enable_auto_delegation,
        enable_pattern_learning: config.enable_pattern_learning,
        enable_calendar_sync: config.enable_calendar_sync,
        working_hours_start: config.working_hours_start.format("%H:%M:%S").to_string(),
        working_hours_end: config.working_hours_end.format("%H:%M:%S").to_string(),
        working_timezone: config.working_timezone,
        working_days: config.working_days,
    }))
}

// ---------------------------------------------------------------------------
// Simulation endpoint (what-if rule testing, issue #246)
// ---------------------------------------------------------------------------

/// JSON request body for `POST /api/v1/routing/simulate`.
#[derive(Debug, Deserialize)]
struct SimulateRequest {
    /// Candidate routing configuration to test against.
    candidate_config: SimulationConfigInput,
    /// Number of recent invoices to replay (capped at 500).
    sample_size: Option<usize>,
}

/// JSON-serializable routing config (mirrors RoutingConfig but uses string times).
#[derive(Debug, Deserialize)]
struct SimulationConfigInput {
    workload_weight: Option<f64>,
    expertise_weight: Option<f64>,
    availability_weight: Option<f64>,
    max_workload_score: Option<f64>,
    min_expertise_score: Option<f64>,
    enable_auto_delegation: Option<bool>,
    enable_pattern_learning: Option<bool>,
    enable_calendar_sync: Option<bool>,
    working_hours_start: Option<String>,
    working_hours_end: Option<String>,
    working_timezone: Option<String>,
    working_days: Option<Vec<i32>>,
}

/// JSON response for a single invoice's simulated outcome.
#[derive(Debug, Serialize)]
struct SimulatedOutcomeResponse {
    invoice_id: Uuid,
    predicted_approver: Option<Uuid>,
    live_approver: Option<Uuid>,
    changed: bool,
    predicted_cycle_hours: f64,
    live_cycle_hours: f64,
    would_stall: bool,
}

/// JSON response for the full simulation summary.
#[derive(Debug, Serialize)]
struct SimulationSummaryResponse {
    outcomes: Vec<SimulatedOutcomeResponse>,
    approver_load_candidate: HashMap<Uuid, u32>,
    approver_load_live: HashMap<Uuid, u32>,
    avg_cycle_hours_candidate: f64,
    avg_cycle_hours_live: f64,
    stalled_count_candidate: u32,
    stalled_count_live: u32,
    changed_count: u32,
    total_simulated: u32,
}

impl From<SimulationSummary> for SimulationSummaryResponse {
    fn from(s: SimulationSummary) -> Self {
        Self {
            outcomes: s
                .outcomes
                .into_iter()
                .map(|o| SimulatedOutcomeResponse {
                    invoice_id: o.invoice_id,
                    predicted_approver: o.predicted_approver.map(|u| u.0),
                    live_approver: o.live_approver.map(|u| u.0),
                    changed: o.changed,
                    predicted_cycle_hours: o.predicted_cycle_hours,
                    live_cycle_hours: o.live_cycle_hours,
                    would_stall: o.would_stall,
                })
                .collect(),
            approver_load_candidate: s
                .approver_load_candidate
                .into_iter()
                .map(|(k, v)| (k.0, v))
                .collect(),
            approver_load_live: s
                .approver_load_live
                .into_iter()
                .map(|(k, v)| (k.0, v))
                .collect(),
            avg_cycle_hours_candidate: s.avg_cycle_hours_candidate,
            avg_cycle_hours_live: s.avg_cycle_hours_live,
            stalled_count_candidate: s.stalled_count_candidate,
            stalled_count_live: s.stalled_count_live,
            changed_count: s.changed_count,
            total_simulated: s.total_simulated,
        }
    }
}

/// Run a what-if simulation: replay the last N invoices through both the live and
/// a candidate routing config and return a diff.
#[utoipa::path(
    post,
    path = "/api/v1/routing/simulate",
    tag = "Routing",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Simulation summary"),
        (status = 400, description = "Invalid candidate config")
    )
)]
async fn simulate_routing_handler(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, _tenant): InvoiceProcessingAccess,
    Json(body): Json<SimulateRequest>,
) -> Result<Json<SimulationSummaryResponse>, StatusCode> {
    let tenant_id = &user.tenant_id;

    // Cap sample size at 500, default 200.
    let sample_size = body.sample_size.unwrap_or(200).min(500).max(1);

    // Get tenant DB pool.
    let tenant_pool = state.db.tenant(tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let routing_repo = billforge_db::RoutingRepository::new((*tenant_pool).clone());

    // Load live routing config.
    let live_config = routing_repo
        .get_routing_config(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get live routing config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Build candidate config by overlaying the provided fields on top of the live config.
    let mut candidate_config = live_config.clone();
    if let Some(w) = body.candidate_config.workload_weight {
        candidate_config.workload_weight = w;
    }
    if let Some(w) = body.candidate_config.expertise_weight {
        candidate_config.expertise_weight = w;
    }
    if let Some(w) = body.candidate_config.availability_weight {
        candidate_config.availability_weight = w;
    }
    if let Some(w) = body.candidate_config.max_workload_score {
        candidate_config.max_workload_score = w;
    }
    if let Some(w) = body.candidate_config.min_expertise_score {
        candidate_config.min_expertise_score = w;
    }
    if let Some(v) = body.candidate_config.enable_auto_delegation {
        candidate_config.enable_auto_delegation = v;
    }
    if let Some(v) = body.candidate_config.enable_pattern_learning {
        candidate_config.enable_pattern_learning = v;
    }
    if let Some(v) = body.candidate_config.enable_calendar_sync {
        candidate_config.enable_calendar_sync = v;
    }
    if let Some(ref t) = body.candidate_config.working_hours_start {
        candidate_config.working_hours_start = t.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    }
    if let Some(ref t) = body.candidate_config.working_hours_end {
        candidate_config.working_hours_end = t.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    }
    if let Some(ref tz) = body.candidate_config.working_timezone {
        candidate_config.working_timezone = tz.clone();
    }
    if let Some(ref days) = body.candidate_config.working_days {
        candidate_config.working_days = days.clone();
    }

    // Load routing context (workloads, availability, expertise).
    let context = routing_repo
        .get_routing_context(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get routing context: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Fetch last N invoices for replay.
    let invoice_rows = sqlx::query_as::<_, InvoiceMinRow>(
        "SELECT id, vendor_id, vendor_name, total_amount_cents, department, gl_code \
         FROM invoices \
         WHERE tenant_id = $1 \
         ORDER BY created_at DESC \
         LIMIT $2",
    )
    .bind(tenant_id.0)
    .bind(sample_size as i64)
    .fetch_all(&*tenant_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch sample invoices: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let invoices: Vec<billforge_core::domain::Invoice> = invoice_rows
        .into_iter()
        .map(|row| row.into_invoice(tenant_id))
        .collect();

    // Build workload HashMap for stall detection.
    let workloads = context.workloads.clone();

    // Run simulation.
    let engine_live = IntelligentRoutingEngine::new(live_config);
    let engine_candidate = IntelligentRoutingEngine::new(candidate_config);

    let summary = simulate_routing(&engine_live, &engine_candidate, &invoices, &context, &workloads);

    Ok(Json(summary.into()))
}

// Minimal invoice row for routing - avoids pulling in full domain type
#[derive(sqlx::FromRow)]
struct InvoiceMinRow {
    id: Uuid,
    vendor_id: Option<Uuid>,
    vendor_name: String,
    total_amount_cents: i64,
    department: Option<String>,
    gl_code: Option<String>,
}

impl InvoiceMinRow {
    fn into_invoice(self, tenant_id: &billforge_core::TenantId) -> billforge_core::domain::Invoice {
        use billforge_core::domain::*;
        use billforge_core::types::Money;
        Invoice {
            id: InvoiceId(self.id),
            tenant_id: tenant_id.clone(),
            vendor_id: self.vendor_id,
            vendor_name: self.vendor_name,
            invoice_number: String::new(),
            invoice_date: None,
            due_date: None,
            po_number: None,
            subtotal: None,
            tax_amount: None,
            total_amount: Money {
                amount: self.total_amount_cents,
                currency: "USD".to_string(),
            },
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::nil(),
            supporting_documents: vec![],
            ocr_confidence: None,
            categorization_confidence: None,
            department: self.department,
            gl_code: self.gl_code,
            cost_center: None,
            notes: None,
            tags: vec![],
            custom_fields: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: Some(billforge_core::UserId(Uuid::nil())),
        }
    }
}

// Need RoutingDataProvider in scope for the trait method
use billforge_core::intelligent_routing::RoutingDataProvider;
