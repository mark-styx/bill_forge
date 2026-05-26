//! Prometheus metrics for BillForge API

use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_histogram_vec, register_int_gauge_vec, Counter, Encoder,
    HistogramVec, IntGaugeVec, TextEncoder,
};

lazy_static! {
    // HTTP request counter
    pub static ref HTTP_REQUESTS_TOTAL: Counter = register_counter!(
        "billforge_http_requests_total",
        "Total number of HTTP requests"
    ).unwrap();

    // HTTP request duration histogram
    pub static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "billforge_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "endpoint", "status"]
    ).unwrap();

    // Active connections gauge
    pub static ref ACTIVE_CONNECTIONS: IntGaugeVec = register_int_gauge_vec!(
        "billforge_active_connections",
        "Number of active connections",
        &["tenant_id"]
    ).unwrap();

    // Invoices processed counter
    pub static ref INVOICES_PROCESSED_TOTAL: Counter = register_counter!(
        "billforge_invoices_processed_total",
        "Total number of invoices processed"
    ).unwrap();

    // QuickBooks sync counter
    pub static ref QUICKBOOKS_SYNC_TOTAL: Counter = register_counter!(
        "billforge_quickbooks_sync_total",
        "Total number of QuickBooks sync operations"
    ).unwrap();

    // Database query duration histogram
    pub static ref DB_QUERY_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "billforge_db_query_duration_seconds",
        "Database query duration in seconds",
        &["query_type"]
    ).unwrap();
}

/// Export metrics in Prometheus text format
pub fn export_metrics() -> String {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    // Gather all metrics
    let metric_families = prometheus::gather();

    // Encode to text format
    encoder.encode(&metric_families, &mut buffer).unwrap();

    String::from_utf8(buffer).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_export() {
        // Increment a counter
        HTTP_REQUESTS_TOTAL.inc();

        // Export metrics
        let metrics = export_metrics();

        // Verify metrics contain our counter
        assert!(metrics.contains("billforge_http_requests_total"));
    }
}
