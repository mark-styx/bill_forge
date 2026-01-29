//! Reporting routes (Reporting module)

use crate::error::ApiResult;
use crate::extractors::{AuthUser, ReportingAccess, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Dashboard endpoints (basic, available to all)
        .route("/dashboard/summary", get(dashboard_summary))
        // Advanced reports (Reporting module required)
        .route("/invoices/by-vendor", get(invoices_by_vendor))
        .route("/invoices/by-status", get(invoices_by_status))
        .route("/invoices/aging", get(invoice_aging))
        .route("/vendors/spend", get(vendor_spend))
        .route("/workflows/metrics", get(workflow_metrics))
        .route("/custom", get(custom_report))
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub invoices_pending_review: u64,
    pub invoices_pending_approval: u64,
    pub invoices_ready_for_payment: u64,
    pub total_amount_pending: f64,
    pub vendors_active: u64,
    pub invoices_this_month: u64,
}

async fn dashboard_summary(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<DashboardSummary>> {
    // Get real counts from the database
    let db = state.db.tenant(&tenant.tenant_id).await?;
    let conn = db.connection().await;
    let conn = conn.lock().await;

    let invoices_pending_review: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM invoices WHERE capture_status IN ('pending', 'ready_for_review')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let invoices_pending_approval: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM invoices WHERE processing_status = 'pending_approval'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let invoices_ready_for_payment: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM invoices WHERE processing_status = 'ready_for_payment'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let total_amount_pending: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_amount), 0) FROM invoices WHERE processing_status NOT IN ('paid', 'voided')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let vendors_active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vendors WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let invoices_this_month: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM invoices WHERE created_at >= date('now', 'start of month')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(Json(DashboardSummary {
        invoices_pending_review: invoices_pending_review as u64,
        invoices_pending_approval: invoices_pending_approval as u64,
        invoices_ready_for_payment: invoices_ready_for_payment as u64,
        total_amount_pending: (total_amount_pending as f64) / 100.0, // Convert cents to dollars
        vendors_active: vendors_active as u64,
        invoices_this_month: invoices_this_month as u64,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DateRangeQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvoicesByVendor {
    pub vendor_id: String,
    pub vendor_name: String,
    pub invoice_count: u64,
    pub total_amount: f64,
}

async fn invoices_by_vendor(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<DateRangeQuery>,
) -> ApiResult<Json<Vec<InvoicesByVendor>>> {
    // TODO: Implement actual report
    Ok(Json(vec![
        InvoicesByVendor {
            vendor_id: "v1".to_string(),
            vendor_name: "Acme Corp".to_string(),
            invoice_count: 15,
            total_amount: 12500.00,
        },
        InvoicesByVendor {
            vendor_id: "v2".to_string(),
            vendor_name: "TechSupplies Inc".to_string(),
            invoice_count: 8,
            total_amount: 8900.00,
        },
    ]))
}

#[derive(Debug, Serialize)]
pub struct InvoicesByStatus {
    pub status: String,
    pub count: u64,
    pub total_amount: f64,
}

async fn invoices_by_status(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<DateRangeQuery>,
) -> ApiResult<Json<Vec<InvoicesByStatus>>> {
    // TODO: Implement actual report
    Ok(Json(vec![
        InvoicesByStatus {
            status: "pending_approval".to_string(),
            count: 5,
            total_amount: 15000.00,
        },
        InvoicesByStatus {
            status: "ready_for_payment".to_string(),
            count: 8,
            total_amount: 23000.00,
        },
    ]))
}

#[derive(Debug, Serialize)]
pub struct AgingBucket {
    pub bucket: String,
    pub count: u64,
    pub total_amount: f64,
}

async fn invoice_aging(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
) -> ApiResult<Json<Vec<AgingBucket>>> {
    // TODO: Implement actual aging report
    Ok(Json(vec![
        AgingBucket {
            bucket: "0-30 days".to_string(),
            count: 25,
            total_amount: 45000.00,
        },
        AgingBucket {
            bucket: "31-60 days".to_string(),
            count: 10,
            total_amount: 18000.00,
        },
        AgingBucket {
            bucket: "61-90 days".to_string(),
            count: 3,
            total_amount: 5000.00,
        },
        AgingBucket {
            bucket: "90+ days".to_string(),
            count: 1,
            total_amount: 2000.00,
        },
    ]))
}

#[derive(Debug, Serialize)]
pub struct VendorSpend {
    pub vendor_id: String,
    pub vendor_name: String,
    pub ytd_spend: f64,
    pub mtd_spend: f64,
    pub invoice_count: u64,
}

async fn vendor_spend(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<DateRangeQuery>,
) -> ApiResult<Json<Vec<VendorSpend>>> {
    // TODO: Implement actual vendor spend report
    Ok(Json(vec![]))
}

#[derive(Debug, Serialize)]
pub struct WorkflowMetrics {
    pub avg_processing_time_hours: f64,
    pub avg_approval_time_hours: f64,
    pub auto_approval_rate: f64,
    pub rejection_rate: f64,
    pub invoices_processed_today: u64,
    pub invoices_processed_this_week: u64,
}

async fn workflow_metrics(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
) -> ApiResult<Json<WorkflowMetrics>> {
    // TODO: Implement actual workflow metrics
    Ok(Json(WorkflowMetrics {
        avg_processing_time_hours: 4.5,
        avg_approval_time_hours: 2.3,
        auto_approval_rate: 0.35,
        rejection_rate: 0.05,
        invoices_processed_today: 12,
        invoices_processed_this_week: 78,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CustomReportQuery {
    pub report_type: String,
    pub filters: Option<String>,
    pub group_by: Option<String>,
}

async fn custom_report(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<CustomReportQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    // TODO: Implement custom report builder
    Ok(Json(serde_json::json!({
        "report_type": query.report_type,
        "data": [],
        "message": "Custom report endpoint - implementation pending"
    })))
}
