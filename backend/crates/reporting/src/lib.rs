//! Reporting Module
//!
//! Analytics and reporting for BillForge platform.

pub mod models;
pub mod service;

pub use models::{
    ActionableItem, AgingBucket, ApprovalAnalytics, ApproverWorkload, BottleneckStage,
    CategoryBreakdown, CustomReportQuery, CustomReportResult, DashboardSummary, DateRange,
    DigestContent, DigestFrequency, DigestHighlight, DigestSummary, DigestType, ExportFormat,
    ExportRequest, ExportResult, InvoiceVolume, ProcessingMetrics, ReportDigest, ReportFilter,
    SpendTrendPoint, StatusDistribution, UpsertDigestRequest, VendorPerformanceMetrics,
    VendorSpend, VendorSpendWithPeriods,
};
pub use service::ReportingService;
