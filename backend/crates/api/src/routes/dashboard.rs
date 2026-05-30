//! Dashboard metrics and analytics endpoints
//!
//! Provides aggregated metrics for the analytics dashboard including:
//! - Invoice processing metrics
//! - Approval workflow metrics
//! - Vendor analytics
//! - Team performance metrics
//! - Stage dwell time bottleneck heat map
//! - Approver workload distribution
//! - Exception rate trend chart

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use billforge_core::Error;
use billforge_db::repositories::MetricsRepositoryImpl;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(get_dashboard_metrics))
        .route("/metrics/invoices", get(get_invoice_metrics))
        .route("/metrics/approvals", get(get_approval_metrics))
        .route("/metrics/vendors", get(get_vendor_metrics))
        .route("/metrics/team", get(get_team_metrics))
        .route("/stage-dwell", get(get_stage_dwell))
        .route("/approver-workload", get(get_approver_workload))
        .route("/exception-trend", get(get_exception_trend))
}

/// Dashboard metrics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DashboardMetrics {
    /// Invoice processing metrics
    pub invoices: InvoiceMetrics,
    /// Approval workflow metrics
    pub approvals: ApprovalMetrics,
    /// Vendor analytics
    pub vendors: VendorMetrics,
    /// Team performance metrics
    pub team: TeamMetrics,
}

/// Invoice processing metrics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InvoiceMetrics {
    /// Total invoices processed
    pub total_invoices: u64,
    /// Invoices pending OCR
    pub pending_ocr: u64,
    /// Invoices ready for review
    pub ready_for_review: u64,
    /// Invoices submitted for approval
    pub submitted: u64,
    /// Invoices approved
    pub approved: u64,
    /// Invoices rejected
    pub rejected: u64,
    /// Invoices paid
    pub paid: u64,
    /// Average processing time in hours
    pub avg_processing_time_hours: f64,
    /// Total invoice value (in cents)
    pub total_value: i64,
    /// Invoices processed this month
    pub this_month: u64,
    /// Processing trend vs last month (%)
    pub trend_vs_last_month: f64,
}

/// Approval workflow metrics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApprovalMetrics {
    /// Pending approval requests
    pub pending_approvals: u64,
    /// Approved today
    pub approved_today: u64,
    /// Rejected today
    pub rejected_today: u64,
    /// Average approval time in hours
    pub avg_approval_time_hours: f64,
    /// Approval rate (%)
    pub approval_rate: f64,
    /// Escalated requests
    pub escalated: u64,
    /// Overdue approvals (past SLA)
    pub overdue: u64,
}

/// Vendor analytics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VendorMetrics {
    /// Total active vendors
    pub total_vendors: u64,
    /// New vendors this month
    pub new_this_month: u64,
    /// Top 5 vendors by invoice count
    pub top_vendors: Vec<TopVendor>,
    /// Vendor concentration (top 10% of vendors by spend)
    pub concentration_percentage: f64,
}

/// Top vendor by invoice count
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TopVendor {
    /// Vendor ID
    pub vendor_id: String,
    /// Vendor name
    pub vendor_name: String,
    /// Invoice count
    pub invoice_count: u64,
    /// Total spend (in cents)
    pub total_amount: i64,
}

/// Team performance metrics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TeamMetrics {
    /// Team member performance stats
    pub members: Vec<TeamMemberStats>,
    /// Average approvals per team member per day
    pub avg_approvals_per_member: f64,
    /// Total pending actions across team
    pub total_pending_actions: u64,
}

/// Team member statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TeamMemberStats {
    /// User ID
    pub user_id: String,
    /// User name
    pub user_name: String,
    /// Approvals this month
    pub approvals_this_month: u64,
    /// Rejections this month
    pub rejections_this_month: u64,
    /// Average response time in hours
    pub avg_response_time_hours: f64,
}

/// Get comprehensive dashboard metrics
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/metrics",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Dashboard metrics", body = DashboardMetrics),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_dashboard_metrics(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let metrics_repo = MetricsRepositoryImpl::new(pool);

    // Fetch all metrics in parallel
    let (invoice_metrics, approval_metrics, vendor_metrics, team_metrics) = tokio::try_join!(
        metrics_repo.get_invoice_metrics(&tenant.tenant_id),
        metrics_repo.get_approval_metrics(&tenant.tenant_id),
        metrics_repo.get_vendor_metrics(&tenant.tenant_id),
        metrics_repo.get_team_metrics(&tenant.tenant_id)
    )
    .map_err(|e| Error::Internal(format!("Failed to fetch metrics: {}", e)))?;

    let metrics = DashboardMetrics {
        invoices: InvoiceMetrics {
            total_invoices: invoice_metrics.total_invoices as u64,
            pending_ocr: invoice_metrics.pending_ocr as u64,
            ready_for_review: invoice_metrics.ready_for_review as u64,
            submitted: invoice_metrics.submitted as u64,
            approved: invoice_metrics.approved as u64,
            rejected: invoice_metrics.rejected as u64,
            paid: invoice_metrics.paid as u64,
            avg_processing_time_hours: invoice_metrics.avg_processing_time_hours,
            total_value: invoice_metrics.total_value,
            this_month: invoice_metrics.this_month as u64,
            trend_vs_last_month: invoice_metrics.trend_vs_last_month,
        },
        approvals: ApprovalMetrics {
            pending_approvals: approval_metrics.pending_approvals as u64,
            approved_today: approval_metrics.approved_today as u64,
            rejected_today: approval_metrics.rejected_today as u64,
            avg_approval_time_hours: approval_metrics.avg_approval_time_hours,
            approval_rate: approval_metrics.approval_rate,
            escalated: approval_metrics.escalated as u64,
            overdue: approval_metrics.overdue as u64,
        },
        vendors: VendorMetrics {
            total_vendors: vendor_metrics.total_vendors as u64,
            new_this_month: vendor_metrics.new_this_month as u64,
            top_vendors: vendor_metrics
                .top_vendors
                .into_iter()
                .map(|v| TopVendor {
                    vendor_id: v.vendor_id,
                    vendor_name: v.vendor_name,
                    invoice_count: v.invoice_count as u64,
                    total_amount: v.total_amount,
                })
                .collect(),
            concentration_percentage: vendor_metrics.concentration_percentage,
        },
        team: TeamMetrics {
            members: team_metrics
                .members
                .into_iter()
                .map(|m| TeamMemberStats {
                    user_id: m.user_id,
                    user_name: m.user_name,
                    approvals_this_month: m.approvals_this_month as u64,
                    rejections_this_month: m.rejections_this_month as u64,
                    avg_response_time_hours: m.avg_response_time_hours,
                })
                .collect(),
            avg_approvals_per_member: team_metrics.avg_approvals_per_member,
            total_pending_actions: team_metrics.total_pending_actions as u64,
        },
    };

    Ok(Json(metrics))
}

/// Get invoice processing metrics
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/metrics/invoices",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Invoice metrics", body = InvoiceMetrics),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_invoice_metrics(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let metrics_repo = MetricsRepositoryImpl::new(pool);

    let db_metrics = metrics_repo
        .get_invoice_metrics(&tenant.tenant_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch invoice metrics: {}", e)))?;

    let metrics = InvoiceMetrics {
        total_invoices: db_metrics.total_invoices as u64,
        pending_ocr: db_metrics.pending_ocr as u64,
        ready_for_review: db_metrics.ready_for_review as u64,
        submitted: db_metrics.submitted as u64,
        approved: db_metrics.approved as u64,
        rejected: db_metrics.rejected as u64,
        paid: db_metrics.paid as u64,
        avg_processing_time_hours: db_metrics.avg_processing_time_hours,
        total_value: db_metrics.total_value,
        this_month: db_metrics.this_month as u64,
        trend_vs_last_month: db_metrics.trend_vs_last_month,
    };

    Ok(Json(metrics))
}

/// Get approval workflow metrics
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/metrics/approvals",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Approval metrics", body = ApprovalMetrics),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_approval_metrics(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let metrics_repo = MetricsRepositoryImpl::new(pool);

    let db_metrics = metrics_repo
        .get_approval_metrics(&tenant.tenant_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch approval metrics: {}", e)))?;

    let metrics = ApprovalMetrics {
        pending_approvals: db_metrics.pending_approvals as u64,
        approved_today: db_metrics.approved_today as u64,
        rejected_today: db_metrics.rejected_today as u64,
        avg_approval_time_hours: db_metrics.avg_approval_time_hours,
        approval_rate: db_metrics.approval_rate,
        escalated: db_metrics.escalated as u64,
        overdue: db_metrics.overdue as u64,
    };

    Ok(Json(metrics))
}

/// Get vendor analytics
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/metrics/vendors",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Vendor metrics", body = VendorMetrics),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_vendor_metrics(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let metrics_repo = MetricsRepositoryImpl::new(pool);

    let db_metrics = metrics_repo
        .get_vendor_metrics(&tenant.tenant_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch vendor metrics: {}", e)))?;

    let metrics = VendorMetrics {
        total_vendors: db_metrics.total_vendors as u64,
        new_this_month: db_metrics.new_this_month as u64,
        top_vendors: db_metrics
            .top_vendors
            .into_iter()
            .map(|v| TopVendor {
                vendor_id: v.vendor_id,
                vendor_name: v.vendor_name,
                invoice_count: v.invoice_count as u64,
                total_amount: v.total_amount,
            })
            .collect(),
        concentration_percentage: db_metrics.concentration_percentage,
    };

    Ok(Json(metrics))
}

/// Get team performance metrics
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/metrics/team",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Team metrics", body = TeamMetrics),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_team_metrics(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let metrics_repo = MetricsRepositoryImpl::new(pool);

    let db_metrics = metrics_repo
        .get_team_metrics(&tenant.tenant_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch team metrics: {}", e)))?;

    let metrics = TeamMetrics {
        members: db_metrics
            .members
            .into_iter()
            .map(|m| TeamMemberStats {
                user_id: m.user_id,
                user_name: m.user_name,
                approvals_this_month: m.approvals_this_month as u64,
                rejections_this_month: m.rejections_this_month as u64,
                avg_response_time_hours: m.avg_response_time_hours,
            })
            .collect(),
        avg_approvals_per_member: db_metrics.avg_approvals_per_member,
        total_pending_actions: db_metrics.total_pending_actions as u64,
    };

    Ok(Json(metrics))
}

// ---------------------------------------------------------------------------
// Stage Dwell Time
// ---------------------------------------------------------------------------

/// Stage dwell-time row for the bottleneck heat map
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StageDwellRow {
    /// Pipeline stage name (e.g. Capture, OCR, Coding, Approval, Payment)
    pub stage: String,
    /// Median minutes invoices spend in this stage
    pub median_minutes: f64,
    /// P90 minutes invoices spend in this stage
    pub p90_minutes: f64,
    /// Number of invoices that passed through this stage in the lookback window
    pub count: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/dashboard/stage-dwell",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Stage dwell time data", body = Vec<StageDwellRow>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_stage_dwell(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query_as::<_, (String, f64, f64, i64)>(
        r#"
        WITH transitions AS (
            SELECT
                from_status AS stage,
                EXTRACT(EPOCH FROM (created_at - LAG(created_at) OVER (PARTITION BY invoice_id ORDER BY created_at))) / 60.0 AS dwell_minutes
            FROM invoice_audit_log
            WHERE tenant_id = $1
              AND created_at > NOW() - INTERVAL '30 days'
              AND from_status IS NOT NULL
        )
        SELECT
            stage,
            COALESCE(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY dwell_minutes), 0) AS median_minutes,
            COALESCE(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY dwell_minutes), 0) AS p90_minutes,
            COUNT(*) AS count
        FROM transitions
        WHERE dwell_minutes IS NOT NULL AND dwell_minutes >= 0
        GROUP BY stage
        ORDER BY MIN(stage) ASC
        "#,
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to query stage dwell: {}", e)))?;

    let result: Vec<StageDwellRow> = rows
        .into_iter()
        .map(
            |(stage, median_minutes, p90_minutes, count)| StageDwellRow {
                stage,
                median_minutes,
                p90_minutes,
                count,
            },
        )
        .collect();

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// Approver Workload
// ---------------------------------------------------------------------------

/// Per-approver workload row
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApproverWorkloadRow {
    pub approver_id: Uuid,
    pub approver_name: String,
    pub pending_count: i64,
    pub near_breach_count: i64,
    pub breached_count: i64,
    pub avg_response_hours: f64,
}

#[utoipa::path(
    get,
    path = "/api/v1/dashboard/approver-workload",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Approver workload data", body = Vec<ApproverWorkloadRow>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_approver_workload(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query_as::<_, (Uuid, String, i64, i64, i64, f64)>(
        r#"
        SELECT
            u.id AS approver_id,
            u.name AS approver_name,
            COUNT(*) FILTER (WHERE ar.status = 'pending') AS pending_count,
            COUNT(*) FILTER (
                WHERE ar.status = 'pending'
                  AND EXTRACT(EPOCH FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at))) / 3600.0
                      >= COALESCE(ar.sla_hours, 24) * 0.8
            ) AS near_breach_count,
            COUNT(*) FILTER (
                WHERE ar.status = 'pending'
                  AND EXTRACT(EPOCH FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at))) / 3600.0
                      >= COALESCE(ar.sla_hours, 24)
            ) AS breached_count,
            COALESCE(
                AVG(
                    EXTRACT(EPOCH FROM (ar.responded_at - ar.created_at)) / 3600.0
                ) FILTER (WHERE ar.status IN ('approved', 'rejected') AND ar.responded_at IS NOT NULL),
                0
            ) AS avg_response_hours
        FROM users u
        LEFT JOIN approval_requests ar
            ON ar.requested_from->>'User' = u.id::text
            AND ar.tenant_id = $1
        WHERE u.tenant_id = $1
          AND u.id IN (
            SELECT DISTINCT NULLIF(requested_from->>'User', '')::uuid
            FROM approval_requests
            WHERE tenant_id = $1 AND status = 'pending'
              AND requested_from->>'User' IS NOT NULL
          )
        GROUP BY u.id, u.name
        ORDER BY pending_count DESC
        LIMIT 10
        "#,
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to query approver workload: {}", e)))?;

    let result: Vec<ApproverWorkloadRow> = rows
        .into_iter()
        .map(
            |(
                approver_id,
                approver_name,
                pending_count,
                near_breach_count,
                breached_count,
                avg_response_hours,
            )| {
                ApproverWorkloadRow {
                    approver_id,
                    approver_name,
                    pending_count,
                    near_breach_count,
                    breached_count,
                    avg_response_hours,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// Exception Rate Trend
// ---------------------------------------------------------------------------

/// Daily exception rate data point
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExceptionTrendPoint {
    /// Date in YYYY-MM-DD format
    pub date: String,
    /// Total invoices processed that day
    pub total_invoices: i64,
    /// Invoices that hit an exception state
    pub exception_count: i64,
    /// exception_count / total_invoices (0.0 if no invoices)
    pub exception_rate: f64,
}

#[derive(Debug, Deserialize)]
struct ExceptionTrendParams {
    #[serde(default = "default_days")]
    days: i32,
}

fn default_days() -> i32 {
    14
}

#[utoipa::path(
    get,
    path = "/api/v1/dashboard/exception-trend",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Exception rate trend", body = Vec<ExceptionTrendPoint>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_exception_trend(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Query(params): Query<ExceptionTrendParams>,
) -> ApiResult<impl IntoResponse> {
    let days = params.days.clamp(1, 90);
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        r#"
        SELECT
            d.day::text AS date,
            COUNT(i.id) AS total_invoices,
            COUNT(i.id) FILTER (
                WHERE i.status IN ('on_hold', 'rejected', 'voided', 'ocr_failed', 'exception')
                   OR i.processing_status IN ('on_hold', 'rejected', 'voided', 'exception')
            ) AS exception_count
        FROM generate_series(
            CURRENT_DATE - ($2::int || ' days')::interval,
            CURRENT_DATE,
            INTERVAL '1 day'
        ) AS d(day)
        LEFT JOIN invoices i
            ON i.tenant_id = $1
            AND i.created_at::date = d.day::date
        GROUP BY d.day
        ORDER BY d.day ASC
        "#,
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(days)
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to query exception trend: {}", e)))?;

    let result: Vec<ExceptionTrendPoint> = rows
        .into_iter()
        .map(|(date, total_invoices, exception_count)| {
            let exception_rate = if total_invoices > 0 {
                (exception_count as f64 / total_invoices as f64) * 100.0
            } else {
                0.0
            };
            ExceptionTrendPoint {
                date,
                total_invoices,
                exception_count,
                exception_rate,
            }
        })
        .collect();

    Ok(Json(result))
}
