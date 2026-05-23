use billforge_worker::jobs::{Job, JobType};
use chrono::Utc;
use serde_json::json;

#[test]
fn job_type_wire_names_are_stable() {
    let cases = [
        (JobType::QuickBooksVendorSync, "quick_books_vendor_sync"),
        (JobType::QuickBooksAccountSync, "quick_books_account_sync"),
        (
            JobType::QuickBooksInvoiceExport,
            "quick_books_invoice_export",
        ),
        (JobType::MetricsAggregation, "metrics_aggregation"),
        (JobType::EmailBatch, "email_batch"),
        (JobType::ReportDigest, "report_digest"),
        (JobType::EmbeddingRefresh, "embedding_refresh"),
        (JobType::CategorizationTraining, "categorization_training"),
        (JobType::RoutingOptimization, "routing_optimization"),
        (JobType::ForecastRefresh, "forecast_refresh"),
        (JobType::AnomalyDetection, "anomaly_detection"),
        (JobType::EdiProcessInbound, "edi_process_inbound"),
        (JobType::EdiSendRemittance, "edi_send_remittance"),
        (JobType::EdiSendAck, "edi_send_ack"),
        (JobType::EdiCheckAckStatus, "edi_check_ack_status"),
        (JobType::OcrProcess, "ocr_process"),
        (JobType::ApprovalExpiry, "approval_expiry"),
    ];

    for (job_type, expected) in cases {
        let serialized = serde_json::to_value(job_type).unwrap();
        assert_eq!(serialized, json!(expected));
    }
}

#[test]
fn job_payload_contract_round_trips() {
    let job = Job {
        id: "job-123".to_string(),
        job_type: JobType::RoutingOptimization,
        tenant_id: "tenant-456".to_string(),
        payload: json!({ "priority": "normal", "attempt": 1 }),
        created_at: Utc::now(),
        retry_count: 2,
    };

    let serialized = serde_json::to_string(&job).unwrap();
    let deserialized: Job = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.id, job.id);
    assert_eq!(deserialized.tenant_id, job.tenant_id);
    assert_eq!(deserialized.payload, job.payload);
    assert_eq!(deserialized.retry_count, job.retry_count);
    assert!(matches!(
        deserialized.job_type,
        JobType::RoutingOptimization
    ));
}
