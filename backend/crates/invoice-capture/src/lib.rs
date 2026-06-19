//! Invoice Capture Module
//!
//! OCR-powered invoice data extraction with support for multiple providers.

pub mod calibration;
pub mod confidence;
pub mod ocr;
pub mod service;

pub use calibration::{
    bucket_for, calibrated_confidence, compute_first_pass_accuracy, tenant_first_pass_accuracy,
    BucketCalibration, FieldAccuracy, FirstPassAccuracy, OcrCalibrationStore,
    PgOcrCalibrationStore, MIN_EXTRACTIONS_FOR_ACCURACY, OCR_FIRST_PASS_ACCURACY_THRESHOLD,
};
pub use confidence::compute_overall_confidence;
pub use ocr::{
    check_health, load_for_tenant, mark_unhealthy, run_private_ocr, try_private_inference_ocr,
    HealthStatus, OcrComparison, OcrComparisonResult, OcrProvider, PrivateInferenceConfig,
    PrivateInferenceError, ProviderResult,
};
pub use service::{
    ocr_routing_decision, resolve_ocr_provider_name, InvoiceCaptureService, OcrRoutingDecision,
    LOCAL_OCR_PROVIDER, OCR_CALIBRATED_FIELDS, OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD,
    OCR_HARD_FAIL_CONFIDENCE_THRESHOLD,
};
