//! OCR providers
//!
//! Supports multiple OCR backends:
//! - **Tesseract**: In-process OCR via the local `tesseract` CLI. Default
//!   provider. Document bytes never leave the BillForge process and this is
//!   the only path that satisfies a tenant's `local_ocr_required` policy.
//! - **AWS Textract**: Cloud-based with table/form extraction.
//! - **Google Cloud Vision**: Cloud-based with handwriting detection.
//! - **Private Inference**: Customer-hosted *remote* OCR endpoint reached via
//!   outbound HTTP POST (#334). Despite the name this is **not** in-process
//!   inference and does **not** satisfy `local_ocr_required` — see
//!   [`private_inference`].

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
use billforge_core::{Error, Result};

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

/// Attempt customer-hosted private-inference OCR for a tenant.
///
/// Note: despite the historical name, this dispatches to a customer-supplied
/// HTTP endpoint, not in-process inference. When a tenant has
/// `local_ocr_required == true` this call short-circuits to `None` *before*
/// any DB or network I/O so document bytes are not sent off-process.
///
/// Returns `Some(result)` when private inference was enabled, healthy, and
/// the endpoint returned a valid response.  Returns `None` when private
/// inference is not configured, is disabled, blocked by `local_ocr_required`,
/// or the call failed (the endpoint is marked unhealthy and an audit-level
/// log is emitted).
pub async fn try_private_inference_ocr(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    document_bytes: &[u8],
    local_ocr_required: bool,
) -> Option<OcrExtractionResult> {
    if local_ocr_required {
        tracing::debug!(
            tenant_id = %tenant_id,
            "private_inference.skip — local_ocr_required policy is on; not attempting customer-hosted remote OCR"
        );
        return None;
    }

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

/// Select the in-process OCR provider for a tenant that has the
/// `local_ocr_required` policy enabled.
///
/// Returns a `TesseractOcr` instance when the host has a working tesseract
/// binary, or a hard error otherwise. This refuses to fall back to any
/// cloud/HTTP provider so a misconfigured host fails the OCR job rather
/// than silently leaking document bytes off-process.
pub fn select_local_only_provider() -> Result<Box<dyn OcrService>> {
    select_local_only_provider_inner(TesseractOcr::is_available_default())
}

/// Inner implementation that takes the availability check as input so it
/// can be exercised in unit tests without depending on the host environment.
pub(crate) fn select_local_only_provider_inner(
    tesseract_available: bool,
) -> Result<Box<dyn OcrService>> {
    if !tesseract_available {
        return Err(Error::Ocr(
            "local_ocr_required is set but the in-process Tesseract OCR binary is not \
             available on this host; refusing to fall back to a cloud or remote provider"
                .to_string(),
        ));
    }
    Ok(Box::new(TesseractOcr::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::traits::OcrService;

    #[tokio::test]
    async fn try_private_inference_short_circuits_when_local_ocr_required() {
        // Lazy pool — never actually connects. If the guard works, we return
        // `None` before any DB query is issued, so the URL is never dialed.
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/postgres")
            .expect("connect_lazy should accept a syntactically valid URL");
        let tenant_id = TenantId::new();

        let result = try_private_inference_ocr(
            &pool,
            &tenant_id,
            b"document-bytes",
            /* local_ocr_required = */ true,
        )
        .await;

        assert!(
            result.is_none(),
            "local_ocr_required must skip the remote private-inference path"
        );
    }

    #[test]
    fn select_local_only_provider_returns_tesseract_when_available() {
        let provider = match select_local_only_provider_inner(true) {
            Ok(p) => p,
            Err(e) => panic!("should select tesseract when available: {e:?}"),
        };
        assert_eq!(provider.provider_name(), "tesseract");
    }

    #[test]
    fn select_local_only_provider_errors_when_tesseract_unavailable() {
        let err = match select_local_only_provider_inner(false) {
            Ok(_) => panic!("must error rather than silently fall back"),
            Err(e) => e,
        };
        match err {
            Error::Ocr(msg) => {
                assert!(
                    msg.contains("Tesseract"),
                    "error must explain the missing in-process provider, got: {msg}"
                );
            }
            other => panic!("expected Error::Ocr, got {other:?}"),
        }
    }
}
