//! Reporting routes (Reporting module)

use crate::error::ApiResult;
use crate::extractors::{AuthUser, ReportingAccess, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get},
    Json, Router,
};
use billforge_reporting::DashboardSummary;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Dashboard endpoints (basic, available to all)
        .route("/dashboard/summary", get(dashboard_summary))
        // KPIs backed by materialized view (sub-second reads)
        .route("/dashboard/kpis", get(dashboard_kpis))
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
        .route("/approvals/sla", get(approval_sla))
        .route("/cash-flow/obligations", get(cash_flow_obligations))
        // Email digest management
        .route("/digests", get(list_digests).post(create_digest))
        .route("/digests/:id", delete(delete_digest))
}

#[utoipa::path(get, path = "/api/v1/reports/dashboard/summary", tag = "Reports", responses((status = 200, description = "Dashboard summary")))]
async fn dashboard_summary(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<DashboardSummary>> {
    // Get real counts from the database
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Use reporting service
    let reporting_service = billforge_reporting::ReportingService::new();
    let summary = reporting_service
        .get_dashboard_summary(&tenant.tenant_id, &pool)
        .await?;

    Ok(Json(summary))
}

// ---------------------------------------------------------------------------
// Dashboard KPIs (materialized view)
// ---------------------------------------------------------------------------

/// Aging bucket metrics for queued invoices.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgingBuckets {
    /// Invoices 0-7 days old
    pub aging_0_7: i64,
    /// Total amount (cents) for 0-7 day bucket
    pub aging_0_7_amount: i64,
    /// Invoices 8-14 days old
    pub aging_8_14: i64,
    /// Total amount (cents) for 8-14 day bucket
    pub aging_8_14_amount: i64,
    /// Invoices 15-30 days old
    pub aging_15_30: i64,
    /// Total amount (cents) for 15-30 day bucket
    pub aging_15_30_amount: i64,
    /// Invoices older than 30 days
    pub aging_30_plus: i64,
    /// Total amount (cents) for 30+ day bucket
    pub aging_30_plus_amount: i64,
}

/// Top vendor by spend.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VendorSpendEntry {
    pub vendor_id: String,
    pub vendor_name: String,
    /// Total amount in cents
    pub total_amount: i64,
    pub invoice_count: i64,
}

/// Aggregated dashboard KPIs returned by the materialized view.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardKpis {
    pub queue_count: i64,
    pub approved_count: i64,
    pub paid_count: i64,
    pub rejected_count: i64,
    pub aging: AgingBuckets,
    pub spend_by_vendor: Vec<VendorSpendEntry>,
    /// Total paid spend in last 30 days (cents)
    pub total_spend_30d: i64,
    /// Average processing hours for paid invoices in last 30 days
    pub avg_processing_hours: f64,
}

#[utoipa::path(
    get,
    path = "/api/v1/reports/dashboard/kpis",
    tag = "Reports",
    responses(
        (status = 200, description = "Dashboard KPIs from materialized view", body = DashboardKpis)
    )
)]
async fn dashboard_kpis(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<DashboardKpis>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let row = sqlx::query_as::<
        _,
        (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            serde_json::Value,
            i64,
            f64,
        ),
    >(
        r#"SELECT
            queue_count, approved_count, paid_count, rejected_count,
            aging_0_7, aging_0_7_amount,
            aging_8_14, aging_8_14_amount,
            aging_15_30, aging_15_30_amount,
            aging_30_plus, aging_30_plus_amount,
            spend_by_vendor,
            total_spend_30d,
            avg_processing_hours
        FROM dashboard_kpis_mv
        WHERE tenant_id = $1"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to query dashboard_kpis_mv: {}", e))
    })?;

    let kpis = if let Some(row) = row {
        let spend_by_vendor: Vec<VendorSpendEntry> =
            serde_json::from_value(row.12).unwrap_or_default();
        DashboardKpis {
            queue_count: row.0,
            approved_count: row.1,
            paid_count: row.2,
            rejected_count: row.3,
            aging: AgingBuckets {
                aging_0_7: row.4,
                aging_0_7_amount: row.5,
                aging_8_14: row.6,
                aging_8_14_amount: row.7,
                aging_15_30: row.8,
                aging_15_30_amount: row.9,
                aging_30_plus: row.10,
                aging_30_plus_amount: row.11,
            },
            spend_by_vendor,
            total_spend_30d: row.13,
            avg_processing_hours: row.14,
        }
    } else {
        // New tenant with no data in the MV - return zero-valued defaults
        DashboardKpis {
            queue_count: 0,
            approved_count: 0,
            paid_count: 0,
            rejected_count: 0,
            aging: AgingBuckets {
                aging_0_7: 0,
                aging_0_7_amount: 0,
                aging_8_14: 0,
                aging_8_14_amount: 0,
                aging_15_30: 0,
                aging_15_30_amount: 0,
                aging_30_plus: 0,
                aging_30_plus_amount: 0,
            },
            spend_by_vendor: vec![],
            total_spend_30d: 0,
            avg_processing_hours: 0.0,
        }
    };

    Ok(Json(kpis))
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

#[utoipa::path(get, path = "/api/v1/reports/invoices/by-vendor", tag = "Reports", responses((status = 200, description = "Invoices grouped by vendor")))]
async fn invoices_by_vendor(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<DateRangeQuery>,
) -> ApiResult<Json<Vec<InvoicesByVendor>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    // Parse date range (default to last 30 days if not provided)
    let end_date = query
        .end_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .unwrap_or_else(|| chrono::Utc::now().naive_utc().date());
    let start_date = query
        .start_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .unwrap_or_else(|| end_date - chrono::Duration::days(30));

    let vendor_spend = reporting_service
        .get_vendor_spend(
            &tenant.tenant_id,
            &pool,
            Some(start_date),
            Some(end_date),
            100, // limit
        )
        .await?;

    let result: Vec<InvoicesByVendor> = vendor_spend
        .into_iter()
        .map(|vs| InvoicesByVendor {
            vendor_id: vs.vendor_id,
            vendor_name: vs.vendor_name,
            invoice_count: vs.invoice_count,
            total_amount: vs.total_spend,
        })
        .collect();

    Ok(Json(result))
}

#[derive(Debug, Serialize)]
pub struct InvoicesByStatus {
    pub status: String,
    pub count: u64,
    pub total_amount: f64,
}

#[utoipa::path(get, path = "/api/v1/reports/invoices/by-status", tag = "Reports", responses((status = 200, description = "Invoice status distribution")))]
async fn invoices_by_status(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<DateRangeQuery>,
) -> ApiResult<Json<Vec<InvoicesByStatus>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let status_dist = reporting_service
        .get_status_distribution(&tenant.tenant_id, &pool)
        .await?;

    let result: Vec<InvoicesByStatus> = status_dist
        .into_iter()
        .map(|sd| InvoicesByStatus {
            status: sd.status,
            count: sd.count,
            total_amount: sd.total_amount,
        })
        .collect();

    Ok(Json(result))
}

#[derive(Debug, Serialize)]
pub struct AgingBucket {
    pub bucket: String,
    pub count: u64,
    pub total_amount: f64,
}

#[utoipa::path(get, path = "/api/v1/reports/invoices/aging", tag = "Reports", responses((status = 200, description = "Invoice aging buckets")))]
async fn invoice_aging(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
) -> ApiResult<Json<Vec<AgingBucket>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let aging_data = reporting_service
        .get_invoice_aging(&tenant.tenant_id, &pool)
        .await?;

    let result: Vec<AgingBucket> = aging_data
        .into_iter()
        .map(|ab| AgingBucket {
            bucket: ab.bucket_name,
            count: ab.invoice_count,
            total_amount: ab.total_amount,
        })
        .collect();

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

#[utoipa::path(get, path = "/api/v1/reports/vendors/spend", tag = "Reports", responses((status = 200, description = "Vendor spend totals")))]
async fn vendor_spend(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<DateRangeQuery>,
) -> ApiResult<Json<Vec<VendorSpend>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    // Get all vendors with their spend data
    let vendor_spend = reporting_service
        .get_vendor_spend(
            &tenant.tenant_id,
            &pool,
            None,
            None,
            100, // limit
        )
        .await?;

    // Get YTD spend for each vendor
    let ytd_spend = reporting_service
        .get_vendor_spend_ytd(&tenant.tenant_id, &pool, 100)
        .await?;

    // Get MTD spend for each vendor
    let mtd_spend = reporting_service
        .get_vendor_spend_mtd(&tenant.tenant_id, &pool, 100)
        .await?;

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
    let result: Vec<VendorSpend> = vendor_spend
        .into_iter()
        .map(|vs| VendorSpend {
            vendor_id: vs.vendor_id.clone(),
            vendor_name: vs.vendor_name,
            ytd_spend: ytd_map.get(&vs.vendor_id).copied().unwrap_or(0.0),
            mtd_spend: mtd_map.get(&vs.vendor_id).copied().unwrap_or(0.0),
            invoice_count: vs.invoice_count,
        })
        .collect();

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

#[utoipa::path(get, path = "/api/v1/reports/workflows/metrics", tag = "Reports", responses((status = 200, description = "Workflow processing metrics")))]
async fn workflow_metrics(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
) -> ApiResult<Json<WorkflowMetrics>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let metrics = reporting_service
        .get_processing_metrics(&tenant.tenant_id, &pool)
        .await?;

    // Get invoices processed today from dashboard summary
    let summary = reporting_service
        .get_dashboard_summary(&tenant.tenant_id, &pool)
        .await?;

    // Get invoices processed this week
    let invoices_this_week = reporting_service
        .get_invoices_processed_this_week(&tenant.tenant_id, &pool)
        .await?;

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

#[utoipa::path(get, path = "/api/v1/reports/custom", tag = "Reports", responses((status = 200, description = "Custom report data")))]
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
                let filters = json
                    .get("filters")
                    .and_then(|f| f.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|filter| {
                                Some(billforge_reporting::ReportFilter {
                                    field: filter.get("field")?.as_str()?.to_string(),
                                    operator: filter.get("operator")?.as_str()?.to_string(),
                                    value: filter.get("value")?.clone(),
                                })
                            })
                            .collect()
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

    let result = reporting_service
        .execute_custom_report(&tenant.tenant_id, &pool, report_query)
        .await?;

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

#[utoipa::path(get, path = "/api/v1/reports/spend/trends", tag = "Reports", responses((status = 200, description = "Spend trends")))]
async fn spend_trends(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<SpendTrendsQuery>,
) -> ApiResult<Json<Vec<billforge_reporting::SpendTrendPoint>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let start_date =
        chrono::NaiveDate::parse_from_str(&query.start_date, "%Y-%m-%d").map_err(|_| {
            billforge_core::Error::Validation(
                "Invalid start_date format. Use YYYY-MM-DD".to_string(),
            )
        })?;
    let end_date =
        chrono::NaiveDate::parse_from_str(&query.end_date, "%Y-%m-%d").map_err(|_| {
            billforge_core::Error::Validation("Invalid end_date format. Use YYYY-MM-DD".to_string())
        })?;

    let group_by = query.group_by.as_deref().unwrap_or("month");

    let trends = reporting_service
        .get_spend_trends(&tenant.tenant_id, &pool, start_date, end_date, group_by)
        .await?;

    Ok(Json(trends))
}

#[derive(Debug, Deserialize)]
pub struct CategoryBreakdownQuery {
    pub category_type: String, // "gl_code", "department", or "cost_center"
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/reports/categories/breakdown", tag = "Reports", responses((status = 200, description = "Category breakdown")))]
async fn category_breakdown(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<CategoryBreakdownQuery>,
) -> ApiResult<Json<Vec<billforge_reporting::CategoryBreakdown>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let start_date = query
        .start_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let end_date = query
        .end_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());

    let breakdown = reporting_service
        .get_category_breakdown(
            &tenant.tenant_id,
            &pool,
            &query.category_type,
            start_date,
            end_date,
        )
        .await?;

    Ok(Json(breakdown))
}

#[derive(Debug, Deserialize)]
pub struct VendorPerformanceQuery {
    pub limit: Option<u32>,
}

#[utoipa::path(get, path = "/api/v1/reports/vendors/performance", tag = "Reports", responses((status = 200, description = "Vendor performance")))]
async fn vendor_performance(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<VendorPerformanceQuery>,
) -> ApiResult<Json<Vec<billforge_reporting::VendorPerformanceMetrics>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let limit = query.limit.unwrap_or(50);

    let performance = reporting_service
        .get_vendor_performance(&tenant.tenant_id, &pool, limit)
        .await?;

    Ok(Json(performance))
}

#[derive(Debug, Deserialize)]
pub struct ApprovalAnalyticsQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/reports/approvals/analytics", tag = "Reports", responses((status = 200, description = "Approval analytics")))]
async fn approval_analytics(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Query(query): Query<ApprovalAnalyticsQuery>,
) -> ApiResult<Json<billforge_reporting::ApprovalAnalytics>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let start_date = query
        .start_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let end_date = query
        .end_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());

    let analytics = reporting_service
        .get_approval_analytics(&tenant.tenant_id, &pool, start_date, end_date)
        .await?;

    Ok(Json(analytics))
}

#[derive(Debug, Serialize)]
pub struct ApprovalSlaItem {
    pub invoice_id: Uuid,
    pub invoice_number: String,
    pub vendor_name: String,
    pub amount_cents: i64,
    pub currency: String,
    pub approval_id: Uuid,
    pub hours_waiting: f64,
    pub sla_hours: i32,
    pub sla_state: String,
    pub approver_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApprovalSlaSummary {
    pub pending_count: i64,
    pub near_breach_count: i64,
    pub breached_count: i64,
    pub items: Vec<ApprovalSlaItem>,
}

#[utoipa::path(get, path = "/api/v1/reports/approvals/sla", tag = "Reports", responses((status = 200, description = "Approval SLA tracking")))]
async fn approval_sla(
    State(state): State<AppState>,
    ReportingAccess(_user, tenant): ReportingAccess,
) -> ApiResult<Json<ApprovalSlaSummary>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let rows = sqlx::query_as::<_, (Uuid, String, String, i64, String, Uuid, f64, Option<String>)>(
        r#"
        SELECT
            i.id,
            i.invoice_number,
            i.vendor_name,
            i.total_amount_cents,
            i.currency,
            ar.id,
            EXTRACT(EPOCH FROM (NOW() - ar.created_at)) / 3600.0 AS hours_waiting,
            u.name AS approver_name
        FROM approval_requests ar
        JOIN invoices i ON i.id = ar.invoice_id
        LEFT JOIN users u ON u.id = ar.responded_by
        WHERE ar.tenant_id = $1 AND ar.status = 'pending'
        ORDER BY ar.created_at ASC
        LIMIT 100
        "#,
    )
    .bind(tenant.tenant_id.as_str())
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to query approval SLA data: {}", e))
    })?;

    let default_sla_hours = 24;
    let near_breach_hours = (default_sla_hours as f64) * 0.8;
    let items: Vec<ApprovalSlaItem> = rows
        .into_iter()
        .map(|row| {
            let sla_state = if row.6 >= default_sla_hours as f64 {
                "breached"
            } else if row.6 >= near_breach_hours {
                "near_breach"
            } else {
                "within_sla"
            };
            ApprovalSlaItem {
                invoice_id: row.0,
                invoice_number: row.1,
                vendor_name: row.2,
                amount_cents: row.3,
                currency: row.4,
                approval_id: row.5,
                hours_waiting: row.6,
                sla_hours: default_sla_hours,
                sla_state: sla_state.to_string(),
                approver_name: row.7,
            }
        })
        .collect();

    let near_breach_count = items
        .iter()
        .filter(|item| item.sla_state == "near_breach")
        .count() as i64;
    let breached_count = items
        .iter()
        .filter(|item| item.sla_state == "breached")
        .count() as i64;

    Ok(Json(ApprovalSlaSummary {
        pending_count: items.len() as i64,
        near_breach_count,
        breached_count,
        items,
    }))
}

#[derive(Debug, Serialize)]
pub struct CashFlowObligation {
    pub invoice_id: Uuid,
    pub invoice_number: String,
    pub vendor_name: String,
    pub due_date: Option<chrono::NaiveDate>,
    pub projected_payment_date: Option<chrono::NaiveDate>,
    pub amount_cents: i64,
    pub currency: String,
    pub processing_status: String,
    pub late_risk: bool,
}

#[utoipa::path(get, path = "/api/v1/reports/cash-flow/obligations", tag = "Reports", responses((status = 200, description = "Cash-flow obligations")))]
async fn cash_flow_obligations(
    State(state): State<AppState>,
    ReportingAccess(_user, tenant): ReportingAccess,
) -> ApiResult<Json<Vec<CashFlowObligation>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let rows = sqlx::query_as::<_, (
        Uuid,
        String,
        String,
        Option<chrono::NaiveDate>,
        i64,
        String,
        String,
    )>(
        r#"
        SELECT
            id,
            invoice_number,
            vendor_name,
            due_date,
            total_amount_cents,
            currency,
            processing_status
        FROM invoices
        WHERE tenant_id = $1
          AND processing_status IN ('submitted', 'pending_approval', 'approved', 'ready_for_payment')
        ORDER BY due_date NULLS LAST, vendor_name ASC
        LIMIT 250
        "#,
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query cash-flow obligations: {}", e)))?;

    let today = chrono::Utc::now().date_naive();
    let obligations = rows
        .into_iter()
        .map(|row| {
            let projected_payment_date = match row.6.as_str() {
                "approved" | "ready_for_payment" => row.3,
                "pending_approval" => row
                    .3
                    .map(|date| date.max(today + chrono::Duration::days(1))),
                _ => row.3,
            };
            CashFlowObligation {
                invoice_id: row.0,
                invoice_number: row.1,
                vendor_name: row.2,
                due_date: row.3,
                projected_payment_date,
                amount_cents: row.4,
                currency: row.5,
                processing_status: row.6,
                late_risk: row.3.map(|date| date < today).unwrap_or(false),
            }
        })
        .collect();

    Ok(Json(obligations))
}

#[utoipa::path(get, path = "/api/v1/reports/digests", tag = "Reports", responses((status = 200, description = "Report digests")))]
async fn list_digests(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<billforge_reporting::ReportDigest>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let digests = reporting_service
        .list_user_digests(&tenant.tenant_id, &pool, user.user_id.0)
        .await?;

    Ok(Json(digests))
}

#[utoipa::path(post, path = "/api/v1/reports/digests", tag = "Reports", request_body = serde_json::Value, responses((status = 200, description = "Digest created")))]
async fn create_digest(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<billforge_reporting::UpsertDigestRequest>,
) -> ApiResult<Json<billforge_reporting::ReportDigest>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    let digest = reporting_service
        .upsert_digest(&tenant.tenant_id, &pool, user.user_id.0, request)
        .await?;

    Ok(Json(digest))
}

#[utoipa::path(delete, path = "/api/v1/reports/digests/{id}", tag = "Reports", params(("id" = String, Path,)), responses((status = 200, description = "Digest deleted")))]
async fn delete_digest(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(digest_id): Path<Uuid>,
) -> ApiResult<()> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let reporting_service = billforge_reporting::ReportingService::new();

    reporting_service
        .delete_digest(&tenant.tenant_id, &pool, user.user_id.0, digest_id)
        .await?;

    Ok(())
}
