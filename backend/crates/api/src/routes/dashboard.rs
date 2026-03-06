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
    let tenant_id = tenant.tenant_id.as_str();

    // TODO: Implement actual metrics aggregation from database
    // For now, return mock data

    let metrics = DashboardMetrics {
        invoices: InvoiceMetrics {
            total_invoices: 1250,
            pending_ocr: 15,
            ready_for_review: 42,
            submitted: 28,
            approved: 1080,
            rejected: 85,
            paid: 920,
            avg_processing_time_hours: 4.2,
            total_value: 125000000, // $1,250,000 in cents
            this_month: 145,
            trend_vs_last_month: 12.5,
        },
        approvals: ApprovalMetrics {
            pending_approvals: 28,
            approved_today: 12,
            rejected_today: 3,
            avg_approval_time_hours: 2.8,
            approval_rate: 92.7,
            escalated: 2,
            overdue: 5,
        },
        vendors: VendorMetrics {
            total_vendors: 324,
            new_this_month: 8,
            top_vendors: vec![
                TopVendor {
                    vendor_id: "vendor-1".to_string(),
                    vendor_name: "Acme Corporation".to_string(),
                    invoice_count: 145,
                    total_amount: 12500000,
                },
                TopVendor {
                    vendor_id: "vendor-2".to_string(),
                    vendor_name: "Tech Supplies Inc".to_string(),
                    invoice_count: 98,
                    total_amount: 8750000,
                },
                TopVendor {
                    vendor_id: "vendor-3".to_string(),
                    vendor_name: "Office Solutions".to_string(),
                    invoice_count: 76,
                    total_amount: 5400000,
                },
            ],
            concentration_percentage: 68.5,
        },
        team: TeamMetrics {
            members: vec![
                TeamMemberStats {
                    user_id: "user-1".to_string(),
                    user_name: "John Smith".to_string(),
                    approvals_this_month: 45,
                    rejections_this_month: 3,
                    avg_response_time_hours: 1.2,
                },
                TeamMemberStats {
                    user_id: "user-2".to_string(),
                    user_name: "Sarah Johnson".to_string(),
                    approvals_this_month: 38,
                    rejections_this_month: 5,
                    avg_response_time_hours: 2.4,
                },
            ],
            avg_approvals_per_member: 41.5,
            total_pending_actions: 28,
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
    let tenant_id = tenant.tenant_id.as_str();

    // TODO: Implement actual metrics from database

    let metrics = InvoiceMetrics {
        total_invoices: 1250,
        pending_ocr: 15,
        ready_for_review: 42,
        submitted: 28,
        approved: 1080,
        rejected: 85,
        paid: 920,
        avg_processing_time_hours: 4.2,
        total_value: 125000000,
        this_month: 145,
        trend_vs_last_month: 12.5,
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
    let tenant_id = tenant.tenant_id.as_str();

    // TODO: Implement actual metrics from database

    let metrics = ApprovalMetrics {
        pending_approvals: 28,
        approved_today: 12,
        rejected_today: 3,
        avg_approval_time_hours: 2.8,
        approval_rate: 92.7,
        escalated: 2,
        overdue: 5,
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
    let tenant_id = tenant.tenant_id.as_str();

    // TODO: Implement actual metrics from database

    let metrics = VendorMetrics {
        total_vendors: 324,
        new_this_month: 8,
        top_vendors: vec![
            TopVendor {
                vendor_id: "vendor-1".to_string(),
                vendor_name: "Acme Corporation".to_string(),
                invoice_count: 145,
                total_amount: 12500000,
            },
            TopVendor {
                vendor_id: "vendor-2".to_string(),
                vendor_name: "Tech Supplies Inc".to_string(),
                invoice_count: 98,
                total_amount: 8750000,
            },
        ],
        concentration_percentage: 68.5,
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
    let tenant_id = tenant.tenant_id.as_str();

    // TODO: Implement actual metrics from database

    let metrics = TeamMetrics {
        members: vec![
            TeamMemberStats {
                user_id: "user-1".to_string(),
                user_name: "John Smith".to_string(),
                approvals_this_month: 45,
                rejections_this_month: 3,
                avg_response_time_hours: 1.2,
            },
            TeamMemberStats {
                user_id: "user-2".to_string(),
                user_name: "Sarah Johnson".to_string(),
                approvals_this_month: 38,
                rejections_this_month: 5,
                avg_response_time_hours: 2.4,
            },
        ],
        avg_approvals_per_member: 41.5,
        total_pending_actions: 28,
    };

    Ok(Json(metrics))
}
