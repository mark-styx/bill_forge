//! Invoice routes (Invoice Capture module)

use crate::error::ApiResult;
use crate::extractors::InvoiceCaptureAccess;
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{CreateInvoiceInput, Invoice, InvoiceFilters, CaptureStatus, ProcessingStatus},
    traits::{InvoiceRepository, OcrService},
    types::{Money, PaginatedResponse, Pagination},
};
use billforge_invoice_capture::ocr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// UUID of the OCR Error Queue (from seed data)
const OCR_ERROR_QUEUE_ID: &str = "11111111-4444-5555-6666-777777770001";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_invoices))
        .route("/", post(create_invoice))
        .route("/upload", post(upload_invoice))
        .route("/:id", get(get_invoice))
        .route("/:id", put(update_invoice))
        .route("/:id", delete(delete_invoice))
        .route("/:id/ocr", post(rerun_ocr))
        .route("/:id/submit", post(submit_for_processing))
}

#[derive(Debug, Deserialize)]
pub struct ListInvoicesQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub vendor_id: Option<String>,
    pub capture_status: Option<String>,
    pub processing_status: Option<String>,
    pub search: Option<String>,
}

async fn list_invoices(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Query(query): Query<ListInvoicesQuery>,
) -> ApiResult<Json<PaginatedResponse<Invoice>>> {
    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(25),
    };

    let filters = InvoiceFilters {
        vendor_id: query.vendor_id.and_then(|s| Uuid::parse_str(&s).ok()),
        search: query.search,
        capture_status: query.capture_status.and_then(|s| CaptureStatus::from_str(&s)),
        processing_status: query.processing_status.and_then(|s| ProcessingStatus::from_str(&s)),
        ..Default::default()
    };

    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    let invoices = repo.list(&tenant.tenant_id, &filters, &pagination).await?;

    Ok(Json(invoices))
}

async fn get_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Invoice>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    let invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(invoice))
}

async fn create_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Json(input): Json<CreateInvoiceInput>,
) -> ApiResult<Json<Invoice>> {
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    let invoice = repo.create(&tenant.tenant_id, input, &user.user_id).await?;

    Ok(Json(invoice))
}

async fn update_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(updates): Json<serde_json::Value>,
) -> ApiResult<Json<Invoice>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    let invoice = repo.update(&tenant.tenant_id, &invoice_id, updates).await?;

    Ok(Json(invoice))
}

async fn delete_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    repo.delete(&tenant.tenant_id, &invoice_id).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub invoice_id: String,
    pub document_id: String,
    pub message: String,
}

async fn upload_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadResponse>> {
    // Process multipart upload
    while let Some(field) = multipart.next_field().await
        .map_err(|e| billforge_core::Error::Validation(format!("Upload error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let file_name = field.file_name().unwrap_or("document.pdf").to_string();
            let content_type = field.content_type().unwrap_or("application/pdf").to_string();
            let data = field.bytes().await
                .map_err(|e| billforge_core::Error::Validation(format!("Failed to read file: {}", e)))?;

            // Store the document via storage service
            let document_id = state.storage.upload(&tenant.tenant_id, &file_name, &data, &content_type).await
                .map_err(|e| billforge_core::Error::Database(format!("Failed to store document: {}", e)))?;

            let storage_key = format!("{}/{}", tenant.tenant_id.as_str(), document_id);

            // Run OCR on the document
            let ocr_provider = ocr::create_provider("tesseract");
            let ocr_result = ocr_provider.extract(&data, &content_type).await;

            // Determine capture status and invoice data based on OCR result
            let (capture_status, vendor_name, invoice_number, total_amount, ocr_confidence, ocr_error, invoice_date, due_date, po_number) =
                match &ocr_result {
                    Ok(result) => {
                        let confidence = [
                            result.invoice_number.confidence,
                            result.vendor_name.confidence,
                            result.total_amount.confidence,
                        ].iter().sum::<f32>() / 3.0;

                        // If confidence is too low, mark as failed
                        let status = if confidence < 0.3 {
                            CaptureStatus::Failed
                        } else {
                            CaptureStatus::ReadyForReview
                        };

                        (
                            status,
                            result.vendor_name.value.clone().unwrap_or_else(|| "Unknown Vendor".to_string()),
                            result.invoice_number.value.clone().unwrap_or_else(|| format!("UPLOAD-{}", &document_id.to_string()[..8].to_uppercase())),
                            Money::usd(result.total_amount.value.unwrap_or(0.0)),
                            Some(confidence),
                            None,
                            result.invoice_date.value,
                            result.due_date.value,
                            result.po_number.value.clone(),
                        )
                    }
                    Err(e) => {
                        tracing::warn!("OCR failed for document {}: {}", document_id, e);
                        (
                            CaptureStatus::Failed,
                            "Unknown Vendor".to_string(),
                            format!("UPLOAD-{}", &document_id.to_string()[..8].to_uppercase()),
                            Money::new(0, "USD".to_string()),
                            None,
                            Some(e.to_string()),
                            None,
                            None,
                            None,
                        )
                    }
                };

            // Build notes with OCR error if applicable
            let notes = match ocr_error {
                Some(err) => Some(format!("Uploaded file: {}. OCR Error: {}", file_name, err)),
                None => Some(format!("Uploaded file: {}", file_name)),
            };

            // Create invoice from OCR result (or fallback values)
            let invoice_input = CreateInvoiceInput {
                vendor_id: None,
                vendor_name,
                invoice_number,
                invoice_date,
                due_date,
                po_number,
                subtotal: None,
                tax_amount: None,
                total_amount,
                currency: "USD".to_string(),
                line_items: vec![],
                document_id,
                ocr_confidence,
                notes,
                tags: vec![],
                department: None,
                gl_code: None,
                cost_center: None,
            };

            let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
            let invoice = repo.create(&tenant.tenant_id, invoice_input, &user.user_id).await?;

            // Update capture status
            repo.update_capture_status(&tenant.tenant_id, &invoice.id, capture_status).await?;

            // If OCR failed, move to error queue
            if capture_status == CaptureStatus::Failed {
                let error_queue_id = Uuid::parse_str(OCR_ERROR_QUEUE_ID).ok();
                if let Some(queue_id) = error_queue_id {
                    // Update invoice queue
                    let db = state.db.tenant(&tenant.tenant_id).await?;
                    let conn = db.connection().await;
                    let conn = conn.lock().await;

                    conn.execute(
                        "UPDATE invoices SET current_queue_id = ? WHERE id = ?",
                        rusqlite::params![queue_id.to_string(), invoice.id.to_string()],
                    ).ok();

                    // Create queue item
                    let queue_item_id = Uuid::new_v4();
                    conn.execute(
                        "INSERT INTO queue_items (id, queue_id, invoice_id, priority, entered_at) VALUES (?, ?, ?, 2, datetime('now'))",
                        rusqlite::params![queue_item_id.to_string(), queue_id.to_string(), invoice.id.to_string()],
                    ).ok();
                }
            }

            // Store document metadata in the tenant database
            let db = state.db.tenant(&tenant.tenant_id).await?;
            let conn = db.connection().await;
            let conn = conn.lock().await;

            conn.execute(
                "INSERT INTO documents (id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at) VALUES (?, ?, ?, ?, ?, ?, 'invoice_original', ?, datetime('now'))",
                rusqlite::params![
                    document_id.to_string(),
                    file_name,
                    content_type,
                    data.len() as i64,
                    storage_key,
                    invoice.id.to_string(),
                    user.user_id.to_string(),
                ],
            ).map_err(|e| billforge_core::Error::Database(format!("Failed to store document metadata: {}", e)))?;

            let message = if capture_status == CaptureStatus::Failed {
                format!("File '{}' uploaded. OCR failed - invoice sent to error queue for manual review.", file_name)
            } else {
                format!("File '{}' uploaded and processed. Invoice ready for review.", file_name)
            };

            return Ok(Json(UploadResponse {
                invoice_id: invoice.id.to_string(),
                document_id: document_id.to_string(),
                message,
            }));
        }
    }

    Err(billforge_core::Error::Validation("No file provided".to_string()).into())
}

async fn rerun_ocr(
    State(_state): State<AppState>,
    InvoiceCaptureAccess(_user, _tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    // TODO: Implement OCR reprocessing
    Ok(Json(serde_json::json!({
        "message": "OCR reprocessing queued",
        "invoice_id": id
    })))
}

async fn submit_for_processing(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        ProcessingStatus::Submitted,
    ).await?;

    Ok(Json(serde_json::json!({
        "message": "Invoice submitted for processing",
        "invoice_id": id
    })))
}
