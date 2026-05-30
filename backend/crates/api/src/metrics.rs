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
}
