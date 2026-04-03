//! Dashboard metrics and analytics endpoints
//!
//! Provides aggregated metrics for the analytics dashboard including:
//! - Invoice processing metrics
//! - Approval workflow metrics
//! - Vendor analytics
//! - Team performance metrics

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    extract::State,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use billforge_core::Error;
use billforge_db::repositories::MetricsRepositoryImpl;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(get_dashboard_metrics))
        .route("/metrics/invoices", get(get_invoice_metrics))
        .route("/metrics/approvals", get(get_approval_metrics))
        .route("/metrics/vendors", get(get_vendor_metrics))
        .route("/metrics/team", get(get_team_metrics))
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
    TenantCtx(tenant): TenantCtx,
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
    TenantCtx(tenant): TenantCtx,
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
    TenantCtx(tenant): TenantCtx,
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
    TenantCtx(tenant): TenantCtx,
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
    TenantCtx(tenant): TenantCtx,
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
