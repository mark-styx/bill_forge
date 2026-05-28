//! Invoice Capture Module
//!
//! OCR-powered invoice data extraction with support for multiple providers.

pub mod ocr;
pub mod service;

pub use service::{
    resolve_ocr_provider_name, InvoiceCaptureService, LOCAL_OCR_PROVIDER,
    OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD,
};
