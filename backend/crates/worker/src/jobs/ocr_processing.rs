//! Background OCR processing job
//!
//! Runs OCR extraction asynchronously so the upload HTTP handler
//! returns immediately. The job downloads the stored document,
//! extracts invoice data via the configured OCR provider, and
//! updates the invoice record with the results.

use anyhow::{Context, Result};
use billforge_core::{
    domain::{CaptureStatus, OcrExtractionResult, QueueType},
    traits::{InvoiceRepository, StorageService, WorkQueueRepository},
    types::Money,
};
use billforge_db::{LocalStorageService, repositories::{InvoiceRepositoryImpl, WorkflowRepositoryImpl}};
use billforge_invoice_capture::ocr;
use serde::Deserialize;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::WorkerConfig;

#[derive(Debug, Deserialize)]
struct OcrJobPayload {
    invoice_id: String,
    document_id: String,
    content_type: String,
}

/// Max retries before marking an OCR job as permanently failed.
const MAX_RETRIES: u32 = 3;

/// Process an OCR extraction job for an uploaded invoice document.
pub async fn process_ocr(
    tenant_id: &str,
    payload: &serde_json::Value,
    config: &WorkerConfig,
    retry_count: u32,
) -> Result<()> {
    let payload: OcrJobPayload = serde_json::from_value(payload.clone())
        .context("Invalid OcrProcess job payload")?;

    let tenant_id: billforge_core::TenantId = tenant_id.parse()
        .context("Invalid tenant ID")?;
    let invoice_id: billforge_core::domain::InvoiceId = payload.invoice_id.parse()
        .map_err(|_| anyhow::anyhow!("Invalid invoice ID: {}", payload.invoice_id))?;
    let document_id: Uuid = payload.document_id.parse()
        .context("Invalid document ID")?;

    info!(
        tenant_id = %tenant_id,
        invoice_id = %invoice_id,
        document_id = %document_id,
        "Starting OCR processing"
    );

    // Get tenant database pool
    let pool = config.pg_manager.tenant(&tenant_id).await?;
    let repo = InvoiceRepositoryImpl::new(pool.clone());

    // Download and run OCR - if anything fails before we can update the
    // invoice, mark it as Failed so it doesn't stay stuck in Processing.
    let doc_bytes = match async {
        let storage = LocalStorageService::new(&config.storage_base_path).await
            .map_err(|e| anyhow::anyhow!("Failed to create storage service: {}", e))?;
        storage.download(&tenant_id, document_id).await
            .context("Failed to download document from storage")
    }.await {
        Ok(bytes) => bytes,
        Err(e) => {
            if retry_count + 1 >= MAX_RETRIES {
                // Final attempt failed - mark invoice as Failed and route to error queue
                error!(invoice_id = %invoice_id, error = %e, "Document download failed after all retries");
                let _ = repo.update(
                    &tenant_id, &invoice_id,
                    serde_json::json!({ "notes": format!("OCR job error: {}", e) }),
                ).await;
                let _ = repo.update_capture_status(&tenant_id, &invoice_id, CaptureStatus::Failed).await;
                route_to_ocr_error_queue(&repo, &pool, &tenant_id, &invoice_id).await;
            } else {
                warn!(invoice_id = %invoice_id, error = %e, retry = retry_count, "Document download failed, will retry");
            }
            return Err(e);
        }
    };

    // Run OCR
    let ocr_provider = ocr::create_provider(&config.ocr_provider);
    let ocr_result = ocr_provider.extract(&doc_bytes, &payload.content_type).await;

    // Process OCR result and update the invoice
    let capture_status = match &ocr_result {
        Ok(result) => {
            // Build update payload — rejects missing or zero totals
            let (updates, confidence) = match build_invoice_update_from_ocr(result, document_id) {
                Ok(v) => v,
                Err(reason) => {
                    if retry_count + 1 >= MAX_RETRIES {
                        error!(
                            invoice_id = %invoice_id,
                            reason = %reason,
                            "OCR result rejected — routing to OcrError"
                        );
                        let _ = repo.update(
                            &tenant_id, &invoice_id,
                            serde_json::json!({ "notes": format!("OCR extraction error: {}", reason) }),
                        ).await;
                        let _ = repo.update_capture_status(
                            &tenant_id, &invoice_id, CaptureStatus::Failed,
                        ).await;
                        route_to_ocr_error_queue(&repo, &pool, &tenant_id, &invoice_id).await;
                    } else {
                        warn!(
                            invoice_id = %invoice_id,
                            reason = %reason,
                            retry = retry_count,
                            "OCR result rejected, will retry"
                        );
                    }
                    return Err(anyhow::anyhow!("OCR extraction rejected: {}", reason));
                }
            };

            let status = if confidence < 0.3 {
                CaptureStatus::Failed
            } else {
                CaptureStatus::ReadyForReview
            };

            if let Err(e) = repo.update(&tenant_id, &invoice_id, updates).await {
                error!(invoice_id = %invoice_id, error = %e, "Failed to update invoice via repo");
                return Err(e.into());
            }

            // Update OCR-specific fields that repo.update() doesn't handle
            let subtotal_cents = result.subtotal.value.map(|v| Money::usd(v).amount);
            let tax_cents = result.tax_amount.value.map(|v| Money::usd(v).amount);
            let line_items_json: serde_json::Value = result
                .line_items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "description": item.description.value.clone().unwrap_or_default(),
                        "quantity": item.quantity.value,
                        "unit_price_cents": item.unit_price.value.map(|v| Money::usd(v).amount),
                        "amount_cents": Money::usd(item.amount.value.unwrap_or(0.0)).amount,
                    })
                })
                .collect::<Vec<_>>()
                .into();

            if let Err(e) = sqlx::query(
                r#"UPDATE invoices
                   SET ocr_confidence = $1,
                       subtotal_cents = COALESCE($2, subtotal_cents),
                       tax_amount_cents = COALESCE($3, tax_amount_cents),
                       line_items = $4,
                       updated_at = NOW()
                   WHERE id = $5 AND tenant_id = $6"#,
            )
            .bind(confidence)
            .bind(subtotal_cents)
            .bind(tax_cents)
            .bind(&line_items_json)
            .bind(invoice_id.as_uuid())
            .bind(*tenant_id.as_uuid())
            .execute(&*pool)
            .await
            {
                if retry_count + 1 >= MAX_RETRIES {
                    error!(
                        invoice_id = %invoice_id,
                        error = %e,
                        "Failed to update OCR-specific fields — routing to OcrError"
                    );
                    let _ = repo.update(
                        &tenant_id, &invoice_id,
                        serde_json::json!({ "notes": format!("OCR field update error: {}", e) }),
                    ).await;
                    let _ = repo.update_capture_status(
                        &tenant_id, &invoice_id, CaptureStatus::Failed,
                    ).await;
                    route_to_ocr_error_queue(&repo, &pool, &tenant_id, &invoice_id).await;
                } else {
                    warn!(
                        invoice_id = %invoice_id,
                        error = %e,
                        retry = retry_count,
                        "Failed to update OCR-specific fields, will retry"
                    );
                }
                return Err(anyhow::anyhow!("OCR field update failed: {}", e));
            }

            status
        }
        Err(e) => {
            if retry_count + 1 >= MAX_RETRIES {
                // Final attempt - mark as Failed and route to error queue
                error!(invoice_id = %invoice_id, error = %e, "OCR extraction failed after all retries");
                let _ = repo.update(
                    &tenant_id, &invoice_id,
                    serde_json::json!({ "notes": format!("OCR Error: {}", e) }),
                ).await;
                let _ = repo.update_capture_status(&tenant_id, &invoice_id, CaptureStatus::Failed).await;
                route_to_ocr_error_queue(&repo, &pool, &tenant_id, &invoice_id).await;
            } else {
                warn!(invoice_id = %invoice_id, error = %e, retry = retry_count, "OCR extraction failed, will retry");
            }
            return Err(anyhow::anyhow!("OCR extraction failed: {}", e));
        }
    };

    // Update capture status
    repo.update_capture_status(&tenant_id, &invoice_id, capture_status)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to update capture status: {}", e))?;

    // If OCR produced low confidence, route to error queue
    if capture_status == CaptureStatus::Failed {
        route_to_ocr_error_queue(&repo, &pool, &tenant_id, &invoice_id).await;
    }

    info!(
        tenant_id = %tenant_id,
        invoice_id = %invoice_id,
        capture_status = ?capture_status,
        "OCR processing completed"
    );

    Ok(())
}

/// Route an invoice to the OcrError work queue for manual review.
async fn route_to_ocr_error_queue(
    _repo: &InvoiceRepositoryImpl,
    pool: &std::sync::Arc<sqlx::PgPool>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
) {
    let queue_repo = WorkflowRepositoryImpl::new(pool.clone());
    match WorkQueueRepository::get_by_type(&queue_repo, tenant_id, QueueType::OcrError).await {
        Ok(Some(error_queue)) => {
            if let Err(e) = WorkQueueRepository::move_item(
                &queue_repo, tenant_id, invoice_id, &error_queue.id, None,
            ).await {
                warn!(invoice_id = %invoice_id, error = %e, "Failed to create OCR error queue item");
            }
            if let Err(e) = sqlx::query(
                "UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(error_queue.id.0)
            .bind(invoice_id.as_uuid())
            .execute(&**pool)
            .await {
                warn!(invoice_id = %invoice_id, error = %e, "Failed to update invoice current_queue_id");
            }
        }
        Ok(None) => {
            error!(invoice_id = %invoice_id, "No OcrError queue found for tenant");
        }
        Err(e) => {
            error!(invoice_id = %invoice_id, error = %e, "Failed to look up OcrError queue");
        }
    }
}

/// Build the invoice update JSON payload and average confidence from an OCR result.
///
/// Returns `Err` when the total is missing or zero — such invoices must not
/// reach `ReadyForReview` with a $0 amount.
fn build_invoice_update_from_ocr(
    result: &OcrExtractionResult,
    document_id: Uuid,
) -> Result<(serde_json::Value, f32), String> {
    // Reject missing total
    let total_raw = result
        .total_amount
        .value
        .ok_or_else(|| "OCR extraction produced no total_amount".to_string())?;

    // Reject zero-or-negative total
    if total_raw <= 0.0 {
        return Err(format!(
            "OCR extraction produced non-positive total_amount: {}",
            total_raw
        ));
    }

    let total_amount = Money::usd(total_raw);
    let vendor_name = result
        .vendor_name
        .value
        .clone()
        .unwrap_or_else(|| "Unknown Vendor".to_string());
    let invoice_number = result
        .invoice_number
        .value
        .clone()
        .unwrap_or_else(|| {
            format!(
                "UPLOAD-{}",
                &document_id.to_string()[..8].to_uppercase()
            )
        });
    let currency = result
        .currency
        .value
        .clone()
        .unwrap_or_else(|| "USD".to_string());

    let mut updates = serde_json::json!({
        "vendor_name": vendor_name,
        "invoice_number": invoice_number,
        "total_amount": {
            "amount": total_amount.amount,
            "currency": currency,
        },
    });
    if let Some(date) = result.invoice_date.value {
        updates["invoice_date"] = serde_json::json!(date.format("%Y-%m-%d").to_string());
    }
    if let Some(date) = result.due_date.value {
        updates["due_date"] = serde_json::json!(date.format("%Y-%m-%d").to_string());
    }
    if let Some(ref po) = result.po_number.value {
        updates["po_number"] = serde_json::json!(po);
    }

    let confidence = [
        result.invoice_number.confidence,
        result.vendor_name.confidence,
        result.total_amount.confidence,
    ]
    .iter()
    .sum::<f32>()
        / 3.0;

    Ok((updates, confidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::domain::ExtractedField;

    #[test]
    fn test_ocr_payload_deserialization() {
        let json = serde_json::json!({
            "invoice_id": "11111111-1111-1111-1111-111111111111",
            "document_id": "22222222-2222-2222-2222-222222222222",
            "content_type": "application/pdf",
        });

        let payload: OcrJobPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.invoice_id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(payload.document_id, "22222222-2222-2222-2222-222222222222");
        assert_eq!(payload.content_type, "application/pdf");
    }

    #[test]
    fn test_ocr_payload_missing_fields() {
        let json = serde_json::json!({
            "invoice_id": "11111111-1111-1111-1111-111111111111",
        });

        let result: std::result::Result<OcrJobPayload, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn build_invoice_update_rejects_missing_total() {
        let result = OcrExtractionResult {
            invoice_number: ExtractedField::with_value("INV-001".to_string(), 0.9),
            invoice_date: ExtractedField::empty(),
            due_date: ExtractedField::empty(),
            vendor_name: ExtractedField::with_value("ACME Corp".to_string(), 0.8),
            vendor_address: ExtractedField::empty(),
            subtotal: ExtractedField::empty(),
            tax_amount: ExtractedField::empty(),
            total_amount: ExtractedField::empty(),
            currency: ExtractedField::with_value("USD".to_string(), 0.9),
            po_number: ExtractedField::empty(),
            line_items: vec![],
            raw_text: String::new(),
            processing_time_ms: 0,
        };
        let doc_id = Uuid::new_v4();
        let err = build_invoice_update_from_ocr(&result, doc_id).unwrap_err();
        assert!(
            err.contains("total_amount"),
            "expected error to mention total_amount, got: {}",
            err
        );
    }

    #[test]
    fn build_invoice_update_rejects_zero_total() {
        let result = OcrExtractionResult {
            invoice_number: ExtractedField::with_value("INV-001".to_string(), 0.9),
            invoice_date: ExtractedField::empty(),
            due_date: ExtractedField::empty(),
            vendor_name: ExtractedField::with_value("ACME Corp".to_string(), 0.8),
            vendor_address: ExtractedField::empty(),
            subtotal: ExtractedField::empty(),
            tax_amount: ExtractedField::empty(),
            total_amount: ExtractedField::with_value(0.0, 0.5),
            currency: ExtractedField::with_value("USD".to_string(), 0.9),
            po_number: ExtractedField::empty(),
            line_items: vec![],
            raw_text: String::new(),
            processing_time_ms: 0,
        };
        let doc_id = Uuid::new_v4();
        let err = build_invoice_update_from_ocr(&result, doc_id).unwrap_err();
        assert!(
            err.contains("non-positive"),
            "expected error to mention non-positive total, got: {}",
            err
        );
    }

    #[test]
    fn build_invoice_update_happy_path_includes_total_cents() {
        let result = OcrExtractionResult {
            invoice_number: ExtractedField::with_value("INV-123".to_string(), 0.95),
            invoice_date: ExtractedField::empty(),
            due_date: ExtractedField::empty(),
            vendor_name: ExtractedField::with_value("Test Vendor".to_string(), 0.85),
            vendor_address: ExtractedField::empty(),
            subtotal: ExtractedField::empty(),
            tax_amount: ExtractedField::empty(),
            total_amount: ExtractedField::with_value(123.45, 0.90),
            currency: ExtractedField::with_value("USD".to_string(), 0.99),
            po_number: ExtractedField::empty(),
            line_items: vec![],
            raw_text: String::new(),
            processing_time_ms: 100,
        };
        let doc_id = Uuid::new_v4();
        let (updates, confidence) = build_invoice_update_from_ocr(&result, doc_id).unwrap();

        // $123.45 => 12345 cents
        assert_eq!(updates["total_amount"]["amount"], 12345);
        assert_eq!(updates["vendor_name"], "Test Vendor");
        assert_eq!(updates["invoice_number"], "INV-123");

        // confidence = mean(0.95, 0.85, 0.90) = 0.90
        let expected_confidence = (0.95 + 0.85 + 0.90) / 3.0;
        assert!(
            (confidence - expected_confidence).abs() < 0.001,
            "expected confidence {}, got {}",
            expected_confidence,
            confidence
        );
    }

    #[test]
    fn build_invoice_update_applies_upload_fallback_when_invoice_number_missing() {
        let result = OcrExtractionResult {
            invoice_number: ExtractedField::empty(),
            invoice_date: ExtractedField::empty(),
            due_date: ExtractedField::empty(),
            vendor_name: ExtractedField::empty(),
            vendor_address: ExtractedField::empty(),
            subtotal: ExtractedField::empty(),
            tax_amount: ExtractedField::empty(),
            total_amount: ExtractedField::with_value(50.00, 0.7),
            currency: ExtractedField::empty(),
            po_number: ExtractedField::empty(),
            line_items: vec![],
            raw_text: String::new(),
            processing_time_ms: 0,
        };
        // Use a known UUID so we can check the prefix
        let doc_id = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let (updates, _confidence) = build_invoice_update_from_ocr(&result, doc_id).unwrap();

        let inv_num = updates["invoice_number"].as_str().unwrap();
        assert!(
            inv_num.starts_with("UPLOAD-"),
            "expected UPLOAD- prefix, got: {}",
            inv_num
        );
        assert_eq!(
            inv_num, "UPLOAD-AAAAAAAA",
            "expected first 8 hex chars uppercased"
        );
        assert_eq!(updates["vendor_name"], "Unknown Vendor");
    }
}
