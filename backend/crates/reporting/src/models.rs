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

/// Spend by vendor with YTD/MTD breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorSpendWithPeriods {
    pub vendor_id: String,
    pub vendor_name: String,
    pub ytd_spend: f64,
    pub mtd_spend: f64,
    pub invoice_count: u64,
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

// === Advanced Reporting Models ===

/// Spend trend data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendTrendPoint {
    pub period: String,
    pub total_spend: f64,
    pub invoice_count: u64,
    pub avg_invoice_amount: f64,
    pub change_from_prior_period: Option<f64>,
    pub change_percentage: Option<f64>,
}

/// Category breakdown (GL code, department, cost center)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryBreakdown {
    pub category_type: String, // "gl_code", "department", "cost_center"
    pub category_value: String,
    pub invoice_count: u64,
    pub total_amount: f64,
    pub percentage_of_total: f64,
    pub avg_amount: f64,
}

/// Vendor performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorPerformanceMetrics {
    pub vendor_id: String,
    pub vendor_name: String,
    pub total_invoices: u64,
    pub total_spend: f64,
    pub on_time_payment_rate: f64,
    pub avg_payment_days: f64,
    pub dispute_rate: f64,
    pub credit_utilization: f64,
    pub reliability_score: f64, // 0-100 score
}

/// Approval analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalAnalytics {
    pub total_approvals: u64,
    pub avg_approval_time_hours: f64,
    pub approval_rate: f64,
    pub rejection_rate: f64,
    pub bottleneck_stages: Vec<BottleneckStage>,
    pub approver_workloads: Vec<ApproverWorkload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckStage {
    pub stage_name: String,
    pub avg_time_hours: f64,
    pub invoice_count: u64,
    pub percentage_of_total_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproverWorkload {
    pub approver_id: String,
    pub approver_name: String,
    pub approvals_completed: u64,
    pub avg_time_to_approve_hours: f64,
    pub pending_approvals: u64,
    pub approval_rate: f64,
}

/// Export request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub report_type: String,
    pub format: ExportFormat,
    pub date_range: Option<DateRange>,
    pub filters: Vec<ReportFilter>,
    pub include_headers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Excel,
    Json,
}

/// Export result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub download_url: String,
    pub filename: String,
    pub expires_at: DateTime<Utc>,
    pub row_count: u64,
}

// === Email Digest Models ===

/// Report digest configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDigest {
    pub id: uuid::Uuid,
    pub tenant_id: String,
    pub user_id: uuid::Uuid,
    pub digest_type: DigestType,
    pub frequency: DigestFrequency,
    pub enabled: bool,
    pub filters: serde_json::Value,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub next_send_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DigestType {
    DailySummary,
    WeeklySummary,
    MonthlySummary,
    ApprovalReminder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DigestFrequency {
    Daily,
    Weekly,
    Monthly,
}

/// Create/update digest request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertDigestRequest {
    pub digest_type: DigestType,
    pub frequency: DigestFrequency,
    pub enabled: bool,
    pub filters: Option<serde_json::Value>,
}

/// Digest content for email generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestContent {
    pub digest_type: DigestType,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub summary: DigestSummary,
    pub highlights: Vec<DigestHighlight>,
    pub actionable_items: Vec<ActionableItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestSummary {
    pub total_invoices: u64,
    pub total_amount: f64,
    pub pending_approvals: u64,
    pub approved_count: u64,
    pub rejected_count: u64,
    pub avg_processing_time_hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestHighlight {
    pub title: String,
    pub description: String,
    pub value: Option<f64>,
    pub change_percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableItem {
    pub item_type: String,
    pub item_id: String,
    pub title: String,
    pub description: String,
    pub url: String,
    pub priority: String,
}
