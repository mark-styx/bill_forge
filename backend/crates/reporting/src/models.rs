//! Reporting data models

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Dashboard summary metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub invoices_pending_review: u64,
    pub invoices_pending_approval: u64,
    pub invoices_ready_for_payment: u64,
    pub invoices_processed_today: u64,
    pub total_pending_amount: f64,
    pub vendors_active: u64,
    pub avg_processing_time_hours: f64,
}

/// Invoice volume by period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceVolume {
    pub period: String,
    pub count: u64,
    pub total_amount: f64,
}

/// Spend by vendor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorSpend {
    pub vendor_id: String,
    pub vendor_name: String,
    pub invoice_count: u64,
    pub total_spend: f64,
    pub avg_invoice_amount: f64,
    pub last_invoice_date: Option<NaiveDate>,
}

/// Invoice aging bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgingBucket {
    pub bucket_name: String,
    pub days_min: i32,
    pub days_max: Option<i32>,
    pub invoice_count: u64,
    pub total_amount: f64,
}

/// Processing metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMetrics {
    pub avg_capture_time_minutes: f64,
    pub avg_approval_time_hours: f64,
    pub avg_total_processing_time_hours: f64,
    pub auto_approval_rate: f64,
    pub rejection_rate: f64,
    pub first_pass_rate: f64,
}

/// Invoice status distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDistribution {
    pub status: String,
    pub count: u64,
    pub percentage: f64,
    pub total_amount: f64,
}

/// Custom report query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomReportQuery {
    pub report_type: String,
    pub date_range: Option<DateRange>,
    pub filters: Vec<ReportFilter>,
    pub group_by: Option<String>,
    pub order_by: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFilter {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

/// Custom report result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomReportResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total_rows: u64,
    pub generated_at: DateTime<Utc>,
}
