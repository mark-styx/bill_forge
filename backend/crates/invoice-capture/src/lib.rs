//! Invoice Capture Module
//!
//! OCR-powered invoice data extraction with support for multiple providers.

pub mod ocr;
pub mod service;

pub use service::InvoiceCaptureService;
