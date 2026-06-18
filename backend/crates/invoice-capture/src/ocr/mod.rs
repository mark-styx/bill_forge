//! OCR providers
//!
//! Supports multiple OCR backends:
//! - **Tesseract**: Open-source, local OCR (default)
//! - **AWS Textract**: Cloud-based with table/form extraction
//! - **Google Cloud Vision**: Cloud-based with handwriting detection
//! - **Private Inference**: Customer-managed endpoint in tenant VPC (#334)

mod aws_textract;
mod google_vision;
pub mod ocr_comparison;
mod private_inference;
mod tesseract;

pub use self::aws_textract::AwsTextractOcr;
pub use self::google_vision::GoogleVisionOcr;
pub use self::ocr_comparison::{OcrComparison, OcrComparisonResult, OcrProvider, ProviderResult};
pub use self::private_inference::{
    check_health, load_for_tenant, mark_unhealthy, run_private_ocr, HealthStatus,
    PrivateInferenceConfig, PrivateInferenceError,
};
pub use self::tesseract::TesseractOcr;

use billforge_core::domain::OcrExtractionResult;
use billforge_core::traits::OcrService;
use billforge_core::types::TenantId;

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
            tracing::warn!(
                "Unknown OCR provider '{}', defaulting to Tesseract",
                provider_name
            );
            Box::new(TesseractOcr::new())
        }
    }
}

/// Get list of available OCR providers
pub fn available_providers() -> Vec<(&'static str, bool)> {
    vec![
        ("tesseract", tesseract::TesseractOcr::is_available_default()),
        (
            "aws_textract",
            aws_textract::AwsTextractOcr::is_configured(),
        ),
        (
            "google_vision",
            google_vision::GoogleVisionOcr::is_configured(),
        ),
    ]
}

/// Attempt private-inference OCR for a tenant.
///
/// Returns `Some(result)` when private inference was enabled, healthy, and
/// the endpoint returned a valid response.  Returns `None` when private
/// inference is not configured, is disabled, or the call failed (the
/// endpoint is marked unhealthy and an audit-level log is emitted).
pub async fn try_private_inference_ocr(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    document_bytes: &[u8],
) -> Option<OcrExtractionResult> {
    let cfg = match private_inference::load_for_tenant(pool, tenant_id).await {
        Some(c) => c,
        None => return None, // no row → not opted in
    };

    if !cfg.enabled {
        return None;
    }

    if cfg.health_status != private_inference::HealthStatus::Healthy {
        return None;
    }

    match private_inference::run_private_ocr(&cfg, document_bytes).await {
        Ok(result) => Some(result),
        Err(e) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                error = %e,
                "private_inference.fallback — private OCR failed, falling back to standard provider"
            );
            let _ = private_inference::mark_unhealthy(pool, tenant_id, &e.to_string()).await;
            None
        }
    }
}
