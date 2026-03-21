//! OCR Pipeline Module
//!
//! Orchestrates OCR processing for invoice documents:
//! - Job queue management (create, process, retry, cancel)
//! - OCR extraction via pluggable providers (Tesseract, Textract, Vision)
//! - Vendor matching (exact → alias → fuzzy)
//! - Batch upload processing
//! - Correction learning for continuous improvement

pub mod batch;
pub mod error;
pub mod pipeline;
pub mod types;
pub mod vendor_matcher;

pub use batch::BatchProcessor;
pub use error::PipelineError;
pub use pipeline::OcrPipeline;
pub use types::*;
pub use vendor_matcher::VendorMatcher;
