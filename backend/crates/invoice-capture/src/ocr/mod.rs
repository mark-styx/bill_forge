//! OCR providers
//!
//! Supports multiple OCR backends:
//! - **Tesseract**: Open-source, local OCR (default)
//! - **AWS Textract**: Cloud-based with table/form extraction
//! - **Google Cloud Vision**: Cloud-based with handwriting detection

mod tesseract;
mod aws_textract;
mod google_vision;
pub mod ocr_comparison;

pub use self::tesseract::TesseractOcr;
pub use self::aws_textract::AwsTextractOcr;
pub use self::google_vision::GoogleVisionOcr;
pub use self::ocr_comparison::{OcrComparison, OcrProvider};

use billforge_core::traits::OcrService;

/// OCR provider factory
///
/// Creates an OCR provider instance based on the provider name.
/// Falls back to Tesseract if provider is not available.
pub fn create_provider(provider_name: &str) -> Box<dyn OcrService> {
    match provider_name {
        "tesseract" => Box::new(TesseractOcr::new()),
        "aws_textract" | "textract" => {
            if aws_textract::AwsTextractOcr::is_configured() {
                Box::new(AwsTextractOcr::new())
            } else {
                tracing::warn!("AWS Textract not configured, falling back to Tesseract");
                Box::new(TesseractOcr::new())
            }
        }
        "google_vision" | "google" => {
            if google_vision::GoogleVisionOcr::is_configured() {
                Box::new(GoogleVisionOcr::new())
            } else {
                tracing::warn!("Google Cloud Vision not configured, falling back to Tesseract");
                Box::new(TesseractOcr::new())
            }
        }
        _ => {
            tracing::warn!("Unknown OCR provider '{}', defaulting to Tesseract", provider_name);
            Box::new(TesseractOcr::new())
        }
    }
}

/// Get list of available OCR providers
pub fn available_providers() -> Vec<(&'static str, bool)> {
    vec![
        ("tesseract", tesseract::TesseractOcr::is_available()),
        ("aws_textract", aws_textract::AwsTextractOcr::is_configured()),
        ("google_vision", google_vision::GoogleVisionOcr::is_configured()),
    ]
}
