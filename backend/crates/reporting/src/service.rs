//! Reporting service with actual database queries
//!
//! Provides analytics and reporting capabilities for BillForge platform.

use crate::models::*;
use billforge_core::{types::TenantId, Result, Error};
use billforge_db::TenantDatabase;
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
        db: &TenantDatabase,
    ) -> Result<DashboardSummary> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Count invoices pending review (capture_status in pending, ready_for_review)
        let invoices_pending_review: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE capture_status IN ('pending', 'ready_for_review')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count invoices pending approval
        let invoices_pending_approval: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status = 'pending_approval'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count invoices ready for payment
        let invoices_ready_for_payment: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status IN ('approved', 'ready_for_payment')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count invoices processed today
        let invoices_processed_today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE date(created_at) = date('now') AND capture_status = 'reviewed'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Total pending amount (cents)
        let total_pending_cents: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(total_amount), 0) FROM invoices WHERE processing_status NOT IN ('paid', 'voided', 'rejected')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count active vendors
        let vendors_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vendors WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Calculate average processing time (hours) for paid invoices
        // Using time from created_at to when it was paid (approximated by updated_at for paid status)
        let avg_processing_time_hours: f64 = conn
            .query_row(
                r#"
                SELECT COALESCE(
                    AVG((julianday(updated_at) - julianday(created_at)) * 24),
                    0.0
                )
                FROM invoices
                WHERE processing_status = 'paid'
                AND created_at >= date('now', '-30 days')
                "#,
                [],
                |row| row.get(0),
            )
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
        db: &TenantDatabase,
        start_date: NaiveDate,
        end_date: NaiveDate,
        group_by: &str,
    ) -> Result<Vec<InvoiceVolume>> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Determine date format based on grouping
        let date_format = match group_by {
            "day" => "%Y-%m-%d",
            "week" => "%Y-W%W",
            "month" => "%Y-%m",
            "quarter" => "%Y-Q",
            "year" => "%Y",
            _ => "%Y-%m",
        };

        let sql = format!(
            r#"
            SELECT
                strftime('{}', invoice_date) as period,
                COUNT(*) as count,
                COALESCE(SUM(total_amount), 0) as total_amount
            FROM invoices
            WHERE invoice_date >= ? AND invoice_date <= ?
            GROUP BY period
            ORDER BY period ASC
            "#,
            date_format
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let rows = stmt
            .query_map(
                rusqlite::params![start_date.to_string(), end_date.to_string()],
                |row| {
                    Ok(InvoiceVolume {
                        period: row.get(0)?,
                        count: row.get::<_, i64>(1)? as u64,
                        total_amount: (row.get::<_, i64>(2)? as f64) / 100.0,
                    })
                },
            )
            .map_err(|e| Error::Database(format!("Failed to query invoice volume: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }

        Ok(results)
    }

    /// Get spend by vendor (top vendors by spend)
    pub async fn get_vendor_spend(
        &self,
        _tenant_id: &TenantId,
        db: &TenantDatabase,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        limit: u32,
    ) -> Result<Vec<VendorSpend>> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut sql = String::from(
            r#"
            SELECT
                i.vendor_id,
                COALESCE(v.name, i.vendor_name) as vendor_name,
                COUNT(*) as invoice_count,
                COALESCE(SUM(i.total_amount), 0) as total_spend,
                MAX(i.invoice_date) as last_invoice_date
            FROM invoices i
            LEFT JOIN vendors v ON i.vendor_id = v.id
            WHERE i.vendor_id IS NOT NULL
            "#,
        );

        let mut params: Vec<String> = Vec::new();

        if let Some(start) = start_date {
            sql.push_str(" AND i.invoice_date >= ?");
            params.push(start.to_string());
        }

        if let Some(end) = end_date {
            sql.push_str(" AND i.invoice_date <= ?");
            params.push(end.to_string());
        }

        sql.push_str(
            r#"
            GROUP BY i.vendor_id, vendor_name
            ORDER BY total_spend DESC
            LIMIT ?
            "#,
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        // Build params slice dynamically
        let results: Vec<VendorSpend> = match (start_date, end_date) {
            (Some(s), Some(e)) => {
                let rows = stmt.query_map(
                    rusqlite::params![s.to_string(), e.to_string(), limit],
                    Self::map_vendor_spend_row,
                ).map_err(|e| Error::Database(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            }
            (Some(s), None) => {
                let rows = stmt.query_map(
                    rusqlite::params![s.to_string(), limit],
                    Self::map_vendor_spend_row,
                ).map_err(|e| Error::Database(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            }
            (None, Some(e)) => {
                let rows = stmt.query_map(
                    rusqlite::params![e.to_string(), limit],
                    Self::map_vendor_spend_row,
                ).map_err(|e| Error::Database(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            }
            (None, None) => {
                let rows = stmt.query_map(
                    rusqlite::params![limit],
                    Self::map_vendor_spend_row,
                ).map_err(|e| Error::Database(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            }
        };

        Ok(results)
    }

    fn map_vendor_spend_row(row: &rusqlite::Row) -> rusqlite::Result<VendorSpend> {
        let vendor_id: Option<String> = row.get(0)?;
        let total_spend_cents: i64 = row.get(3)?;
        let invoice_count: i64 = row.get(2)?;
        let last_invoice_str: Option<String> = row.get(4)?;

        Ok(VendorSpend {
            vendor_id: vendor_id.unwrap_or_default(),
            vendor_name: row.get(1)?,
            invoice_count: invoice_count as u64,
            total_spend: (total_spend_cents as f64) / 100.0,
            avg_invoice_amount: if invoice_count > 0 {
                (total_spend_cents as f64) / (invoice_count as f64) / 100.0
            } else {
                0.0
            },
            last_invoice_date: last_invoice_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
        })
    }

    /// Get invoice aging report (buckets: 0-30, 31-60, 61-90, 90+ days)
    pub async fn get_invoice_aging(
        &self,
        _tenant_id: &TenantId,
        db: &TenantDatabase,
    ) -> Result<Vec<AgingBucket>> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Query invoices that are unpaid and group by age
        let sql = r#"
            SELECT
                CASE
                    WHEN julianday('now') - julianday(due_date) <= 0 THEN 'Current'
                    WHEN julianday('now') - julianday(due_date) <= 30 THEN '1-30 days'
                    WHEN julianday('now') - julianday(due_date) <= 60 THEN '31-60 days'
                    WHEN julianday('now') - julianday(due_date) <= 90 THEN '61-90 days'
                    ELSE '90+ days'
                END as bucket,
                CASE
                    WHEN julianday('now') - julianday(due_date) <= 0 THEN 0
                    WHEN julianday('now') - julianday(due_date) <= 30 THEN 1
                    WHEN julianday('now') - julianday(due_date) <= 60 THEN 31
                    WHEN julianday('now') - julianday(due_date) <= 90 THEN 61
                    ELSE 91
                END as days_min,
                CASE
                    WHEN julianday('now') - julianday(due_date) <= 0 THEN 0
                    WHEN julianday('now') - julianday(due_date) <= 30 THEN 30
                    WHEN julianday('now') - julianday(due_date) <= 60 THEN 60
                    WHEN julianday('now') - julianday(due_date) <= 90 THEN 90
                    ELSE NULL
                END as days_max,
                COUNT(*) as invoice_count,
                COALESCE(SUM(total_amount), 0) as total_amount
            FROM invoices
            WHERE processing_status NOT IN ('paid', 'voided', 'rejected')
            AND due_date IS NOT NULL
            GROUP BY bucket, days_min, days_max
            ORDER BY days_min ASC
        "#;

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let days_max: Option<i32> = row.get(2)?;
                Ok(AgingBucket {
                    bucket_name: row.get(0)?,
                    days_min: row.get(1)?,
                    days_max,
                    invoice_count: row.get::<_, i64>(3)? as u64,
                    total_amount: (row.get::<_, i64>(4)? as f64) / 100.0,
                })
            })
            .map_err(|e| Error::Database(format!("Failed to query aging: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }

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
        db: &TenantDatabase,
    ) -> Result<ProcessingMetrics> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Average capture time (from created to reviewed)
        let avg_capture_time_minutes: f64 = conn
            .query_row(
                r#"
                SELECT COALESCE(
                    AVG((julianday(updated_at) - julianday(created_at)) * 24 * 60),
                    0.0
                )
                FROM invoices
                WHERE capture_status = 'reviewed'
                AND created_at >= date('now', '-30 days')
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // Average approval time (from submitted to approved)
        let avg_approval_time_hours: f64 = conn
            .query_row(
                r#"
                SELECT COALESCE(
                    AVG((julianday(updated_at) - julianday(created_at)) * 24),
                    0.0
                )
                FROM invoices
                WHERE processing_status IN ('approved', 'ready_for_payment', 'paid')
                AND created_at >= date('now', '-30 days')
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // Average total processing time (from created to paid)
        let avg_total_processing_time_hours: f64 = conn
            .query_row(
                r#"
                SELECT COALESCE(
                    AVG((julianday(updated_at) - julianday(created_at)) * 24),
                    0.0
                )
                FROM invoices
                WHERE processing_status = 'paid'
                AND created_at >= date('now', '-30 days')
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // Count total and auto-approved for rate calculation
        let total_approved: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status IN ('approved', 'ready_for_payment', 'paid') AND created_at >= date('now', '-30 days')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_rejected: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status = 'rejected' AND created_at >= date('now', '-30 days')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_processed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status NOT IN ('draft', 'submitted') AND created_at >= date('now', '-30 days')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1); // Avoid division by zero

        // First pass rate (invoices that didn't go on hold or get rejected first)
        let first_pass_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status = 'paid' AND notes NOT LIKE '%hold%' AND created_at >= date('now', '-30 days')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let paid_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM invoices WHERE processing_status = 'paid' AND created_at >= date('now', '-30 days')",
                [],
                |row| row.get(0),
            )
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
        db: &TenantDatabase,
    ) -> Result<Vec<StatusDistribution>> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let sql = r#"
            SELECT
                processing_status as status,
                COUNT(*) as count,
                COALESCE(SUM(total_amount), 0) as total_amount
            FROM invoices
            GROUP BY processing_status
            ORDER BY count DESC
        "#;

        let total_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM invoices", [], |row| row.get(0))
            .unwrap_or(1);

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let count: i64 = row.get(1)?;
                Ok(StatusDistribution {
                    status: row.get(0)?,
                    count: count as u64,
                    percentage: (count as f64) / (total_count as f64) * 100.0,
                    total_amount: (row.get::<_, i64>(2)? as f64) / 100.0,
                })
            })
            .map_err(|e| Error::Database(format!("Failed to query status distribution: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }

        Ok(results)
    }

    /// Execute a custom report query
    pub async fn execute_custom_report(
        &self,
        _tenant_id: &TenantId,
        db: &TenantDatabase,
        query: CustomReportQuery,
    ) -> Result<CustomReportResult> {
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Build query based on report type
        let sql = match query.report_type.as_str() {
            "invoices_by_department" => {
                r#"
                SELECT
                    COALESCE(department, 'Unassigned') as department,
                    COUNT(*) as invoice_count,
                    SUM(total_amount) / 100.0 as total_amount
                FROM invoices
                WHERE invoice_date >= ? AND invoice_date <= ?
                GROUP BY department
                ORDER BY total_amount DESC
                "#.to_string()
            }
            "invoices_by_gl_code" => {
                r#"
                SELECT
                    COALESCE(gl_code, 'Unassigned') as gl_code,
                    COUNT(*) as invoice_count,
                    SUM(total_amount) / 100.0 as total_amount
                FROM invoices
                WHERE invoice_date >= ? AND invoice_date <= ?
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
                WHERE created_at >= ? AND created_at <= ?
                GROUP BY status
                "#.to_string()
            }
            _ => {
                return Err(Error::Validation(format!("Unknown report type: {}", query.report_type)));
            }
        };

        // Get date range
        let (start_date, end_date) = if let Some(ref range) = query.date_range {
            (range.start.to_string(), range.end.to_string())
        } else {
            // Default to last 30 days
            let end = Utc::now().naive_utc().date();
            let start = end - Duration::days(30);
            (start.to_string(), end.to_string())
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        // Get column names from statement
        let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let rows_result = stmt
            .query_map(rusqlite::params![start_date, end_date], |row| {
                let mut values = Vec::new();
                for i in 0..columns.len() {
                    // Try different types
                    if let Ok(v) = row.get::<_, String>(i) {
                        values.push(serde_json::Value::String(v));
                    } else if let Ok(v) = row.get::<_, i64>(i) {
                        values.push(serde_json::Value::Number(v.into()));
                    } else if let Ok(v) = row.get::<_, f64>(i) {
                        values.push(serde_json::json!(v));
                    } else {
                        values.push(serde_json::Value::Null);
                    }
                }
                Ok(values)
            })
            .map_err(|e| Error::Database(format!("Failed to execute custom report: {}", e)))?;

        let mut rows = Vec::new();
        for row in rows_result {
            rows.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }

        let total_rows = rows.len() as u64;

        Ok(CustomReportResult {
            columns,
            rows,
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
