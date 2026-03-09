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
use billforge_reporting::DashboardSummary;

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
        // New advanced reporting endpoints
        .route("/spend/trends", get(spend_trends))
        .route("/categories/breakdown", get(category_breakdown))
        .route("/vendors/performance", get(vendor_performance))
        .route("/approvals/analytics", get(approval_analytics))
}

async fn dashboard_summary(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<DashboardSummary>> {
    // Get real counts from the database
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Use reporting service
    let reporting_service = billforge_reporting::ReportingService::new();
    let summary = reporting_service.get_dashboard_summary(&tenant.tenant_id, &pool).await?;

    Ok(Json(summary))
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    // Parse date range (default to last 30 days if not provided)
    let end_date = query.end_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .unwrap_or_else(|| chrono::Utc::now().naive_utc().date());
    let start_date = query.start_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .unwrap_or_else(|| end_date - chrono::Duration::days(30));

    let vendor_spend = reporting_service.get_vendor_spend(
        &tenant.tenant_id,
        &pool,
        Some(start_date),
        Some(end_date),
        100, // limit
    ).await?;

    let result: Vec<InvoicesByVendor> = vendor_spend.into_iter().map(|vs| InvoicesByVendor {
        vendor_id: vs.vendor_id,
        vendor_name: vs.vendor_name,
        invoice_count: vs.invoice_count,
        total_amount: vs.total_spend,
    }).collect();

    Ok(Json(result))
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let status_dist = reporting_service.get_status_distribution(&tenant.tenant_id, &pool).await?;

    let result: Vec<InvoicesByStatus> = status_dist.into_iter().map(|sd| InvoicesByStatus {
        status: sd.status,
        count: sd.count,
        total_amount: sd.total_amount,
    }).collect();

    Ok(Json(result))
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let aging_data = reporting_service.get_invoice_aging(&tenant.tenant_id, &pool).await?;

    let result: Vec<AgingBucket> = aging_data.into_iter().map(|ab| AgingBucket {
        bucket: ab.bucket_name,
        count: ab.invoice_count,
        total_amount: ab.total_amount,
    }).collect();

    Ok(Json(result))
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    // Get all vendors with their spend data
    let vendor_spend = reporting_service.get_vendor_spend(
        &tenant.tenant_id,
        &pool,
        None,
        None,
        100, // limit
    ).await?;

    // Get YTD spend for each vendor
    let ytd_spend = reporting_service.get_vendor_spend_ytd(
        &tenant.tenant_id,
        &pool,
        100,
    ).await?;

    // Get MTD spend for each vendor
    let mtd_spend = reporting_service.get_vendor_spend_mtd(
        &tenant.tenant_id,
        &pool,
        100,
    ).await?;

    // Build lookup maps for YTD and MTD
    let ytd_map: std::collections::HashMap<String, f64> = ytd_spend
        .iter()
        .map(|vs| (vs.vendor_id.clone(), vs.total_spend))
        .collect();

    let mtd_map: std::collections::HashMap<String, f64> = mtd_spend
        .iter()
        .map(|vs| (vs.vendor_id.clone(), vs.total_spend))
        .collect();

    // Combine data
    let result: Vec<VendorSpend> = vendor_spend.into_iter().map(|vs| VendorSpend {
        vendor_id: vs.vendor_id.clone(),
        vendor_name: vs.vendor_name,
        ytd_spend: ytd_map.get(&vs.vendor_id).copied().unwrap_or(0.0),
        mtd_spend: mtd_map.get(&vs.vendor_id).copied().unwrap_or(0.0),
        invoice_count: vs.invoice_count,
    }).collect();

    Ok(Json(result))
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let metrics = reporting_service.get_processing_metrics(&tenant.tenant_id, &pool).await?;

    // Get invoices processed today from dashboard summary
    let summary = reporting_service.get_dashboard_summary(&tenant.tenant_id, &pool).await?;

    // Get invoices processed this week
    let invoices_this_week = reporting_service.get_invoices_processed_this_week(&tenant.tenant_id, &pool).await?;

    Ok(Json(WorkflowMetrics {
        avg_processing_time_hours: metrics.avg_total_processing_time_hours,
        avg_approval_time_hours: metrics.avg_approval_time_hours,
        auto_approval_rate: metrics.auto_approval_rate,
        rejection_rate: metrics.rejection_rate,
        invoices_processed_today: summary.invoices_processed_today,
        invoices_processed_this_week: invoices_this_week,
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    // Parse filters JSON if provided
    let (date_range, filters) = if let Some(filters_str) = query.filters {
        match serde_json::from_str::<serde_json::Value>(&filters_str) {
            Ok(json) => {
                // Extract date_range
                let date_range = json.get("date_range").and_then(|dr| {
                    let start = dr.get("start")?.as_str()?;
                    let end = dr.get("end")?.as_str()?;

                    Some(billforge_reporting::DateRange {
                        start: chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d").ok()?,
                        end: chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d").ok()?,
                    })
                });

                // Extract filters array
                let filters = json.get("filters")
                    .and_then(|f| f.as_array())
                    .map(|arr| {
                        arr.iter().filter_map(|filter| {
                            Some(billforge_reporting::ReportFilter {
                                field: filter.get("field")?.as_str()?.to_string(),
                                operator: filter.get("operator")?.as_str()?.to_string(),
                                value: filter.get("value")?.clone(),
                            })
                        }).collect()
                    })
                    .unwrap_or_default();

                (date_range, filters)
            }
            Err(_) => (None, vec![]),
        }
    } else {
        (None, vec![])
    };

    // Build CustomReportQuery from HTTP query params
    let report_query = billforge_reporting::CustomReportQuery {
        report_type: query.report_type,
        date_range,
        filters,
        group_by: query.group_by,
        order_by: None,
        limit: None,
    };

    let result = reporting_service.execute_custom_report(&tenant.tenant_id, &pool, report_query).await?;

    // Convert CustomReportResult to JSON
    Ok(Json(serde_json::json!({
        "columns": result.columns,
        "rows": result.rows,
        "total_rows": result.total_rows,
        "generated_at": result.generated_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct SpendTrendsQuery {
    pub start_date: String,
    pub end_date: String,
    pub group_by: Option<String>,
}

async fn spend_trends(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<SpendTrendsQuery>,
) -> ApiResult<Json<Vec<billforge_reporting::SpendTrendPoint>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let start_date = chrono::NaiveDate::parse_from_str(&query.start_date, "%Y-%m-%d")
        .map_err(|_| billforge_core::Error::Validation("Invalid start_date format. Use YYYY-MM-DD".to_string()))?;
    let end_date = chrono::NaiveDate::parse_from_str(&query.end_date, "%Y-%m-%d")
        .map_err(|_| billforge_core::Error::Validation("Invalid end_date format. Use YYYY-MM-DD".to_string()))?;

    let group_by = query.group_by.as_deref().unwrap_or("month");

    let trends = reporting_service.get_spend_trends(
        &tenant.tenant_id,
        &pool,
        start_date,
        end_date,
        group_by,
    ).await?;

    Ok(Json(trends))
}

#[derive(Debug, Deserialize)]
pub struct CategoryBreakdownQuery {
    pub category_type: String, // "gl_code", "department", or "cost_center"
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

async fn category_breakdown(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<CategoryBreakdownQuery>,
) -> ApiResult<Json<Vec<billforge_reporting::CategoryBreakdown>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let start_date = query.start_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let end_date = query.end_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());

    let breakdown = reporting_service.get_category_breakdown(
        &tenant.tenant_id,
        &pool,
        &query.category_type,
        start_date,
        end_date,
    ).await?;

    Ok(Json(breakdown))
}

#[derive(Debug, Deserialize)]
pub struct VendorPerformanceQuery {
    pub limit: Option<u32>,
}

async fn vendor_performance(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<VendorPerformanceQuery>,
) -> ApiResult<Json<Vec<billforge_reporting::VendorPerformanceMetrics>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let limit = query.limit.unwrap_or(50);

    let performance = reporting_service.get_vendor_performance(
        &tenant.tenant_id,
        &pool,
        limit,
    ).await?;

    Ok(Json(performance))
}

#[derive(Debug, Deserialize)]
pub struct ApprovalAnalyticsQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

async fn approval_analytics(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<ApprovalAnalyticsQuery>,
) -> ApiResult<Json<billforge_reporting::ApprovalAnalytics>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let start_date = query.start_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let end_date = query.end_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());

    let analytics = reporting_service.get_approval_analytics(
        &tenant.tenant_id,
        &pool,
        start_date,
        end_date,
    ).await?;

    Ok(Json(analytics))
}
