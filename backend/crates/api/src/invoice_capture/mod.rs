//! Invoice Capture endpoint - multi-provider OCR pipeline with confidence scoring.
//!
//! Accepts a single PDF/image upload, runs OCR (Tesseract local or AWS Textract stub),
//! extracts line items with per-line confidence, and persists everything tenant-scoped.
//!
//! # System dependencies
//!
//! - `tesseract` (>= 4.0) - open-source OCR engine
//! - `poppler` (provides `pdftoppm`) - optional, for better PDF-to-image conversion
//!
//! macOS:  `brew install tesseract poppler`
//! Ubuntu: `apt-get install tesseract-ocr poppler-utils`

use crate::error::ApiResult;
use crate::extractors::InvoiceCaptureAccess;
use crate::state::AppState;
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use billforge_invoice_capture::{compute_overall_confidence, ocr, resolve_ocr_provider_name};
use serde::Serialize;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum accepted upload size in bytes (10 MB).
const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// MIME types accepted for upload.
const ACCEPTED_MIME_TYPES: &[&str] = &["application/pdf", "image/png", "image/jpeg"];

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new().route("/", post(upload_capture))
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CaptureResponse {
    pub capture_id: String,
    /// ID of the `invoices` row created by bridging this capture into the
    /// unified upload pipeline (#373). Callers can poll
    /// `GET /invoices/{invoice_id}` for OCR/routing status exactly as they
    /// would for a `/invoices/upload` upload. Identical to `capture_id` when
    /// the bridge is unavailable (single-pillar builds without
    /// `processing`/`analytics`/`billing`).
    pub invoice_id: String,
    pub provider: String,
    pub overall_confidence: f32,
    pub privacy_mode: String,
    pub line_items: Vec<LineItemResponse>,
}

#[derive(Debug, Serialize)]
pub struct LineItemResponse {
    pub line_no: i32,
    pub description: Option<String>,
    pub quantity: Option<f64>,
    pub unit_price: Option<f64>,
    pub total: Option<f64>,
    pub confidence: f32,
    pub raw_text: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

async fn upload_capture(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    mut multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
    let mut file_data: Option<(String, String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Upload error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or("document").to_string();
            let mime = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();

            // Validate MIME type
            if !ACCEPTED_MIME_TYPES.contains(&mime.as_str()) {
                return Err(billforge_core::Error::Validation(format!(
                    "Unsupported file type '{}'. Accepted: {}",
                    mime,
                    ACCEPTED_MIME_TYPES.join(", ")
                ))
                .into());
            }

            let bytes = field.bytes().await.map_err(|e| {
                billforge_core::Error::Validation(format!("Failed to read file: {}", e))
            })?;

            // Validate size
            if bytes.len() > MAX_FILE_SIZE {
                return Err(billforge_core::Error::Validation(format!(
                    "File too large ({} bytes). Maximum: {} bytes.",
                    bytes.len(),
                    MAX_FILE_SIZE
                ))
                .into());
            }

            file_data = Some((filename, mime, bytes.to_vec()));
        }
    }

    let (filename, mime, data) =
        file_data.ok_or_else(|| billforge_core::Error::Validation("No file provided".into()))?;

    // Select OCR provider based on tenant privacy settings.
    let provider_name = resolve_ocr_provider_name(&state.config.ocr_provider, &tenant.settings);
    let ocr_provider = ocr::create_provider(&provider_name);
    let effective_provider = ocr_provider.provider_name().to_string();
    let privacy_mode = if tenant.settings.features.local_ocr_required {
        "local_only"
    } else {
        "cloud_allowed"
    };

    // Run OCR
    let ocr_result = ocr_provider.extract(&data, &mime).await.map_err(|e| {
        tracing::warn!(error = %e, "OCR extraction failed");
        e
    })?;

    // Compute overall confidence (average of non-zero field confidences)
    let overall_confidence = compute_overall_confidence(&ocr_result);

    // Persist capture + line items
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let capture_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO invoice_captures
               (id, tenant_id, original_filename, mime_type, provider, overall_confidence, status, uploaded_by, privacy_mode)
           VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7, $8)"#,
    )
    .bind(capture_id)
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&filename)
    .bind(&mime)
    .bind(&effective_provider)
    .bind(overall_confidence)
    .bind(user.user_id.as_uuid())
    .bind(privacy_mode)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to insert capture: {}", e)))?;

    // Build line-item responses and persist them
    let mut line_items_resp = Vec::new();
    for (i, item) in ocr_result.line_items.iter().enumerate() {
        let line_no = (i + 1) as i32;
        let description = item.description.value.clone();
        let quantity = item.quantity.value;
        let unit_price = item.unit_price.value;
        let total = item.amount.value;
        let confidence = avg_line_confidence(item);
        let raw_text = description.clone();

        sqlx::query(
            r#"INSERT INTO invoice_line_items
                   (id, capture_id, tenant_id, line_no, description, quantity, unit_price, total, confidence, raw_text)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(Uuid::new_v4())
        .bind(capture_id)
        .bind(*tenant.tenant_id.as_uuid())
        .bind(line_no)
        .bind(&description)
        .bind(quantity)
        .bind(unit_price)
        .bind(total)
        .bind(confidence)
        .bind(&raw_text)
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to insert line item: {}", e))
        })?;

        line_items_resp.push(LineItemResponse {
            line_no,
            description,
            quantity,
            unit_price,
            total,
            confidence,
            raw_text,
        });
    }

    // Bridge the capture into the unified upload pipeline (#373). The
    // `/invoice-captures` endpoint used to run OCR inline, persist only to the
    // `invoice_captures`/`invoice_line_items` tables, and never feed the worker
    // or the routing engine. We now delegate to the same shared per-file
    // helper that `/invoices/upload` uses, so this capture also:
    //   - stores the original file via `state.storage.upload`
    //   - creates an `invoices` row (`InvoiceRepositoryImpl::create`)
    //   - inserts the matching `documents` row
    //   - writes an `AuditEntry` (`AuditAction::Create`, `ResourceType::Invoice`)
    //   - calls `enqueue_ocr_job` (or runs sync OCR as a fallback) so the worker
    //     performs OCR persistence + `run_straight_through_processing` for
    //     routing/categorization.
    //
    // The `invoice_captures` write above is retained as an OCR provenance
    // record; it no longer represents a divergent pipeline.
    //
    // Gated behind the same feature set as `routes::invoices` so a
    // single-pillar (`--features capture`) build still compiles. In that
    // degraded mode we expose `capture_id` as `invoice_id` so the response
    // shape stays stable; the provenance row is still authoritative.
    #[cfg(all(
        feature = "capture",
        feature = "processing",
        feature = "analytics",
        feature = "billing"
    ))]
    let invoice_id = {
        let capture_started = std::time::Instant::now();
        let upload = crate::routes::invoices::upload_invoice_file(
            &state,
            &user,
            &tenant,
            filename,
            mime,
            &data,
            capture_started,
        )
        .await?;
        upload.invoice_id
    };
    #[cfg(not(all(
        feature = "capture",
        feature = "processing",
        feature = "analytics",
        feature = "billing"
    )))]
    let invoice_id = capture_id.to_string();

    let response = CaptureResponse {
        capture_id: capture_id.to_string(),
        invoice_id,
        provider: effective_provider,
        overall_confidence,
        privacy_mode: privacy_mode.to_string(),
        line_items: line_items_resp,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

// ---------------------------------------------------------------------------
// Confidence helpers
// ---------------------------------------------------------------------------

// NOTE: `compute_overall_confidence` now lives in the shared
// `billforge_invoice_capture` crate (`confidence` module) so the worker and
// this handler cannot drift apart on confidence math (#373). It is imported
// above via `billforge_invoice_capture::compute_overall_confidence`.

/// Average confidence of a single line item's fields.
fn avg_line_confidence(item: &billforge_core::domain::ExtractedLineItem) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for c in [
        item.description.confidence,
        item.quantity.confidence,
        item.unit_price.confidence,
        item.amount.confidence,
    ] {
        if c > 0.0 {
            sum += c;
            count += 1;
        }
    }
    if count > 0 {
        (sum / count as f32).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accepted_mime_types_include_pdf_png_jpeg() {
        assert!(ACCEPTED_MIME_TYPES.contains(&"application/pdf"));
        assert!(ACCEPTED_MIME_TYPES.contains(&"image/png"));
        assert!(ACCEPTED_MIME_TYPES.contains(&"image/jpeg"));
        assert!(!ACCEPTED_MIME_TYPES.contains(&"text/plain"));
    }
}
