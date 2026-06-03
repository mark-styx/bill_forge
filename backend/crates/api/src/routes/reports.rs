//! Reporting routes (Reporting module)

use crate::error::ApiResult;
use crate::extractors::{AuthUser, ReportingAccess, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use billforge_reporting::DashboardSummary;
use chrono::Datelike;
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
        .route("/cash-flow/forecast", get(ap_cash_flow_forecast))
        .route("/cash-flow/forecast/simulate", post(ap_cash_flow_forecast_simulate))
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
    pub deadline_at: chrono::DateTime<chrono::Utc>,
    pub percent_elapsed: f64,
    pub sla_state: String,
    pub approver_name: Option<String>,
    pub approver_label: String,
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
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            i64,
            String,
            Uuid,
            f64,
            i32,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            serde_json::Value,
            Option<String>,
        ),
    >(
        r#"
        SELECT
            i.id,
            i.invoice_number,
            i.vendor_name,
            i.total_amount_cents,
            i.currency,
            ar.id,
            EXTRACT(EPOCH FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at))) / 3600.0 AS hours_waiting,
            COALESCE(ar.sla_hours, 24) AS sla_hours,
            COALESCE(ar.sla_started_at, ar.created_at) AS sla_started_at,
            COALESCE(
                ar.expires_at,
                COALESCE(ar.sla_started_at, ar.created_at) + (COALESCE(ar.sla_hours, 24)::text || ' hours')::interval
            ) AS deadline_at,
            ar.requested_from,
            u.name AS approver_name
        FROM approval_requests ar
        JOIN invoices i ON i.id = ar.invoice_id
        LEFT JOIN users u ON u.id = NULLIF(ar.requested_from->>'User', '')::uuid
        WHERE ar.tenant_id = $1 AND ar.status = 'pending'
        ORDER BY deadline_at ASC
        LIMIT 100
        "#,
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to query approval SLA data: {}", e))
    })?;

    let items: Vec<ApprovalSlaItem> = rows
        .into_iter()
        .map(|row| {
            let percent_elapsed = if row.7 > 0 {
                ((row.6 / row.7 as f64) * 100.0).clamp(0.0, 999.0)
            } else {
                0.0
            };
            let sla_state = if percent_elapsed >= 100.0 {
                "breached"
            } else if percent_elapsed >= 80.0 {
                "near_breach"
            } else {
                "within_sla"
            };
            let approver_label = approval_target_label(&row.10, row.11.as_deref());
            ApprovalSlaItem {
                invoice_id: row.0,
                invoice_number: row.1,
                vendor_name: row.2,
                amount_cents: row.3,
                currency: row.4,
                approval_id: row.5,
                hours_waiting: row.6,
                sla_hours: row.7,
                deadline_at: row.9,
                percent_elapsed,
                sla_state: sla_state.to_string(),
                approver_name: row.11,
                approver_label,
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

fn approval_target_label(requested_from: &serde_json::Value, user_name: Option<&str>) -> String {
    if let Some(name) = user_name {
        return name.to_string();
    }
    if let Some(role) = requested_from.get("Role").and_then(|value| value.as_str()) {
        return format!("Role: {}", role);
    }
    if let Some(users) = requested_from
        .get("AnyOf")
        .and_then(|value| value.as_array())
    {
        return format!("Any of {} approvers", users.len());
    }
    if let Some(users) = requested_from
        .get("AllOf")
        .and_then(|value| value.as_array())
    {
        return format!("All {} approvers", users.len());
    }
    if requested_from.get("User").is_some() {
        return "Assigned user".to_string();
    }
    "Approver".to_string()
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

// ---------------------------------------------------------------------------
// AP-driven cash flow forecast (13-week CFO view)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForecastBreakdownEntry {
    pub name: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForecastDay {
    pub date: chrono::NaiveDate,
    pub expected_amount: i64,
    pub low_band: i64,
    pub high_band: i64,
    pub vendor_breakdown: Vec<ForecastBreakdownEntry>,
    pub gl_breakdown: Vec<ForecastBreakdownEntry>,
    pub funding_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForecastWeek {
    pub week_start: chrono::NaiveDate,
    pub week_end: chrono::NaiveDate,
    pub expected_amount: i64,
    pub low_band: i64,
    pub high_band: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForecastMonth {
    pub month: String,
    pub expected_amount: i64,
    pub low_band: i64,
    pub high_band: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApCashFlowForecast {
    pub as_of_date: chrono::NaiveDate,
    pub horizon_weeks: i32,
    pub daily: Vec<ForecastDay>,
    pub weekly: Vec<ForecastWeek>,
    pub monthly: Vec<ForecastMonth>,
}

/// Scenario adjustments for the what-if simulator.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScenarioInputs {
    /// Delay pending-approval invoices by N days (0-30).
    pub pending_approval_delay_days: Option<i32>,
    /// If true, force EPD capture whenever the discount deadline is still in the future.
    pub capture_all_epd: Option<bool>,
    /// Shift effective date for non-approved invoices by N days (-30 to +30).
    pub vendor_term_shift_days: Option<i32>,
    /// Override the daily funding-alert threshold (cents).
    pub override_funding_threshold_cents: Option<i64>,
}

/// A simulated forecast paired with the baseline for comparison.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApCashFlowSimulation {
    pub baseline: ApCashFlowForecast,
    pub scenario: ApCashFlowForecast,
    pub scenario_inputs: ScenarioInputs,
}

/// Request body for the simulate endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SimulateRequest {
    pub horizon_weeks: Option<i32>,
    pub as_of_date: Option<chrono::NaiveDate>,
    pub min_daily_funding_threshold: Option<i64>,
    pub scenario: ScenarioInputs,
}

#[derive(Debug, Deserialize)]
struct ForecastQuery {
    horizon_weeks: Option<i32>,
    as_of_date: Option<chrono::NaiveDate>,
    min_daily_funding_threshold: Option<i64>,
}

#[derive(Debug)]
struct InvoiceForecastRow {
    invoice_id: Uuid,
    vendor_name: String,
    amount_cents: i64,
    processing_status: String,
    due_date: Option<chrono::NaiveDate>,
    discount_deadline: Option<chrono::NaiveDate>,
    discount_percent: Option<f64>,
    discount_captured_at: Option<chrono::DateTime<chrono::Utc>>,
    discount_missed_at: Option<chrono::DateTime<chrono::Utc>>,
    gl_code: Option<String>,
}

/// Return the confidence factor for a given approval status.
/// Returns (low_factor, high_factor) relative to the expected amount.
fn confidence_band(status: &str) -> (f64, f64) {
    match status {
        "approved" | "ready_for_payment" => (1.0, 1.0),
        "pending_approval" => (0.85, 1.15),
        "submitted" => (0.70, 1.30),
        _ => (0.70, 1.30),
    }
}

#[utoipa::path(get, path = "/api/v1/reports/cash-flow/forecast", tag = "Reports", responses((status = 200, description = "AP cash flow forecast")))]
async fn ap_cash_flow_forecast(
    State(state): State<AppState>,
    ReportingAccess(_user, tenant): ReportingAccess,
    Query(params): Query<ForecastQuery>,
) -> ApiResult<Json<ApCashFlowForecast>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let horizon_weeks = params.horizon_weeks.unwrap_or(13).clamp(1, 52);
    let as_of_date = params
        .as_of_date
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    let forecast = compute_ap_forecast(
        &pool,
        tenant.tenant_id.as_uuid(),
        horizon_weeks,
        as_of_date,
        params.min_daily_funding_threshold,
        None,
    )
    .await?;

    Ok(Json(forecast))
}

/// Core forecast computation shared by GET and simulate endpoints.
async fn compute_ap_forecast(
    pool: &sqlx::PgPool,
    tenant_id: &Uuid,
    horizon_weeks: i32,
    as_of_date: chrono::NaiveDate,
    min_daily_funding_threshold: Option<i64>,
    scenario: Option<&ScenarioInputs>,
) -> ApiResult<ApCashFlowForecast> {
    let horizon_end = as_of_date + chrono::Duration::weeks(horizon_weeks as i64);

    // Fetch active invoices with their EPD data and GL codes
    let rows = sqlx::query_as::<_, (
        Uuid,
        String,
        i64,
        String,
        Option<chrono::NaiveDate>,
        Option<chrono::NaiveDate>,
        Option<f64>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    )>(
        r#"
        SELECT
            i.id,
            i.vendor_name,
            i.total_amount_cents,
            i.processing_status,
            i.due_date,
            i.discount_deadline,
            CAST(i.discount_percent AS DOUBLE PRECISION),
            i.discount_captured_at,
            i.discount_missed_at,
            cs.suggested_gl_code
        FROM invoices i
        LEFT JOIN LATERAL (
            SELECT suggested_gl_code
            FROM category_suggestions
            WHERE invoice_id = i.id
              AND accepted_gl_code IS TRUE
              AND suggested_gl_code IS NOT NULL
            LIMIT 1
        ) cs ON TRUE
        WHERE i.tenant_id = $1
          AND i.processing_status IN ('submitted', 'pending_approval', 'approved', 'ready_for_payment')
          AND i.due_date IS NOT NULL
        ORDER BY i.due_date ASC
        LIMIT 1000
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query forecast data: {}", e)))?;

    let today = chrono::Utc::now().date_naive();

    // Build per-invoice forecast rows with effective pay date
    let invoice_rows: Vec<InvoiceForecastRow> = rows
        .into_iter()
        .map(|row| InvoiceForecastRow {
            invoice_id: row.0,
            vendor_name: row.1,
            amount_cents: row.2,
            processing_status: row.3,
            due_date: row.4,
            discount_deadline: row.5,
            discount_percent: row.6,
            discount_captured_at: row.7,
            discount_missed_at: row.8,
            gl_code: row.9,
        })
        .collect();

    // For each invoice, compute effective_pay_date
    // Returns (effective_date, effective_amount_cents)
    let invoice_contributions: Vec<(chrono::NaiveDate, i64, f64, f64, &InvoiceForecastRow)> =
        invoice_rows
            .iter()
            .filter_map(|inv| {
                let due = inv.due_date?;
                let mut effective_date = due;
                let mut effective_amount = inv.amount_cents as f64;

                // Projected payment date adjustment (same logic as cash_flow_obligations)
                match inv.processing_status.as_str() {
                    "approved" | "ready_for_payment" => {}
                    "pending_approval" => {
                        effective_date = due.max(today + chrono::Duration::days(1));
                    }
                    _ => {}
                }

                // --- Scenario adjustments ---
                if let Some(sc) = scenario {
                    // pending_approval_delay_days: shift pending-approval invoices
                    if let Some(delay) = sc.pending_approval_delay_days {
                        if delay > 0 && inv.processing_status == "pending_approval" {
                            effective_date = effective_date + chrono::Duration::days(delay as i64);
                        }
                    }

                    // capture_all_epd: force EPD capture when deadline is in the future
                    if sc.capture_all_epd.unwrap_or(false) {
                        let epd_eligible = inv.discount_captured_at.is_none()
                            && inv.discount_missed_at.is_none()
                            && inv.discount_deadline.is_some()
                            && inv.discount_percent.map_or(false, |p| p > 0.0);
                        if epd_eligible {
                            if let Some(dd) = inv.discount_deadline {
                                if dd >= today {
                                    // Force capture regardless of deadline vs effective_date
                                    effective_date = dd;
                                    if let Some(pct) = inv.discount_percent {
                                        effective_amount =
                                            inv.amount_cents as f64 * (1.0 - pct / 100.0);
                                    }
                                }
                            }
                        }
                    } else {
                        // Standard EPD logic
                        let epd_active = inv.discount_captured_at.is_none()
                            && inv.discount_missed_at.is_none()
                            && inv.discount_deadline.is_some()
                            && inv.discount_percent.map_or(false, |p| p > 0.0);

                        if epd_active {
                            if let Some(dd) = inv.discount_deadline {
                                if dd >= today && dd <= effective_date {
                                    effective_date = dd;
                                    if let Some(pct) = inv.discount_percent {
                                        effective_amount =
                                            inv.amount_cents as f64 * (1.0 - pct / 100.0);
                                    }
                                }
                            }
                        }
                    }

                    // vendor_term_shift_days: shift effective date for non-approved invoices
                    if let Some(shift) = sc.vendor_term_shift_days {
                        if !matches!(
                            inv.processing_status.as_str(),
                            "approved" | "ready_for_payment"
                        ) {
                            effective_date =
                                effective_date + chrono::Duration::days(shift as i64);
                            effective_date = effective_date.max(today);
                        }
                    }
                } else {
                    // Standard EPD logic (no scenario)
                    let epd_active = inv.discount_captured_at.is_none()
                        && inv.discount_missed_at.is_none()
                        && inv.discount_deadline.is_some()
                        && inv.discount_percent.map_or(false, |p| p > 0.0);

                    if epd_active {
                        if let Some(dd) = inv.discount_deadline {
                            if dd >= today && dd <= effective_date {
                                effective_date = dd;
                                if let Some(pct) = inv.discount_percent {
                                    effective_amount =
                                        inv.amount_cents as f64 * (1.0 - pct / 100.0);
                                }
                            }
                        }
                    }
                }

                // Only include invoices within the forecast horizon
                if effective_date < as_of_date || effective_date > horizon_end {
                    return None;
                }

                let (low_f, high_f) = confidence_band(&inv.processing_status);
                Some((effective_date, effective_amount as i64, low_f, high_f, inv))
            })
            .collect();

    // Build daily buckets
    let num_days = (horizon_end - as_of_date).num_days() as usize;
    let mut daily_expected: Vec<i64> = vec![0; num_days];
    let mut daily_low: Vec<i64> = vec![0; num_days];
    let mut daily_high: Vec<i64> = vec![0; num_days];
    let mut daily_vendors: Vec<std::collections::HashMap<String, i64>> =
        vec![std::collections::HashMap::new(); num_days];
    let mut daily_gl: Vec<std::collections::HashMap<String, i64>> =
        vec![std::collections::HashMap::new(); num_days];

    for (date, amount, low_f, high_f, _inv) in &invoice_contributions {
        let offset = (*date - as_of_date).num_days() as usize;
        if offset < num_days {
            daily_expected[offset] += amount;
            daily_low[offset] += (*amount as f64 * low_f) as i64;
            daily_high[offset] += (*amount as f64 * high_f) as i64;
        }
    }

    // Aggregate vendor and GL breakdowns per day
    for (date, amount, _low_f, _high_f, inv) in &invoice_contributions {
        let offset = (*date - as_of_date).num_days() as usize;
        if offset < num_days {
            *daily_vendors[offset]
                .entry(inv.vendor_name.clone())
                .or_insert(0) += amount;
            if let Some(ref gl) = inv.gl_code {
                *daily_gl[offset].entry(gl.clone()).or_insert(0) += amount;
            } else {
                *daily_gl[offset]
                    .entry("Uncategorized".to_string())
                    .or_insert(0) += amount;
            }
        }
    }

    // Compute funding threshold: if provided use it, otherwise compute median.
    // Scenario may override the threshold.
    let threshold_override = scenario.and_then(|s| s.override_funding_threshold_cents);
    let funding_threshold = threshold_override.unwrap_or_else(|| {
        let nonzero_amounts: Vec<i64> =
            daily_expected.iter().copied().filter(|&a| a > 0).collect();
        let median_amount = if nonzero_amounts.is_empty() {
            0i64
        } else {
            let mut sorted = nonzero_amounts.clone();
            sorted.sort();
            sorted[sorted.len() / 2]
        };
        min_daily_funding_threshold
            .unwrap_or_else(|| (median_amount as f64 * 1.5) as i64)
    });

    // Build daily forecast
    let daily: Vec<ForecastDay> = (0..num_days)
        .map(|i| {
            let date = as_of_date + chrono::Duration::days(i as i64);
            let expected = daily_expected[i];
            let mut vendor_breakdown: Vec<ForecastBreakdownEntry> = daily_vendors[i]
                .iter()
                .map(|(k, &v)| ForecastBreakdownEntry {
                    name: k.clone(),
                    amount_cents: v,
                })
                .collect();
            vendor_breakdown.sort_by(|a, b| b.amount_cents.cmp(&a.amount_cents));
            let mut gl_breakdown: Vec<ForecastBreakdownEntry> = daily_gl[i]
                .iter()
                .map(|(k, &v)| ForecastBreakdownEntry {
                    name: k.clone(),
                    amount_cents: v,
                })
                .collect();
            gl_breakdown.sort_by(|a, b| b.amount_cents.cmp(&a.amount_cents));
            ForecastDay {
                date,
                expected_amount: expected,
                low_band: daily_low[i],
                high_band: daily_high[i],
                vendor_breakdown,
                gl_breakdown,
                funding_required: funding_threshold > 0 && expected > funding_threshold,
            }
        })
        .collect();

    // Build weekly buckets
    let num_weeks = horizon_weeks as usize;
    let weekly: Vec<ForecastWeek> = (0..num_weeks)
        .map(|w| {
            let week_start = as_of_date + chrono::Duration::weeks(w as i64);
            let week_end = (week_start + chrono::Duration::days(6)).min(horizon_end);
            let start_offset = (week_start - as_of_date).num_days() as usize;
            let end_offset = ((week_end - as_of_date).num_days() as usize).min(num_days);
            let mut expected = 0i64;
            let mut low = 0i64;
            let mut high = 0i64;
            for d in start_offset..=end_offset {
                if d < num_days {
                    expected += daily_expected[d];
                    low += daily_low[d];
                    high += daily_high[d];
                }
            }
            ForecastWeek {
                week_start,
                week_end,
                expected_amount: expected,
                low_band: low,
                high_band: high,
            }
        })
        .collect();

    // Build monthly buckets
    let mut month_map: std::collections::BTreeMap<String, (i64, i64, i64)> =
        std::collections::BTreeMap::new();
    for day in &daily {
        let month_label = format!("{}-{:02}", day.date.year(), day.date.month());
        let entry = month_map.entry(month_label).or_insert((0, 0, 0));
        entry.0 += day.expected_amount;
        entry.1 += day.low_band;
        entry.2 += day.high_band;
    }
    let monthly: Vec<ForecastMonth> = month_map
        .into_iter()
        .map(|(month, (expected, low, high))| ForecastMonth {
            month,
            expected_amount: expected,
            low_band: low,
            high_band: high,
        })
        .collect();

    Ok(ApCashFlowForecast {
        as_of_date,
        horizon_weeks,
        daily,
        weekly,
        monthly,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/reports/cash-flow/forecast/simulate",
    tag = "Reports",
    request_body = SimulateRequest,
    responses((status = 200, description = "Cash flow simulation with baseline comparison"))
)]
async fn ap_cash_flow_forecast_simulate(
    State(state): State<AppState>,
    ReportingAccess(_user, tenant): ReportingAccess,
    Json(body): Json<SimulateRequest>,
) -> ApiResult<Json<ApCashFlowSimulation>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let horizon_weeks = body.horizon_weeks.unwrap_or(13).clamp(1, 52);
    let as_of_date = body
        .as_of_date
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    // Run baseline (no scenario)
    let baseline = compute_ap_forecast(
        &pool,
        tenant.tenant_id.as_uuid(),
        horizon_weeks,
        as_of_date,
        body.min_daily_funding_threshold,
        None,
    )
    .await?;

    // Run scenario
    let scenario_forecast = compute_ap_forecast(
        &pool,
        tenant.tenant_id.as_uuid(),
        horizon_weeks,
        as_of_date,
        body.min_daily_funding_threshold,
        Some(&body.scenario),
    )
    .await?;

    Ok(Json(ApCashFlowSimulation {
        baseline,
        scenario: scenario_forecast,
        scenario_inputs: body.scenario,
    }))
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
