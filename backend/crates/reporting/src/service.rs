//! Reporting service with actual database queries
//!
//! Provides analytics and reporting capabilities for BillForge platform.

use crate::models::*;
use billforge_core::{types::TenantId, Result, Error};
use sqlx::{PgPool, Row, Column};
use std::sync::Arc;
use chrono::{NaiveDate, DateTime, Utc, Duration, Datelike, Timelike};

// Re-export models used in public API
pub use crate::models::{
    SpendTrendPoint, CategoryBreakdown, VendorPerformanceMetrics, ApprovalAnalytics,
};

/// Service for generating reports from tenant data
pub struct ReportingService {
    // Service is stateless - database access is passed per-method
}

impl ReportingService {
    pub fn new() -> Self {
        Self {}
    }

    /// Get dashboard summary metrics for a tenant
    pub async fn get_dashboard_summary(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
    ) -> Result<DashboardSummary> {
        // Count invoices pending review (capture_status in pending, ready_for_review)
        let invoices_pending_review: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE capture_status IN ('pending', 'ready_for_review')"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        // Count invoices pending approval
        let invoices_pending_approval: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status = 'pending_approval'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        // Count invoices ready for payment
        let invoices_ready_for_payment: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status IN ('approved', 'ready_for_payment')"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        // Count invoices processed today
        let invoices_processed_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE DATE(created_at) = CURRENT_DATE AND capture_status = 'reviewed'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        // Total pending amount (cents)
        let total_pending_cents: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_amount), 0) FROM invoices WHERE processing_status NOT IN ('paid', 'voided', 'rejected')"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        // Count active vendors
        let vendors_active: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vendors WHERE status = 'active'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        // Calculate average processing time (hours) for paid invoices
        // Using time from created_at to when it was paid (approximated by updated_at for paid status)
        let avg_processing_time_hours: f64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600),
                0.0
            )
            FROM invoices
            WHERE processing_status = 'paid'
            AND created_at >= NOW() - INTERVAL '30 days'
            "#
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0.0);

        Ok(DashboardSummary {
            invoices_pending_review: invoices_pending_review as u64,
            invoices_pending_approval: invoices_pending_approval as u64,
            invoices_ready_for_payment: invoices_ready_for_payment as u64,
            invoices_processed_today: invoices_processed_today as u64,
            total_pending_amount: (total_pending_cents as f64) / 100.0,
            vendors_active: vendors_active as u64,
            avg_processing_time_hours,
        })
    }

    /// Get invoice volume over time
    pub async fn get_invoice_volume(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        group_by: &str,
    ) -> Result<Vec<InvoiceVolume>> {
        // Determine date format based on grouping
        let date_expr = match group_by {
            "day" => "to_char(invoice_date, 'YYYY-MM-DD')",
            "week" => "to_char(invoice_date, 'YYYY-WW')",
            "month" => "to_char(invoice_date, 'YYYY-MM')",
            "quarter" => "to_char(invoice_date, 'YYYY-Q')",
            "year" => "to_char(invoice_date, 'YYYY')",
            _ => "to_char(invoice_date, 'YYYY-MM')",
        };

        let sql = format!(
            r#"
            SELECT
                {} as period,
                COUNT(*) as count,
                COALESCE(SUM(total_amount_cents), 0) as total_amount
            FROM invoices
            WHERE invoice_date >= $1 AND invoice_date <= $2
            GROUP BY period
            ORDER BY period ASC
            "#,
            date_expr
        );

        #[derive(sqlx::FromRow)]
        struct VolumeRow {
            period: String,
            count: i64,
            total_amount: i64,
        }

        let rows = sqlx::query_as::<_, VolumeRow>(&sql)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query invoice volume: {}", e)))?;

        let results = rows
            .into_iter()
            .map(|row| InvoiceVolume {
                period: row.period,
                count: row.count as u64,
                total_amount: (row.total_amount as f64) / 100.0,
            })
            .collect();

        Ok(results)
    }

    /// Get spend by vendor (top vendors by spend)
    pub async fn get_vendor_spend(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        limit: u32,
    ) -> Result<Vec<VendorSpend>> {
        #[derive(sqlx::FromRow)]
        struct VendorSpendRow {
            vendor_id: Option<String>,
            vendor_name: String,
            invoice_count: i64,
            total_spend: i64,
            last_invoice_date: Option<NaiveDate>,
        }

        // Build base query
        let mut query_str = String::from(
            r#"
            SELECT
                i.vendor_id::text,
                COALESCE(v.name, i.vendor_name) as vendor_name,
                COUNT(*) as invoice_count,
                COALESCE(SUM(i.total_amount_cents), 0) as total_spend,
                MAX(i.invoice_date) as last_invoice_date
            FROM invoices i
            LEFT JOIN vendors v ON i.vendor_id = v.id
            WHERE i.vendor_id IS NOT NULL
            "#
        );

        let mut param_count = 1;

        if start_date.is_some() {
            query_str.push_str(&format!(" AND i.invoice_date >= ${}", param_count));
            param_count += 1;
        }

        if end_date.is_some() {
            query_str.push_str(&format!(" AND i.invoice_date <= ${}", param_count));
            param_count += 1;
        }

        query_str.push_str(&format!(
            r#"
            GROUP BY i.vendor_id, vendor_name
            ORDER BY total_spend DESC
            LIMIT ${}
            "#,
            param_count
        ));

        // Build query dynamically based on provided parameters
        match (start_date, end_date) {
            (Some(start), Some(end)) => {
                let rows = sqlx::query_as::<_, VendorSpendRow>(&query_str)
                    .bind(start)
                    .bind(end)
                    .bind(limit as i32)
                    .fetch_all(&**pool)
                    .await
                    .map_err(|e| Error::Database(format!("Failed to query vendor spend: {}", e)))?;

                Ok(rows.into_iter().map(|row| VendorSpend {
                    vendor_id: row.vendor_id.unwrap_or_default(),
                    vendor_name: row.vendor_name,
                    invoice_count: row.invoice_count as u64,
                    total_spend: (row.total_spend as f64) / 100.0,
                    avg_invoice_amount: if row.invoice_count > 0 {
                        (row.total_spend as f64) / (row.invoice_count as f64) / 100.0
                    } else {
                        0.0
                    },
                    last_invoice_date: row.last_invoice_date,
                }).collect())
            }
            (Some(start), None) => {
                let rows = sqlx::query_as::<_, VendorSpendRow>(&query_str)
                    .bind(start)
                    .bind(limit as i32)
                    .fetch_all(&**pool)
                    .await
                    .map_err(|e| Error::Database(format!("Failed to query vendor spend: {}", e)))?;

                Ok(rows.into_iter().map(|row| VendorSpend {
                    vendor_id: row.vendor_id.unwrap_or_default(),
                    vendor_name: row.vendor_name,
                    invoice_count: row.invoice_count as u64,
                    total_spend: (row.total_spend as f64) / 100.0,
                    avg_invoice_amount: if row.invoice_count > 0 {
                        (row.total_spend as f64) / (row.invoice_count as f64) / 100.0
                    } else {
                        0.0
                    },
                    last_invoice_date: row.last_invoice_date,
                }).collect())
            }
            (None, Some(end)) => {
                let rows = sqlx::query_as::<_, VendorSpendRow>(&query_str)
                    .bind(end)
                    .bind(limit as i32)
                    .fetch_all(&**pool)
                    .await
                    .map_err(|e| Error::Database(format!("Failed to query vendor spend: {}", e)))?;

                Ok(rows.into_iter().map(|row| VendorSpend {
                    vendor_id: row.vendor_id.unwrap_or_default(),
                    vendor_name: row.vendor_name,
                    invoice_count: row.invoice_count as u64,
                    total_spend: (row.total_spend as f64) / 100.0,
                    avg_invoice_amount: if row.invoice_count > 0 {
                        (row.total_spend as f64) / (row.invoice_count as f64) / 100.0
                    } else {
                        0.0
                    },
                    last_invoice_date: row.last_invoice_date,
                }).collect())
            }
            (None, None) => {
                let rows = sqlx::query_as::<_, VendorSpendRow>(&query_str)
                    .bind(limit as i32)
                    .fetch_all(&**pool)
                    .await
                    .map_err(|e| Error::Database(format!("Failed to query vendor spend: {}", e)))?;

                Ok(rows.into_iter().map(|row| VendorSpend {
                    vendor_id: row.vendor_id.unwrap_or_default(),
                    vendor_name: row.vendor_name,
                    invoice_count: row.invoice_count as u64,
                    total_spend: (row.total_spend as f64) / 100.0,
                    avg_invoice_amount: if row.invoice_count > 0 {
                        (row.total_spend as f64) / (row.invoice_count as f64) / 100.0
                    } else {
                        0.0
                    },
                    last_invoice_date: row.last_invoice_date,
                }).collect())
            }
        }
    }

    /// Get YTD spend by vendor
    pub async fn get_vendor_spend_ytd(
        &self,
        tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        limit: u32,
    ) -> Result<Vec<VendorSpend>> {
        let now = Utc::now().naive_utc().date();
        let year_start = NaiveDate::from_ymd_opt(now.year(), 1, 1)
            .ok_or_else(|| Error::Validation("Invalid year start date".to_string()))?;

        self.get_vendor_spend(tenant_id, pool, Some(year_start), Some(now), limit).await
    }

    /// Get MTD spend by vendor
    pub async fn get_vendor_spend_mtd(
        &self,
        tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        limit: u32,
    ) -> Result<Vec<VendorSpend>> {
        let now = Utc::now().naive_utc().date();
        let month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .ok_or_else(|| Error::Validation("Invalid month start date".to_string()))?;

        self.get_vendor_spend(tenant_id, pool, Some(month_start), Some(now), limit).await
    }

    /// Get count of invoices processed this week (Monday to current day)
    pub async fn get_invoices_processed_this_week(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
    ) -> Result<u64> {
        // Calculate the Monday of the current week
        let now = Utc::now().naive_utc().date();
        let weekday = now.weekday().num_days_from_monday() as i32;
        let week_start = now - Duration::days(weekday as i64);

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM invoices
            WHERE capture_status = 'reviewed'
            AND DATE(created_at) >= $1
            "#
        )
        .bind(week_start)
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        Ok(count as u64)
    }

    /// Get invoice aging report (buckets: 0-30, 31-60, 61-90, 90+ days)
    pub async fn get_invoice_aging(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
    ) -> Result<Vec<AgingBucket>> {
        #[derive(sqlx::FromRow)]
        struct AgingRow {
            bucket: String,
            days_min: i32,
            days_max: Option<i32>,
            invoice_count: i64,
            total_amount: i64,
        }

        // Query invoices that are unpaid and group by age
        let sql = r#"
            SELECT
                CASE
                    WHEN CURRENT_DATE - due_date <= 0 THEN 'Current'
                    WHEN CURRENT_DATE - due_date <= 30 THEN '1-30 days'
                    WHEN CURRENT_DATE - due_date <= 60 THEN '31-60 days'
                    WHEN CURRENT_DATE - due_date <= 90 THEN '61-90 days'
                    ELSE '90+ days'
                END as bucket,
                CASE
                    WHEN CURRENT_DATE - due_date <= 0 THEN 0
                    WHEN CURRENT_DATE - due_date <= 30 THEN 1
                    WHEN CURRENT_DATE - due_date <= 60 THEN 31
                    WHEN CURRENT_DATE - due_date <= 90 THEN 61
                    ELSE 91
                END as days_min,
                CASE
                    WHEN CURRENT_DATE - due_date <= 0 THEN 0
                    WHEN CURRENT_DATE - due_date <= 30 THEN 30
                    WHEN CURRENT_DATE - due_date <= 60 THEN 60
                    WHEN CURRENT_DATE - due_date <= 90 THEN 90
                    ELSE NULL
                END as days_max,
                COUNT(*) as invoice_count,
                COALESCE(SUM(total_amount_cents), 0) as total_amount
            FROM invoices
            WHERE processing_status NOT IN ('paid', 'voided', 'rejected')
            AND due_date IS NOT NULL
            GROUP BY bucket, days_min, days_max
            ORDER BY days_min ASC
        "#;

        let rows = sqlx::query_as::<_, AgingRow>(sql)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query aging: {}", e)))?;

        let mut results: Vec<AgingBucket> = rows
            .into_iter()
            .map(|row| AgingBucket {
                bucket_name: row.bucket,
                days_min: row.days_min,
                days_max: row.days_max,
                invoice_count: row.invoice_count as u64,
                total_amount: (row.total_amount as f64) / 100.0,
            })
            .collect();

        // If no results, return default empty buckets
        if results.is_empty() {
            results = vec![
                AgingBucket {
                    bucket_name: "Current".to_string(),
                    days_min: 0,
                    days_max: Some(0),
                    invoice_count: 0,
                    total_amount: 0.0,
                },
                AgingBucket {
                    bucket_name: "1-30 days".to_string(),
                    days_min: 1,
                    days_max: Some(30),
                    invoice_count: 0,
                    total_amount: 0.0,
                },
                AgingBucket {
                    bucket_name: "31-60 days".to_string(),
                    days_min: 31,
                    days_max: Some(60),
                    invoice_count: 0,
                    total_amount: 0.0,
                },
                AgingBucket {
                    bucket_name: "61-90 days".to_string(),
                    days_min: 61,
                    days_max: Some(90),
                    invoice_count: 0,
                    total_amount: 0.0,
                },
                AgingBucket {
                    bucket_name: "90+ days".to_string(),
                    days_min: 91,
                    days_max: None,
                    invoice_count: 0,
                    total_amount: 0.0,
                },
            ];
        }

        Ok(results)
    }

    /// Get processing metrics
    pub async fn get_processing_metrics(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
    ) -> Result<ProcessingMetrics> {
        // Average capture time (from created to reviewed)
        let avg_capture_time_minutes: f64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 60),
                0.0
            )
            FROM invoices
            WHERE capture_status = 'reviewed'
            AND created_at >= NOW() - INTERVAL '30 days'
            "#
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0.0);

        // Average approval time (from submitted to approved)
        let avg_approval_time_hours: f64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600),
                0.0
            )
            FROM invoices
            WHERE processing_status IN ('approved', 'ready_for_payment', 'paid')
            AND created_at >= NOW() - INTERVAL '30 days'
            "#
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0.0);

        // Average total processing time (from created to paid)
        let avg_total_processing_time_hours: f64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600),
                0.0
            )
            FROM invoices
            WHERE processing_status = 'paid'
            AND created_at >= NOW() - INTERVAL '30 days'
            "#
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0.0);

        // Count total and auto-approved for rate calculation
        let total_approved: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status IN ('approved', 'ready_for_payment', 'paid') AND created_at >= NOW() - INTERVAL '30 days'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        let total_rejected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status = 'rejected' AND created_at >= NOW() - INTERVAL '30 days'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        let total_processed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status NOT IN ('draft', 'submitted') AND created_at >= NOW() - INTERVAL '30 days'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(1); // Avoid division by zero

        // First pass rate (invoices that didn't go on hold or get rejected first)
        let first_pass_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status = 'paid' AND notes NOT LIKE '%hold%' AND created_at >= NOW() - INTERVAL '30 days'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(0);

        let paid_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE processing_status = 'paid' AND created_at >= NOW() - INTERVAL '30 days'"
        )
        .fetch_one(&**pool)
        .await
        .unwrap_or(1);

        Ok(ProcessingMetrics {
            avg_capture_time_minutes,
            avg_approval_time_hours,
            avg_total_processing_time_hours,
            auto_approval_rate: 0.0, // Would need workflow rule tracking to calculate
            rejection_rate: if total_processed > 0 {
                (total_rejected as f64) / (total_processed as f64)
            } else {
                0.0
            },
            first_pass_rate: if paid_count > 0 {
                (first_pass_count as f64) / (paid_count as f64)
            } else {
                0.0
            },
        })
    }

    /// Get invoice status distribution
    pub async fn get_status_distribution(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
    ) -> Result<Vec<StatusDistribution>> {
        #[derive(sqlx::FromRow)]
        struct StatusRow {
            status: String,
            count: i64,
            total_amount: i64,
        }

        let sql = r#"
            SELECT
                processing_status as status,
                COUNT(*) as count,
                COALESCE(SUM(total_amount_cents), 0) as total_amount
            FROM invoices
            GROUP BY processing_status
            ORDER BY count DESC
        "#;

        let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices")
            .fetch_one(&**pool)
            .await
            .unwrap_or(1);

        let rows = sqlx::query_as::<_, StatusRow>(sql)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query status distribution: {}", e)))?;

        let results = rows
            .into_iter()
            .map(|row| StatusDistribution {
                status: row.status,
                count: row.count as u64,
                percentage: if total_count > 0 {
                    (row.count as f64) / (total_count as f64) * 100.0
                } else {
                    0.0
                },
                total_amount: (row.total_amount as f64) / 100.0,
            })
            .collect();

        Ok(results)
    }

    /// Execute a custom report query
    pub async fn execute_custom_report(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        query: CustomReportQuery,
    ) -> Result<CustomReportResult> {
        // Build query based on report type
        let sql = match query.report_type.as_str() {
            "invoices_by_department" => {
                r#"
                SELECT
                    COALESCE(department, 'Unassigned') as department,
                    COUNT(*) as invoice_count,
                    SUM(total_amount_cents) / 100.0 as total_amount
                FROM invoices
                WHERE invoice_date >= $1 AND invoice_date <= $2
                GROUP BY department
                ORDER BY total_amount DESC
                "#.to_string()
            }
            "invoices_by_gl_code" => {
                r#"
                SELECT
                    COALESCE(gl_code, 'Unassigned') as gl_code,
                    COUNT(*) as invoice_count,
                    SUM(total_amount_cents) / 100.0 as total_amount
                FROM invoices
                WHERE invoice_date >= $1 AND invoice_date <= $2
                GROUP BY gl_code
                ORDER BY total_amount DESC
                "#.to_string()
            }
            "approval_summary" => {
                r#"
                SELECT
                    status,
                    COUNT(*) as count,
                    SUM(CASE WHEN comments IS NOT NULL THEN 1 ELSE 0 END) as with_comments
                FROM approval_requests
                WHERE created_at >= $1 AND created_at <= $2
                GROUP BY status
                "#.to_string()
            }
            _ => {
                return Err(Error::Validation(format!("Unknown report type: {}", query.report_type)));
            }
        };

        // Get date range
        let (start_date, end_date) = if let Some(ref range) = query.date_range {
            (range.start, range.end)
        } else {
            // Default to last 30 days
            let end = Utc::now().naive_utc().date();
            let start = end - Duration::days(30);
            (start, end)
        };

        // Execute query and get raw rows
        let rows = sqlx::query(&sql)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to execute custom report: {}", e)))?;

        // Extract column names from the first row if available
        let columns: Vec<String> = if !rows.is_empty() {
            rows[0]
                .columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect()
        } else {
            // Return empty result if no rows
            return Ok(CustomReportResult {
                columns: vec![],
                rows: vec![],
                total_rows: 0,
                generated_at: Utc::now(),
            });
        };

        // Convert rows to JSON values
        let mut result_rows = Vec::new();
        for row in rows {
            let mut values = Vec::new();
            for (i, _column) in columns.iter().enumerate() {
                // Try different types
                if let Ok(v) = row.try_get::<String, _>(i) {
                    values.push(serde_json::Value::String(v));
                } else if let Ok(v) = row.try_get::<i64, _>(i) {
                    values.push(serde_json::Value::Number(v.into()));
                } else if let Ok(v) = row.try_get::<f64, _>(i) {
                    values.push(serde_json::json!(v));
                } else {
                    values.push(serde_json::Value::Null);
                }
            }
            result_rows.push(values);
        }

        let total_rows = result_rows.len() as u64;

        Ok(CustomReportResult {
            columns,
            rows: result_rows,
            total_rows,
            generated_at: Utc::now(),
        })
    }

    /// Get spend trends over time with period-over-period comparison
    pub async fn get_spend_trends(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        group_by: &str,
    ) -> Result<Vec<SpendTrendPoint>> {
        // Determine date format based on grouping
        let date_expr = match group_by {
            "day" => "to_char(invoice_date, 'YYYY-MM-DD')",
            "week" => "to_char(invoice_date, 'YYYY-WW')",
            "month" => "to_char(invoice_date, 'YYYY-MM')",
            "quarter" => "to_char(invoice_date, 'YYYY-Q')",
            "year" => "to_char(invoice_date, 'YYYY')",
            _ => "to_char(invoice_date, 'YYYY-MM')",
        };

        let sql = format!(
            r#"
            WITH period_data AS (
                SELECT
                    {} as period,
                    COUNT(*) as invoice_count,
                    COALESCE(SUM(total_amount_cents), 0) / 100.0 as total_spend
                FROM invoices
                WHERE invoice_date >= $1 AND invoice_date <= $2
                GROUP BY period
            )
            SELECT
                period,
                total_spend,
                invoice_count,
                CASE WHEN invoice_count > 0 THEN total_spend / invoice_count ELSE 0 END as avg_invoice_amount,
                LAG(total_spend) OVER (ORDER BY period) as prior_spend
            FROM period_data
            ORDER BY period ASC
            "#,
            date_expr
        );

        #[derive(sqlx::FromRow)]
        struct TrendRow {
            period: String,
            total_spend: f64,
            invoice_count: i64,
            avg_invoice_amount: f64,
            prior_spend: Option<f64>,
        }

        let rows = sqlx::query_as::<_, TrendRow>(&sql)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query spend trends: {}", e)))?;

        let results = rows
            .into_iter()
            .map(|row| {
                let change_from_prior = row.prior_spend.map(|prior| row.total_spend - prior);
                let change_percentage = row.prior_spend.and_then(|prior| {
                    if prior > 0.0 {
                        Some(((row.total_spend - prior) / prior) * 100.0)
                    } else {
                        None
                    }
                });

                SpendTrendPoint {
                    period: row.period,
                    total_spend: row.total_spend,
                    invoice_count: row.invoice_count as u64,
                    avg_invoice_amount: row.avg_invoice_amount,
                    change_from_prior_period: change_from_prior,
                    change_percentage,
                }
            })
            .collect();

        Ok(results)
    }

    /// Get category breakdown (GL codes, departments, cost centers)
    pub async fn get_category_breakdown(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        category_type: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<Vec<CategoryBreakdown>> {
        let column = match category_type {
            "gl_code" => "gl_code",
            "department" => "department",
            "cost_center" => "cost_center",
            _ => return Err(Error::Validation(format!("Invalid category type: {}", category_type))),
        };

        let mut sql = format!(
            r#"
            SELECT
                COALESCE({}, 'Unassigned') as category_value,
                COUNT(*) as invoice_count,
                COALESCE(SUM(total_amount_cents), 0) / 100.0 as total_amount
            FROM invoices
            "#,
            column
        );

        let mut conditions: Vec<String> = Vec::new();
        if start_date.is_some() {
            conditions.push("invoice_date >= $1".to_string());
        }
        if end_date.is_some() {
            conditions.push(format!("invoice_date <= ${}", if start_date.is_some() { 2 } else { 1 }));
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(&format!(
            " GROUP BY {} ORDER BY total_amount DESC",
            column
        ));

        #[derive(sqlx::FromRow)]
        struct CategoryRow {
            category_value: String,
            invoice_count: i64,
            total_amount: f64,
        }

        let rows = match (start_date, end_date) {
            (Some(start), Some(end)) => {
                sqlx::query_as::<_, CategoryRow>(&sql)
                    .bind(start)
                    .bind(end)
                    .fetch_all(&**pool)
                    .await
            }
            (Some(start), None) => {
                sqlx::query_as::<_, CategoryRow>(&sql)
                    .bind(start)
                    .fetch_all(&**pool)
                    .await
            }
            (None, Some(end)) => {
                sqlx::query_as::<_, CategoryRow>(&sql)
                    .bind(end)
                    .fetch_all(&**pool)
                    .await
            }
            (None, None) => {
                sqlx::query_as::<_, CategoryRow>(&sql)
                    .fetch_all(&**pool)
                    .await
            }
        }
        .map_err(|e| Error::Database(format!("Failed to query category breakdown: {}", e)))?;

        let total_spend: f64 = rows.iter().map(|r| r.total_amount).sum();
        let total_count: u64 = rows.len() as u64;

        let results = rows
            .into_iter()
            .map(|row| CategoryBreakdown {
                category_type: category_type.to_string(),
                category_value: row.category_value,
                invoice_count: row.invoice_count as u64,
                total_amount: row.total_amount,
                percentage_of_total: if total_spend > 0.0 {
                    (row.total_amount / total_spend) * 100.0
                } else {
                    0.0
                },
                avg_amount: if row.invoice_count > 0 {
                    row.total_amount / (row.invoice_count as f64)
                } else {
                    0.0
                },
            })
            .collect();

        Ok(results)
    }

    /// Get vendor performance metrics
    pub async fn get_vendor_performance(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        limit: u32,
    ) -> Result<Vec<VendorPerformanceMetrics>> {
        let sql = r#"
            WITH vendor_stats AS (
                SELECT
                    i.vendor_id::text,
                    COALESCE(v.name, i.vendor_name) as vendor_name,
                    COUNT(*) as total_invoices,
                    COALESCE(SUM(i.total_amount_cents), 0) / 100.0 as total_spend,
                    -- Calculate on-time payment rate (paid before or on due_date)
                    AVG(
                        CASE
                            WHEN i.processing_status = 'paid' AND i.due_date IS NOT NULL THEN
                                CASE
                                    WHEN DATE(i.updated_at) <= i.due_date THEN 1.0
                                    ELSE 0.0
                                END
                            ELSE NULL
                        END
                    ) as on_time_payment_rate,
                    -- Calculate average payment days
                    AVG(
                        CASE
                            WHEN i.processing_status = 'paid' THEN
                                EXTRACT(EPOCH FROM (i.updated_at - i.created_at)) / 86400.0
                            ELSE NULL
                        END
                    ) as avg_payment_days,
                    -- Dispute rate (invoices with notes containing dispute keywords)
                    COUNT(*) FILTER (WHERE i.notes ILIKE '%dispute%' OR i.notes ILIKE '%discrepancy%')::float / COUNT(*) as dispute_rate
                FROM invoices i
                LEFT JOIN vendors v ON i.vendor_id = v.id
                WHERE i.vendor_id IS NOT NULL
                GROUP BY i.vendor_id, vendor_name
            )
            SELECT
                vendor_id,
                vendor_name,
                total_invoices,
                total_spend,
                COALESCE(on_time_payment_rate, 0.0) as on_time_payment_rate,
                COALESCE(avg_payment_days, 0.0) as avg_payment_days,
                COALESCE(dispute_rate, 0.0) as dispute_rate,
                0.0 as credit_utilization,
                -- Calculate reliability score (0-100)
                GREATEST(0, LEAST(100,
                    50.0 + -- Base score
                    COALESCE(on_time_payment_rate, 0.0) * 30 + -- Up to 30 points for on-time payments
                    (1 - COALESCE(dispute_rate, 0.0)) * 20 -- Up to 20 points for low dispute rate
                )) as reliability_score
            FROM vendor_stats
            ORDER BY total_spend DESC
            LIMIT $1
        "#;

        #[derive(sqlx::FromRow)]
        struct VendorPerfRow {
            vendor_id: String,
            vendor_name: String,
            total_invoices: i64,
            total_spend: f64,
            on_time_payment_rate: f64,
            avg_payment_days: f64,
            dispute_rate: f64,
            credit_utilization: f64,
            reliability_score: f64,
        }

        let rows = sqlx::query_as::<_, VendorPerfRow>(sql)
            .bind(limit as i32)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query vendor performance: {}", e)))?;

        let results = rows
            .into_iter()
            .map(|row| VendorPerformanceMetrics {
                vendor_id: row.vendor_id,
                vendor_name: row.vendor_name,
                total_invoices: row.total_invoices as u64,
                total_spend: row.total_spend,
                on_time_payment_rate: row.on_time_payment_rate,
                avg_payment_days: row.avg_payment_days,
                dispute_rate: row.dispute_rate,
                credit_utilization: row.credit_utilization,
                reliability_score: row.reliability_score,
            })
            .collect();

        Ok(results)
    }

    /// Get approval analytics including bottlenecks and approver workloads
    pub async fn get_approval_analytics(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<ApprovalAnalytics> {
        // Build date filter conditions
        let date_filter = match (start_date, end_date) {
            (Some(start), Some(end)) => format!(
                "WHERE ar.created_at >= '{}' AND ar.created_at <= '{}'",
                start, end
            ),
            (Some(start), None) => format!("WHERE ar.created_at >= '{}'", start),
            (None, Some(end)) => format!("WHERE ar.created_at <= '{}'", end),
            (None, None) => String::new(),
        };

        // Get overall approval stats
        let stats_sql = format!(
            r#"
            SELECT
                COUNT(*) as total_approvals,
                AVG(EXTRACT(EPOCH FROM (ar.decided_at - ar.created_at)) / 3600) as avg_approval_time_hours,
                COUNT(*) FILTER (WHERE ar.status = 'approved')::float / NULLIF(COUNT(*), 0) as approval_rate,
                COUNT(*) FILTER (WHERE ar.status = 'rejected')::float / NULLIF(COUNT(*), 0) as rejection_rate
            FROM approval_requests ar
            {}
            "#,
            date_filter
        );

        #[derive(sqlx::FromRow)]
        struct StatsRow {
            total_approvals: i64,
            avg_approval_time_hours: Option<f64>,
            approval_rate: Option<f64>,
            rejection_rate: Option<f64>,
        }

        let stats = sqlx::query_as::<_, StatsRow>(&stats_sql)
            .fetch_one(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query approval stats: {}", e)))?;

        // Get bottleneck stages (workflow steps with longest average times)
        let bottleneck_sql = format!(
            r#"
            SELECT
                COALESCE(ar.level::text, 'Unknown') as stage_name,
                AVG(EXTRACT(EPOCH FROM (ar.decided_at - ar.created_at)) / 3600) as avg_time_hours,
                COUNT(*) as invoice_count
            FROM approval_requests ar
            {}
            GROUP BY ar.level
            ORDER BY avg_time_hours DESC
            LIMIT 5
            "#,
            date_filter
        );

        #[derive(sqlx::FromRow)]
        struct BottleneckRow {
            stage_name: String,
            avg_time_hours: Option<f64>,
            invoice_count: i64,
        }

        let bottleneck_rows = sqlx::query_as::<_, BottleneckRow>(&bottleneck_sql)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query bottlenecks: {}", e)))?;

        let total_time: f64 = bottleneck_rows
            .iter()
            .filter_map(|r| r.avg_time_hours)
            .sum();

        let bottleneck_stages = bottleneck_rows
            .into_iter()
            .map(|row| BottleneckStage {
                stage_name: row.stage_name,
                avg_time_hours: row.avg_time_hours.unwrap_or(0.0),
                invoice_count: row.invoice_count as u64,
                percentage_of_total_time: if total_time > 0.0 && row.avg_time_hours.is_some() {
                    (row.avg_time_hours.unwrap() / total_time) * 100.0
                } else {
                    0.0
                },
            })
            .collect();

        // Get approver workloads
        let workload_sql = format!(
            r#"
            SELECT
                ar.approver_id::text,
                COALESCE(u.email, 'Unknown') as approver_name,
                COUNT(*) FILTER (WHERE ar.status IN ('approved', 'rejected')) as approvals_completed,
                AVG(EXTRACT(EPOCH FROM (ar.decided_at - ar.created_at)) / 3600)
                    FILTER (WHERE ar.status IN ('approved', 'rejected')) as avg_time_to_approve_hours,
                COUNT(*) FILTER (WHERE ar.status = 'pending') as pending_approvals,
                COUNT(*) FILTER (WHERE ar.status = 'approved')::float /
                    NULLIF(COUNT(*) FILTER (WHERE ar.status IN ('approved', 'rejected')), 0) as approval_rate
            FROM approval_requests ar
            LEFT JOIN users u ON ar.approver_id = u.id
            {}
            GROUP BY ar.approver_id, u.email
            ORDER BY approvals_completed DESC
            LIMIT 20
            "#,
            date_filter
        );

        #[derive(sqlx::FromRow)]
        struct WorkloadRow {
            approver_id: String,
            approver_name: String,
            approvals_completed: i64,
            avg_time_to_approve_hours: Option<f64>,
            pending_approvals: i64,
            approval_rate: Option<f64>,
        }

        let workload_rows = sqlx::query_as::<_, WorkloadRow>(&workload_sql)
            .fetch_all(&**pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query approver workloads: {}", e)))?;

        let approver_workloads = workload_rows
            .into_iter()
            .map(|row| ApproverWorkload {
                approver_id: row.approver_id,
                approver_name: row.approver_name,
                approvals_completed: row.approvals_completed as u64,
                avg_time_to_approve_hours: row.avg_time_to_approve_hours.unwrap_or(0.0),
                pending_approvals: row.pending_approvals as u64,
                approval_rate: row.approval_rate.unwrap_or(0.0),
            })
            .collect();

        Ok(ApprovalAnalytics {
            total_approvals: stats.total_approvals as u64,
            avg_approval_time_hours: stats.avg_approval_time_hours.unwrap_or(0.0),
            approval_rate: stats.approval_rate.unwrap_or(0.0),
            rejection_rate: stats.rejection_rate.unwrap_or(0.0),
            bottleneck_stages,
            approver_workloads,
        })
    }

    // === Email Digest Methods ===

    /// Generate digest content for a user
    pub async fn generate_digest_content(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        user_id: uuid::Uuid,
        digest_type: DigestType,
        period_start: NaiveDate,
        period_end: NaiveDate,
    ) -> Result<DigestContent> {
        // Get summary metrics
        let summary = self.get_digest_summary(_tenant_id, pool, period_start, period_end).await?;

        // Get highlights based on digest type
        let highlights = self.get_digest_highlights(_tenant_id, pool, period_start, period_end, &digest_type).await?;

        // Get actionable items (pending approvals for this user)
        let actionable_items = self.get_actionable_items(_tenant_id, pool, user_id).await?;

        Ok(DigestContent {
            digest_type,
            period_start,
            period_end,
            summary,
            highlights,
            actionable_items,
        })
    }

    async fn get_digest_summary(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<DigestSummary> {
        #[derive(sqlx::FromRow)]
        struct SummaryRow {
            total_invoices: i64,
            total_amount_cents: i64,
            pending_approvals: i64,
            approved_count: i64,
            rejected_count: i64,
            avg_processing_time_hours: Option<f64>,
        }

        let row = sqlx::query_as::<_, SummaryRow>(
            r#"
            SELECT
                COUNT(*) as total_invoices,
                COALESCE(SUM(total_amount_cents), 0) as total_amount_cents,
                COUNT(*) FILTER (WHERE status = 'pending_approval') as pending_approvals,
                COUNT(*) FILTER (WHERE status = 'approved' OR status = 'ready_for_payment') as approved_count,
                COUNT(*) FILTER (WHERE status = 'rejected') as rejected_count,
                AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600) as avg_processing_time_hours
            FROM invoices
            WHERE invoice_date >= $1 AND invoice_date <= $2
            "#,
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_one(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to query digest summary: {}", e)))?;

        Ok(DigestSummary {
            total_invoices: row.total_invoices as u64,
            total_amount: row.total_amount_cents as f64 / 100.0,
            pending_approvals: row.pending_approvals as u64,
            approved_count: row.approved_count as u64,
            rejected_count: row.rejected_count as u64,
            avg_processing_time_hours: row.avg_processing_time_hours.unwrap_or(0.0),
        })
    }

    async fn get_digest_highlights(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        digest_type: &DigestType,
    ) -> Result<Vec<DigestHighlight>> {
        let mut highlights = Vec::new();

        // Top vendor by spend
        #[derive(sqlx::FromRow)]
        struct VendorRow {
            vendor_name: String,
            total_spend: i64,
        }

        let top_vendor = sqlx::query_as::<_, VendorRow>(
            r#"
            SELECT v.name as vendor_name, COALESCE(SUM(i.total_amount_cents), 0) as total_spend
            FROM invoices i
            JOIN vendors v ON i.vendor_id = v.id
            WHERE i.invoice_date >= $1 AND i.invoice_date <= $2
            GROUP BY v.name
            ORDER BY total_spend DESC
            LIMIT 1
            "#,
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to query top vendor: {}", e)))?;

        if let Some(vendor) = top_vendor {
            highlights.push(DigestHighlight {
                title: "Top Vendor by Spend".to_string(),
                description: format!("{}", vendor.vendor_name),
                value: Some(vendor.total_spend as f64 / 100.0),
                change_percentage: None,
            });
        }

        // For weekly/monthly, add trend comparison
        if matches!(digest_type, DigestType::WeeklySummary | DigestType::MonthlySummary) {
            let prior_start = start_date - chrono::Duration::days((end_date - start_date).num_days() + 1);
            let prior_end = start_date - chrono::Duration::days(1);

            #[derive(sqlx::FromRow)]
            struct TrendRow {
                current_spend: i64,
                prior_spend: i64,
            }

            let trend = sqlx::query_as::<_, TrendRow>(
                r#"
                SELECT
                    (SELECT COALESCE(SUM(total_amount_cents), 0) FROM invoices WHERE invoice_date >= $1 AND invoice_date <= $2) as current_spend,
                    (SELECT COALESCE(SUM(total_amount_cents), 0) FROM invoices WHERE invoice_date >= $3 AND invoice_date <= $4) as prior_spend
                "#,
            )
            .bind(start_date)
            .bind(end_date)
            .bind(prior_start)
            .bind(prior_end)
            .fetch_one(pool.as_ref())
            .await
            .ok();

            if let Some(trend) = trend {
                let change_pct = if trend.prior_spend > 0 {
                    Some(((trend.current_spend - trend.prior_spend) as f64 / trend.prior_spend as f64) * 100.0)
                } else {
                    None
                };

                highlights.push(DigestHighlight {
                    title: "Spend vs Prior Period".to_string(),
                    description: if change_pct.map_or(false, |p| p > 0.0) {
                        "Increased"
                    } else {
                        "Decreased"
                    }
                    .to_string(),
                    value: Some(trend.current_spend as f64 / 100.0),
                    change_percentage: change_pct,
                });
            }
        }

        Ok(highlights)
    }

    async fn get_actionable_items(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        user_id: uuid::Uuid,
    ) -> Result<Vec<ActionableItem>> {
        #[derive(sqlx::FromRow)]
        struct PendingRow {
            invoice_id: uuid::Uuid,
            invoice_number: String,
            vendor_name: String,
            amount_cents: i64,
            age_days: i64,
        }

        let rows = sqlx::query_as::<_, PendingRow>(
            r#"
            SELECT
                i.id as invoice_id,
                i.invoice_number,
                v.name as vendor_name,
                i.total_amount_cents as amount_cents,
                EXTRACT(DAY FROM NOW() - i.created_at)::bigint as age_days
            FROM invoices i
            JOIN vendors v ON i.vendor_id = v.id
            WHERE i.status = 'pending_approval'
            ORDER BY i.created_at ASC
            LIMIT 10
            "#,
        )
        .bind(user_id)
        .fetch_all(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to query actionable items: {}", e)))?;

        let items = rows
            .into_iter()
            .map(|row| ActionableItem {
                item_type: "pending_approval".to_string(),
                item_id: row.invoice_id.to_string(),
                title: format!("Invoice {} from {}", row.invoice_number, row.vendor_name),
                description: format!("Amount: ${:.2}, Age: {} days", row.amount_cents as f64 / 100.0, row.age_days),
                url: format!("/invoices/{}", row.invoice_id),
                priority: if row.age_days > 7 { "high" } else { "normal" }.to_string(),
            })
            .collect();

        Ok(items)
    }

    /// Get digests that are due for sending
    pub async fn get_due_digests(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
    ) -> Result<Vec<ReportDigest>> {
        #[derive(sqlx::FromRow)]
        struct DigestRow {
            id: uuid::Uuid,
            tenant_id: uuid::Uuid,
            user_id: uuid::Uuid,
            digest_type: String,
            frequency: String,
            enabled: bool,
            filters: serde_json::Value,
            last_sent_at: Option<DateTime<Utc>>,
            next_send_at: DateTime<Utc>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, DigestRow>(
            r#"
            SELECT id, tenant_id, user_id, digest_type, frequency, enabled, filters,
                   last_sent_at, next_send_at, created_at, updated_at
            FROM report_digests
            WHERE enabled = true AND next_send_at <= NOW()
            ORDER BY next_send_at ASC
            LIMIT 100
            "#,
        )
        .fetch_all(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to query due digests: {}", e)))?;

        let digests = rows
            .into_iter()
            .filter_map(|row| {
                let digest_type = match row.digest_type.as_str() {
                    "daily_summary" => Some(DigestType::DailySummary),
                    "weekly_summary" => Some(DigestType::WeeklySummary),
                    "monthly_summary" => Some(DigestType::MonthlySummary),
                    "approval_reminder" => Some(DigestType::ApprovalReminder),
                    _ => None,
                }?;

                let frequency = match row.frequency.as_str() {
                    "daily" => Some(DigestFrequency::Daily),
                    "weekly" => Some(DigestFrequency::Weekly),
                    "monthly" => Some(DigestFrequency::Monthly),
                    _ => None,
                }?;

                Some(ReportDigest {
                    id: row.id,
                    tenant_id: row.tenant_id.to_string(),
                    user_id: row.user_id,
                    digest_type,
                    frequency,
                    enabled: row.enabled,
                    filters: row.filters,
                    last_sent_at: row.last_sent_at,
                    next_send_at: row.next_send_at,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                })
            })
            .collect();

        Ok(digests)
    }

    /// Upsert a digest configuration
    pub async fn upsert_digest(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        user_id: uuid::Uuid,
        request: UpsertDigestRequest,
    ) -> Result<ReportDigest> {
        let digest_type_str = match request.digest_type {
            DigestType::DailySummary => "daily_summary",
            DigestType::WeeklySummary => "weekly_summary",
            DigestType::MonthlySummary => "monthly_summary",
            DigestType::ApprovalReminder => "approval_reminder",
        };

        let filters = request.filters.unwrap_or(serde_json::json!({}));

        // Calculate next send time based on frequency
        let now = Utc::now();
        let next_send_at = match request.frequency {
            DigestFrequency::Daily => now + chrono::Duration::days(1),
            DigestFrequency::Weekly => now + chrono::Duration::weeks(1),
            DigestFrequency::Monthly => {
                // Next month same day
                chrono::NaiveDate::from_ymd_opt(
                    now.year(),
                    now.month() + 1,
                    now.day(),
                )
                .and_then(|d| d.and_hms_opt(now.hour(), now.minute(), 0))
                .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
                .unwrap_or_else(|| now + chrono::Duration::days(30))
            }
        };

        #[derive(sqlx::FromRow)]
        struct DigestRow {
            id: uuid::Uuid,
            tenant_id: uuid::Uuid,
            user_id: uuid::Uuid,
            digest_type: String,
            frequency: String,
            enabled: bool,
            filters: serde_json::Value,
            last_sent_at: Option<DateTime<Utc>>,
            next_send_at: DateTime<Utc>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let row = sqlx::query_as::<_, DigestRow>(
            r#"
            INSERT INTO report_digests (tenant_id, user_id, digest_type, frequency, enabled, filters, next_send_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (tenant_id, user_id, digest_type)
            DO UPDATE SET
                frequency = EXCLUDED.frequency,
                enabled = EXCLUDED.enabled,
                filters = EXCLUDED.filters,
                next_send_at = EXCLUDED.next_send_at,
                updated_at = NOW()
            RETURNING id, tenant_id, user_id, digest_type, frequency, enabled, filters,
                      last_sent_at, next_send_at, created_at, updated_at
            "#,
        )
        .bind(*_tenant_id.as_uuid())
        .bind(user_id)
        .bind(digest_type_str)
        .bind(match request.frequency {
            DigestFrequency::Daily => "daily",
            DigestFrequency::Weekly => "weekly",
            DigestFrequency::Monthly => "monthly",
        })
        .bind(request.enabled)
        .bind(&filters)
        .bind(next_send_at)
        .fetch_one(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to upsert digest: {}", e)))?;

        let digest_type = match row.digest_type.as_str() {
            "daily_summary" => DigestType::DailySummary,
            "weekly_summary" => DigestType::WeeklySummary,
            "monthly_summary" => DigestType::MonthlySummary,
            "approval_reminder" => DigestType::ApprovalReminder,
            _ => unreachable!(),
        };

        let frequency = match row.frequency.as_str() {
            "daily" => DigestFrequency::Daily,
            "weekly" => DigestFrequency::Weekly,
            "monthly" => DigestFrequency::Monthly,
            _ => unreachable!(),
        };

        Ok(ReportDigest {
            id: row.id,
            tenant_id: row.tenant_id.to_string(),
            user_id: row.user_id,
            digest_type,
            frequency,
            enabled: row.enabled,
            filters: row.filters,
            last_sent_at: row.last_sent_at,
            next_send_at: row.next_send_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// List digests for a user
    pub async fn list_user_digests(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        user_id: uuid::Uuid,
    ) -> Result<Vec<ReportDigest>> {
        #[derive(sqlx::FromRow)]
        struct DigestRow {
            id: uuid::Uuid,
            tenant_id: uuid::Uuid,
            user_id: uuid::Uuid,
            digest_type: String,
            frequency: String,
            enabled: bool,
            filters: serde_json::Value,
            last_sent_at: Option<DateTime<Utc>>,
            next_send_at: DateTime<Utc>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, DigestRow>(
            r#"
            SELECT id, tenant_id, user_id, digest_type, frequency, enabled, filters,
                   last_sent_at, next_send_at, created_at, updated_at
            FROM report_digests
            WHERE tenant_id = $1 AND user_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(*_tenant_id.as_uuid())
        .bind(user_id)
        .fetch_all(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to list digests: {}", e)))?;

        let digests = rows
            .into_iter()
            .filter_map(|row| {
                let digest_type = match row.digest_type.as_str() {
                    "daily_summary" => Some(DigestType::DailySummary),
                    "weekly_summary" => Some(DigestType::WeeklySummary),
                    "monthly_summary" => Some(DigestType::MonthlySummary),
                    "approval_reminder" => Some(DigestType::ApprovalReminder),
                    _ => None,
                }?;

                let frequency = match row.frequency.as_str() {
                    "daily" => Some(DigestFrequency::Daily),
                    "weekly" => Some(DigestFrequency::Weekly),
                    "monthly" => Some(DigestFrequency::Monthly),
                    _ => None,
                }?;

                Some(ReportDigest {
                    id: row.id,
                    tenant_id: row.tenant_id.to_string(),
                    user_id: row.user_id,
                    digest_type,
                    frequency,
                    enabled: row.enabled,
                    filters: row.filters,
                    last_sent_at: row.last_sent_at,
                    next_send_at: row.next_send_at,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                })
            })
            .collect();

        Ok(digests)
    }

    /// Delete a digest
    pub async fn delete_digest(
        &self,
        _tenant_id: &TenantId,
        pool: &Arc<PgPool>,
        user_id: uuid::Uuid,
        digest_id: uuid::Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM report_digests
            WHERE tenant_id = $1 AND user_id = $2 AND id = $3
            "#,
        )
        .bind(*_tenant_id.as_uuid())
        .bind(user_id)
        .bind(digest_id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| Error::Database(format!("Failed to delete digest: {}", e)))?;

        Ok(())
    }
}

impl Default for ReportingService {
    fn default() -> Self {
        Self::new()
    }
}
