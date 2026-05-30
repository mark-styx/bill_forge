//! Invoice Capture Module
//!
//! OCR-powered invoice data extraction with support for multiple providers.

pub mod calibration;
pub mod ocr;
pub mod service;

pub use calibration::{calibrated_confidence, OcrCalibrationStore, PgOcrCalibrationStore};
pub use service::{
    ocr_routing_decision, resolve_ocr_provider_name, InvoiceCaptureService, OcrRoutingDecision,
    LOCAL_OCR_PROVIDER, OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD,
    OCR_HARD_FAIL_CONFIDENCE_THRESHOLD,
};
