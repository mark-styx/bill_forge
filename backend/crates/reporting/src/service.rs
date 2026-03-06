//! Reporting service with actual database queries
//!
//! Provides analytics and reporting capabilities for BillForge platform.

use crate::models::*;
use billforge_core::{types::TenantId, Result, Error};
use sqlx::{PgPool, Row, Column};
use std::sync::Arc;
use chrono::{NaiveDate, Utc, Duration};

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
}

impl Default for ReportingService {
    fn default() -> Self {
        Self::new()
    }
}
