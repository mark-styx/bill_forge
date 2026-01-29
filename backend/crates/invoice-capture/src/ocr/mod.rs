//! OCR providers

mod tesseract;
// mod textract; // AWS Textract implementation
// mod google_vision; // Google Cloud Vision implementation

pub use self::tesseract::TesseractOcr;

use async_trait::async_trait;
use billforge_core::{domain::OcrExtractionResult, traits::OcrService, Result};

/// OCR provider factory
pub fn create_provider(provider_name: &str) -> Box<dyn OcrService> {
    match provider_name {
        "tesseract" => Box::new(TesseractOcr::new()),
        // "aws_textract" => Box::new(TextractOcr::new()),
        // "google_vision" => Box::new(GoogleVisionOcr::new()),
        _ => Box::new(TesseractOcr::new()),
    }
}
