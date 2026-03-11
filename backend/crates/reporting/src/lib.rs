//! Reporting Module
//!
//! Analytics and reporting for BillForge platform.

pub mod service;
pub mod models;

pub use service::ReportingService;
pub use models::{
    DashboardSummary, InvoiceVolume, VendorSpend, VendorSpendWithPeriods, AgingBucket,
    ProcessingMetrics, StatusDistribution, CustomReportQuery, CustomReportResult, DateRange,
    ReportFilter, SpendTrendPoint, CategoryBreakdown, VendorPerformanceMetrics, ApprovalAnalytics,
    BottleneckStage, ApproverWorkload, ExportRequest, ExportFormat, ExportResult,
    ReportDigest, DigestType, DigestFrequency, UpsertDigestRequest, DigestContent,
    DigestSummary, DigestHighlight, ActionableItem,
};
