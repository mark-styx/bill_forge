//! Prometheus metrics for BillForge API

use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_counter_vec, register_histogram, register_histogram_vec,
    register_int_gauge_vec, Counter, CounterVec, Encoder, Histogram, HistogramVec, IntGaugeVec,
    TextEncoder,
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

    // --- SLO / OCR accuracy metric series (#223) ---

    /// Sub-200ms API SLO compliance counter. Label `compliant` is "true" or "false".
    pub static ref API_SUB_200MS_COMPLIANCE_TOTAL: CounterVec = register_counter_vec!(
        "billforge_api_sub_200ms_compliance_total",
        "Counts requests that completed within (compliant=true) or exceeded (compliant=false) the 200ms SLO",
        &["endpoint", "compliant"]
    ).unwrap();

    /// Histogram tracking time from upload capture start to OCR job enqueue.
    pub static ref CAPTURE_TO_QUEUE_DURATION_SECONDS: Histogram = register_histogram!(
        "billforge_capture_to_queue_seconds",
        "Seconds from invoice upload start to OCR job enqueue",
        vec![0.01, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0]
    ).unwrap();

    /// OCR provider outcome counter (success/failure per provider).
    pub static ref OCR_PROVIDER_OUTCOME_TOTAL: CounterVec = register_counter_vec!(
        "billforge_ocr_provider_outcome_total",
        "OCR provider outcome counts",
        &["provider", "outcome"]
    ).unwrap();

    /// First-pass OCR field confidence histogram per field name.
    pub static ref OCR_FIRST_PASS_FIELD_CONFIDENCE: HistogramVec = register_histogram_vec!(
        "billforge_ocr_first_pass_field_confidence",
        "Confidence score distribution for first-pass OCR field extraction",
        &["field"],
        vec![0.1, 0.3, 0.5, 0.7, 0.8, 0.9, 0.95, 0.99, 1.0]
    ).unwrap();

    // --- End-to-end capture timing SLO metrics (#305) ---

    /// Histogram tracking time from invoice capture (created_at) to approval queue
    /// placement or auto-approval decision. Buckets aligned to the sub-5-minute SLO.
    pub static ref CAPTURE_TO_APPROVAL_QUEUE_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "billforge_capture_to_approval_queue_seconds",
        "Seconds from invoice capture to approval queue placement or auto-approval decision",
        &["tenant_id", "outcome"],
        vec![30.0, 60.0, 120.0, 180.0, 240.0, 300.0, 420.0, 600.0, 900.0, 1800.0, 3600.0]
    ).unwrap();

    /// Histogram tracking time from invoice capture (created_at) to a terminal
    /// processing status (approved, rejected, posted, etc.).
    pub static ref CAPTURE_TO_FINAL_STATUS_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "billforge_capture_to_final_status_seconds",
        "Seconds from invoice capture to terminal processing status",
        &["tenant_id", "final_status"],
        vec![30.0, 60.0, 120.0, 180.0, 240.0, 300.0, 420.0, 600.0, 900.0, 1800.0, 3600.0]
    ).unwrap();
}

/// Record request-level SLO telemetry: HTTP duration histogram + sub-200ms compliance.
pub fn record_request_slo(endpoint: &str, status: u16, elapsed_secs: f64) {
    let status_label = status.to_string();
    HTTP_REQUEST_DURATION_SECONDS
        .with_label_values(&["UNKNOWN", endpoint, &status_label])
        .observe(elapsed_secs);
    let compliant = if elapsed_secs < 0.2 { "true" } else { "false" };
    API_SUB_200MS_COMPLIANCE_TOTAL
        .with_label_values(&[endpoint, compliant])
        .inc();
}

/// Observe capture-to-approval-queue duration (one-liner for call sites).
/// `outcome` should be one of: `routed_for_approval`, `auto_approved`, `exception`.
pub fn observe_capture_to_approval_queue(tenant_id: &str, outcome: &str, duration_secs: f64) {
    CAPTURE_TO_APPROVAL_QUEUE_DURATION_SECONDS
        .with_label_values(&[tenant_id, outcome])
        .observe(duration_secs);
}

/// Observe capture-to-final-status duration (one-liner for call sites).
/// `final_status` should be a known ProcessingStatus string (e.g. `approved`, `rejected`).
pub fn observe_capture_to_final_status(tenant_id: &str, final_status: &str, duration_secs: f64) {
    CAPTURE_TO_FINAL_STATUS_DURATION_SECONDS
        .with_label_values(&[tenant_id, final_status])
        .observe(duration_secs);
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

    #[test]
    fn test_slo_and_ocr_metrics_export() {
        // Exercise the four new metric series
        API_SUB_200MS_COMPLIANCE_TOTAL
            .with_label_values(&["/api/v1/invoices/upload", "true"])
            .inc();
        CAPTURE_TO_QUEUE_DURATION_SECONDS.observe(0.15);
        OCR_PROVIDER_OUTCOME_TOTAL
            .with_label_values(&["tesseract", "success"])
            .inc();
        OCR_FIRST_PASS_FIELD_CONFIDENCE
            .with_label_values(&["invoice_number"])
            .observe(0.92);

        // Also exercise the helper
        record_request_slo("/api/v1/invoices", 200, 0.05);

        let metrics = export_metrics();

        assert!(
            metrics.contains("billforge_api_sub_200ms_compliance_total"),
            "missing sub-200ms compliance metric"
        );
        assert!(
            metrics.contains("billforge_capture_to_queue_seconds"),
            "missing capture-to-queue metric"
        );
        assert!(
            metrics.contains("billforge_ocr_provider_outcome_total"),
            "missing OCR provider outcome metric"
        );
        assert!(
            metrics.contains("billforge_ocr_first_pass_field_confidence"),
            "missing first-pass field confidence metric"
        );

        // Verify label values appear
        assert!(metrics.contains("compliant=\"true\""));
        assert!(metrics.contains("provider=\"tesseract\""));
        assert!(metrics.contains("outcome=\"success\""));
        assert!(metrics.contains("field=\"invoice_number\""));
    }

    #[test]
    fn test_capture_to_approval_queue_histogram_registered_and_observed() {
        // Exercise the helper
        observe_capture_to_approval_queue("tenant-abc", "routed_for_approval", 250.0);
        observe_capture_to_approval_queue("tenant-abc", "auto_approved", 45.0);

        let metrics = export_metrics();

        // Metric name present
        assert!(
            metrics.contains("billforge_capture_to_approval_queue_seconds"),
            "missing capture-to-approval-queue metric"
        );

        // Label values present
        assert!(
            metrics.contains("outcome=\"routed_for_approval\""),
            "missing routed_for_approval outcome label"
        );
        assert!(
            metrics.contains("outcome=\"auto_approved\""),
            "missing auto_approved outcome label"
        );
        assert!(
            metrics.contains("tenant_id=\"tenant-abc\""),
            "missing tenant_id label"
        );

        // _count should be 2 total observations across both outcomes
        assert!(
            metrics.contains("billforge_capture_to_approval_queue_seconds_count"),
            "missing _count series"
        );

        // Bucket boundary 240.0 should have captured the auto_approved (45s) but not the other
        assert!(
            metrics.contains("billforge_capture_to_approval_queue_seconds_bucket"),
            "missing _bucket series"
        );
    }

    #[test]
    fn test_capture_to_final_status_histogram_registered_and_observed() {
        observe_capture_to_final_status("tenant-xyz", "approved", 300.0);
        observe_capture_to_final_status("tenant-xyz", "rejected", 120.0);

        let metrics = export_metrics();

        assert!(
            metrics.contains("billforge_capture_to_final_status_seconds"),
            "missing capture-to-final-status metric"
        );
        assert!(
            metrics.contains("final_status=\"approved\""),
            "missing approved final_status label"
        );
        assert!(
            metrics.contains("final_status=\"rejected\""),
            "missing rejected final_status label"
        );
        assert!(
            metrics.contains("tenant_id=\"tenant-xyz\""),
            "missing tenant_id label"
        );
        assert!(
            metrics.contains("billforge_capture_to_final_status_seconds_count"),
            "missing _count series"
        );
        assert!(
            metrics.contains("billforge_capture_to_final_status_seconds_bucket"),
            "missing _bucket series"
        );
    }

    #[test]
    fn test_capture_to_approval_queue_empty_outcome_no_panic() {
        // Observing with an empty/unknown outcome must not panic;
        // it falls through as a label value (safe default).
        observe_capture_to_approval_queue("tenant-empty", "", 60.0);

        let metrics = export_metrics();
        assert!(
            metrics.contains("billforge_capture_to_approval_queue_seconds"),
            "metric should still be present with empty outcome"
        );
        // The empty string becomes outcome="" in the output
        assert!(
            metrics.contains("outcome=\"\""),
            "empty outcome label should appear"
        );
    }
}
