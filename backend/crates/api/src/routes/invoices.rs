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
    domain::{AuditAction, AuditEntry, CreateInvoiceInput, CreateLineItemInput, ExtractedLineItem, Invoice, InvoiceFilters, CaptureStatus, ProcessingStatus, QueueType, ResourceType},
    traits::{AuditService, InvoiceRepository},
    types::{Money, PaginatedResponse, Pagination},
};
use billforge_invoice_capture::ocr;
use billforge_invoice_processing::feedback_loop::{CategorizationFeedback, FeedbackLearning, FeedbackType};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

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
        .route("/:id/suggest-categories", post(suggest_categories))
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

#[utoipa::path(
    get,
    path = "/invoices",
    tag = "Invoices",
    params(
        ("page" = Option<u32>, Query, description = "Page number (1-indexed)"),
        ("per_page" = Option<u32>, Query, description = "Items per page"),
        ("vendor_id" = Option<String>, Query, description = "Filter by vendor ID"),
        ("capture_status" = Option<String>, Query, description = "Filter by capture status"),
        ("processing_status" = Option<String>, Query, description = "Filter by processing status"),
        ("search" = Option<String>, Query, description = "Search term")
    ),
    responses(
        (status = 200, description = "List of invoices"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
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

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let invoices = repo.list(&tenant.tenant_id, &filters, &pagination).await?;

    Ok(Json(invoices))
}

#[utoipa::path(
    get,
    path = "/invoices/{id}",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "Invoice details"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn get_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Invoice>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(invoice))
}

#[utoipa::path(
    post,
    path = "/invoices",
    tag = "Invoices",
    request_body = String,
    responses(
        (status = 200, description = "Invoice created"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn create_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Json(input): Json<CreateInvoiceInput>,
) -> ApiResult<Json<Invoice>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let invoice = repo.create(&tenant.tenant_id, input, &user.user_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::Invoice,
        invoice.id.to_string(),
        format!("Created invoice {}", invoice.invoice_number),
    ).with_user_email(&user.email)
     .with_new_value(serde_json::to_value(&invoice).unwrap_or_default());
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(invoice))
}

#[utoipa::path(
    put,
    path = "/invoices/{id}",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "Invoice updated"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn update_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(updates): Json<serde_json::Value>,
) -> ApiResult<Json<Invoice>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    let old_invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?;
    let invoice = repo.update(&tenant.tenant_id, &invoice_id, updates.clone()).await?;

    // Best-effort categorization feedback recording (refs #159)
    if let Some(ref old) = old_invoice {
        let has_cat_key = updates.get("gl_code").is_some()
            || updates.get("department").is_some()
            || updates.get("cost_center").is_some();
        if has_cat_key {
            let gl_changed = old.gl_code != invoice.gl_code;
            let dept_changed = old.department != invoice.department;
            let cc_changed = old.cost_center != invoice.cost_center;
            let feedback_type = if gl_changed || dept_changed || cc_changed {
                FeedbackType::Correction
            } else {
                FeedbackType::Acceptance
            };
            let mut summary: String = invoice
                .line_items
                .iter()
                .map(|li| li.description.clone())
                .collect::<Vec<_>>()
                .join("; ");
            if summary.len() > 500 {
                let mut end = 500;
                while !summary.is_char_boundary(end) {
                    end -= 1;
                }
                summary.truncate(end);
            }

            let feedback = CategorizationFeedback {
                tenant_id: tenant.tenant_id.as_str().to_string(),
                invoice_id: *invoice.id.as_uuid(),
                vendor_id: invoice.vendor_id,
                vendor_name: invoice.vendor_name.clone(),
                suggested_gl_code: old.gl_code.clone(),
                suggested_department: old.department.clone(),
                suggested_cost_center: old.cost_center.clone(),
                suggestion_confidence: old.categorization_confidence,
                suggestion_source: Some("auto".to_string()),
                accepted_gl_code: invoice.gl_code.clone(),
                accepted_department: invoice.department.clone(),
                accepted_cost_center: invoice.cost_center.clone(),
                line_items_summary: summary,
                total_amount_cents: invoice.total_amount.amount,
                feedback_type,
            };
            if let Err(e) =
                FeedbackLearning::new((*pool).clone()).record_feedback(feedback).await
            {
                tracing::warn!(
                    error = %e,
                    invoice_id = %invoice.id,
                    "Failed to record categorization feedback"
                );
            }
        }
    }

    let mut audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::Invoice,
        invoice.id.to_string(),
        format!("Updated invoice {}", invoice.invoice_number),
    ).with_user_email(&user.email)
     .with_new_value(serde_json::to_value(&invoice).unwrap_or_default());
    if let Some(old) = old_invoice {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or_default());
    }
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(invoice))
}

#[utoipa::path(
    delete,
    path = "/invoices/{id}",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "Invoice deleted"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn delete_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    let old_invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?;
    repo.delete(&tenant.tenant_id, &invoice_id).await?;

    let mut audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::Invoice,
        id.clone(), "Deleted invoice",
    ).with_user_email(&user.email);
    if let Some(old) = old_invoice {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or_default());
    }
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    pub invoice_id: String,
    pub document_id: String,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/invoices/upload",
    tag = "Invoices",
    responses(
        (status = 200, description = "Invoice uploaded and OCR processing queued", body = UploadResponse),
        (status = 400, description = "Invalid upload"),
        (status = 401, description = "Unauthorized")
    )
)]
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
            let file_name_for_msg = file_name.clone();
            let document_id = state.storage.upload(&tenant.tenant_id, &file_name, &data, &content_type).await
                .map_err(|e| billforge_core::Error::Database(format!("Failed to store document: {}", e)))?;

            let storage_key = format!("{}/{}", tenant.tenant_id.as_str(), document_id);

            // Create invoice with Processing status (placeholder data until OCR completes)
            let invoice_input = CreateInvoiceInput {
                vendor_id: None,
                vendor_name: "Processing...".to_string(),
                invoice_number: format!("UPLOAD-{}", document_id),
                invoice_date: None,
                due_date: None,
                po_number: None,
                subtotal: None,
                tax_amount: None,
                total_amount: Money::new(0, "USD".to_string()),
                currency: "USD".to_string(),
                line_items: vec![],
                document_id,
                ocr_confidence: None,
                notes: Some(format!("Uploaded file: {}", file_name)),
                tags: vec![],
                department: None,
                gl_code: None,
                cost_center: None,
            };

            let pool = state.db.tenant(&tenant.tenant_id).await?;
            let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
            let invoice = repo.create(&tenant.tenant_id, invoice_input, &user.user_id).await?;

            // Set capture status to Processing
            repo.update_capture_status(&tenant.tenant_id, &invoice.id, CaptureStatus::Processing).await?;

            // Store document metadata
            sqlx::query(
                "INSERT INTO documents (id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'invoice_original', $8, NOW())"
            )
            .bind(document_id)
            .bind(*tenant.tenant_id.as_uuid())
            .bind(&file_name)
            .bind(&content_type)
            .bind(data.len() as i64)
            .bind(&storage_key)
            .bind(invoice.id.as_uuid())
            .bind(user.user_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to store document metadata: {}", e)))?;

            // Audit: invoice created via upload
            let audit_entry = AuditEntry::new(
                tenant.tenant_id.clone(), Some(user.user_id.clone()),
                AuditAction::Create, ResourceType::Invoice,
                invoice.id.to_string(),
                format!("Uploaded invoice from file '{}'", file_name),
            ).with_user_email(&user.email)
             .with_metadata(serde_json::json!({
                 "document_id": document_id.to_string(),
                 "file_name": file_name,
                 "content_type": content_type,
             }));
            let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
            if let Err(e) = audit_repo.log(audit_entry).await {
                tracing::warn!(error = %e, "Failed to log audit entry");
            }

            // Enqueue async OCR job if Redis is available, otherwise fall back to sync
            let message = if let Some(ref redis_client) = state.redis {
                match enqueue_ocr_job(
                    redis_client,
                    &tenant.tenant_id,
                    &invoice.id,
                    document_id,
                    &content_type,
                ).await {
                    Ok(_) => {
                        tracing::info!(
                            invoice_id = %invoice.id,
                            "OCR job enqueued for async processing"
                        );
                        format!("File '{}' uploaded. OCR processing queued - poll GET /invoices/{} for status.", file_name_for_msg, invoice.id)
                    }
                    Err(e) => {
                        tracing::warn!(
                            invoice_id = %invoice.id,
                            error = %e,
                            "Failed to enqueue OCR job, falling back to sync"
                        );
                        // Fall back to synchronous OCR
                        let status = run_sync_ocr(&state, &tenant.tenant_id, &invoice.id, &data, &content_type, &repo, &pool).await;
                        sync_ocr_message(&file_name_for_msg, status)
                    }
                }
            } else {
                // No Redis configured, run OCR synchronously
                let status = run_sync_ocr(&state, &tenant.tenant_id, &invoice.id, &data, &content_type, &repo, &pool).await;
                sync_ocr_message(&file_name_for_msg, status)
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

/// Enqueue an OCR processing job to the Redis job queue.
async fn enqueue_ocr_job(
    redis_client: &redis::Client,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
    document_id: Uuid,
    content_type: &str,
) -> Result<(), billforge_core::Error> {
    let job = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "job_type": "ocr_process",
        "tenant_id": tenant_id.to_string(),
        "payload": {
            "invoice_id": invoice_id.to_string(),
            "document_id": document_id.to_string(),
            "content_type": content_type,
        },
        "created_at": chrono::Utc::now(),
        "retry_count": 0,
    });

    let job_json = serde_json::to_string(&job)
        .map_err(|e| billforge_core::Error::Database(format!("Failed to serialize OCR job: {}", e)))?;

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Redis connection failed: {}", e)))?;

    conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to enqueue OCR job: {}", e)))?;

    Ok(())
}

/// Synchronous OCR fallback when Redis is unavailable.
/// Runs OCR inline and updates the invoice with results.
/// Returns the resulting capture status.
async fn run_sync_ocr(
    state: &AppState,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
    data: &[u8],
    content_type: &str,
    repo: &billforge_db::repositories::InvoiceRepositoryImpl,
    pool: &std::sync::Arc<sqlx::PgPool>,
) -> CaptureStatus {
    let ocr_provider = ocr::create_provider(&state.config.ocr_provider);
    let ocr_result = ocr_provider.extract(data, content_type).await;

    let capture_status = match &ocr_result {
        Ok(result) => {
            let confidence = [
                result.invoice_number.confidence,
                result.vendor_name.confidence,
                result.total_amount.confidence,
            ]
            .iter()
            .sum::<f32>()
                / 3.0;

            let status = if confidence < 0.3 {
                CaptureStatus::Failed
            } else {
                CaptureStatus::ReadyForReview
            };

            let vendor_name = result.vendor_name.value.clone()
                .unwrap_or_else(|| "Unknown Vendor".to_string());
            let invoice_number = result.invoice_number.value.clone()
                .unwrap_or_else(|| format!("UPLOAD-{}", &Uuid::new_v4().to_string()[..8].to_uppercase()));
            let total_amount = Money::usd(result.total_amount.value.unwrap_or(0.0));
            let currency = result.currency.value.clone().unwrap_or_else(|| "USD".to_string());

            // Use repo.update() for fields it supports
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

            if let Err(e) = repo.update(tenant_id, invoice_id, updates).await {
                tracing::error!(invoice_id = %invoice_id, error = %e, "Failed to update invoice with OCR results");
            }

            // Update OCR-specific fields via raw SQL
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
            .execute(&**pool)
            .await
            {
                tracing::warn!(invoice_id = %invoice_id, error = %e, "Failed to update OCR-specific fields");
            }

            status
        }
        Err(e) => {
            tracing::warn!(invoice_id = %invoice_id, error = %e, "Sync OCR failed");
            let _ = repo.update(
                tenant_id,
                invoice_id,
                serde_json::json!({ "notes": format!("OCR Error: {}", e) }),
            ).await;
            CaptureStatus::Failed
        }
    };

    if let Err(e) = repo.update_capture_status(tenant_id, invoice_id, capture_status).await {
        tracing::error!(invoice_id = %invoice_id, error = %e, "Failed to update capture status");
    }

    // Move to error queue if OCR failed
    if capture_status == CaptureStatus::Failed {
        let queue_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
        if let Ok(Some(error_queue)) = billforge_core::traits::WorkQueueRepository::get_by_type(
            &queue_repo,
            tenant_id,
            QueueType::OcrError,
        ).await {
            let _ = billforge_core::traits::WorkQueueRepository::move_item(
                &queue_repo,
                tenant_id,
                invoice_id,
                &error_queue.id,
                None,
            ).await;
            let _ = sqlx::query(
                "UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2"
            )
            .bind(error_queue.id.0)
            .bind(invoice_id.as_uuid())
            .execute(&**pool)
            .await;
        }
    }

    capture_status
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/ocr",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "OCR reprocessing queued or completed"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn rerun_ocr(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id: billforge_core::domain::InvoiceId = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Verify invoice exists and get its document_id
    let invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Look up the stored MIME type for this invoice's primary document
    let mime_type: String = sqlx::query_scalar(
        "SELECT mime_type FROM documents WHERE id = $1 AND tenant_id = $2"
    )
    .bind(invoice.document_id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to look up document MIME type: {}", e)))?
    .unwrap_or_else(|| "application/pdf".to_string());

    // Mark as Processing
    repo.update_capture_status(&tenant.tenant_id, &invoice_id, CaptureStatus::Processing).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::OcrRerun, ResourceType::Invoice,
        id.clone(), format!("Reran OCR for invoice {}", invoice.invoice_number),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    // Try async via Redis, fall back to sync
    if let Some(ref redis_client) = state.redis {
        match enqueue_ocr_job(
            redis_client,
            &tenant.tenant_id,
            &invoice_id,
            invoice.document_id,
            &mime_type,
        ).await {
            Ok(_) => {
                tracing::info!(invoice_id = %invoice_id, "OCR reprocessing job enqueued");
                return Ok(Json(serde_json::json!({
                    "message": "OCR reprocessing queued",
                    "invoice_id": id,
                    "status": "processing",
                })));
            }
            Err(e) => {
                tracing::warn!(invoice_id = %invoice_id, error = %e, "Failed to enqueue OCR rerun, falling back to sync");
            }
        }
    }

    // Synchronous fallback
    let invoice_repo = std::sync::Arc::new(repo);
    let capture_service = billforge_invoice_capture::InvoiceCaptureService::new(
        &state.config.ocr_provider,
        invoice_repo,
        state.storage.clone(),
    );

    let ocr_result = capture_service.reprocess_ocr(&tenant.tenant_id, &invoice_id).await?;

    Ok(Json(serde_json::json!({
        "message": "OCR reprocessing completed",
        "invoice_id": id,
        "vendor_name": ocr_result.vendor_name.value,
        "invoice_number": ocr_result.invoice_number.value,
        "total_amount": ocr_result.total_amount.value,
        "invoice_date": ocr_result.invoice_date.value,
        "due_date": ocr_result.due_date.value,
    })))
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/submit",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "Invoice submitted for processing"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn submit_for_processing(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get the invoice
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let mut invoice = invoice_repo.get_by_id(&tenant.tenant_id, &invoice_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Block submission while OCR is still running asynchronously
    if invoice.capture_status == CaptureStatus::Processing {
        return Err(billforge_core::Error::Validation(
            "Invoice is still being processed by OCR. Please wait for processing to complete before submitting.".to_string(),
        ).into());
    }

    // Update status to Submitted
    invoice_repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        ProcessingStatus::Submitted,
    ).await?;
    invoice.processing_status = ProcessingStatus::Submitted;

    // Run auto-categorization if ANY categorization field is missing
    if invoice.gl_code.is_none() || invoice.department.is_none() || invoice.cost_center.is_none() {
        // Capture which fields already have values so we don't overwrite them
        let had_gl_code = invoice.gl_code.is_some();
        let had_department = invoice.department.is_some();
        let had_cost_center = invoice.cost_center.is_some();

        let cat_engine = billforge_invoice_processing::CategorizationEngine::new((*pool).clone());
        let line_items: Vec<billforge_invoice_processing::categorization::LineItemInput> = invoice.line_items.iter().map(|li| {
            billforge_invoice_processing::categorization::LineItemInput {
                description: li.description.clone(),
                quantity: li.quantity,
                amount: li.amount.amount as f64 / 100.0,
            }
        }).collect();
        let total = invoice.total_amount.amount as f64 / 100.0;
        let tenant_id_str = tenant.tenant_id.to_string();

        // Try ML-based categorization first if OpenAI API key is available,
        // then fall back to rule-based engine
        let cat_result = if let Ok(openai_api_key) = std::env::var("OPENAI_API_KEY") {
            let ml_categorizer = billforge_invoice_processing::MLCategorizer::new(
                (*pool).clone(),
                openai_api_key,
            );
            match ml_categorizer
                .suggest_categories_ml(
                    &tenant_id_str,
                    invoice.vendor_id,
                    &invoice.vendor_name,
                    &line_items,
                    total,
                )
                .await
            {
                Ok(ml_result) => {
                    tracing::info!(
                        invoice_id = %invoice_id,
                        confidence = ml_result.overall_confidence,
                        "ML auto-categorization succeeded"
                    );
                    Ok(ml_result)
                }
                Err(e) => {
                    tracing::warn!(
                        invoice_id = %invoice_id,
                        error = %e,
                        "ML auto-categorization failed, falling back to rule-based"
                    );
                    cat_engine.suggest_categories(
                        &tenant_id_str,
                        invoice.vendor_id,
                        &invoice.vendor_name,
                        &line_items,
                        total,
                    ).await
                }
            }
        } else {
            cat_engine.suggest_categories(
                &tenant_id_str,
                invoice.vendor_id,
                &invoice.vendor_name,
                &line_items,
                total,
            ).await
        };

        match cat_result {
            Ok(categorization) => {
                // Build updates JSON, only including fields that were originally None
                let mut updates = serde_json::json!({
                    "categorization_confidence": categorization.overall_confidence,
                });
                if !had_gl_code {
                    updates["gl_code"] = serde_json::json!(
                        categorization.gl_code.as_ref().map(|s| &s.value)
                    );
                }
                if !had_department {
                    updates["department"] = serde_json::json!(
                        categorization.department.as_ref().map(|s| &s.value)
                    );
                }
                if !had_cost_center {
                    updates["cost_center"] = serde_json::json!(
                        categorization.cost_center.as_ref().map(|s| &s.value)
                    );
                }
                if let Err(e) = invoice_repo.update(&tenant.tenant_id, &invoice_id, updates).await {
                    tracing::warn!(invoice_id = %invoice_id, error = %e, "Failed to persist auto-categorization");
                } else {
                    // Re-fetch so the workflow engine sees populated fields
                    if let Some(refetched) = invoice_repo.get_by_id(&tenant.tenant_id, &invoice_id).await? {
                        invoice = refetched;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    invoice_id = %invoice_id,
                    error = %e,
                    "Auto-categorization failed, continuing without"
                );
            }
        }
    }

    // Get workflow rules for processing
    let workflow_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let rules = billforge_core::traits::WorkflowRuleRepository::list(
        &workflow_repo,
        &tenant.tenant_id,
        None,
    ).await?;

    // Filter to active rules only
    let _active_rules: Vec<_> = rules.into_iter().filter(|r| r.is_active).collect();

    // Process invoice through workflow engine
    use std::sync::Arc;
    let engine = billforge_invoice_processing::WorkflowEngine::new(
        Arc::new(invoice_repo) as Arc<dyn billforge_core::traits::InvoiceRepository>,
        Arc::new(workflow_repo) as Arc<dyn billforge_core::traits::WorkflowRuleRepository>,
        Arc::new(billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone()))
            as Arc<dyn billforge_core::traits::ApprovalRepository>,
    );

    let final_status = engine.process_invoice(&tenant.tenant_id, &invoice).await?;

    // Update invoice with final status from workflow
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    invoice_repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        final_status,
    ).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoiceSubmitted, ResourceType::Invoice,
        id.clone(),
        format!("Submitted invoice for processing, status: {:?}", final_status),
    ).with_user_email(&user.email)
     .with_metadata(serde_json::json!({ "processing_status": format!("{:?}", final_status) }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    // Assign invoice to the appropriate workflow queue based on final_status
    let target_queue_type = match final_status {
        ProcessingStatus::Approved | ProcessingStatus::ReadyForPayment => QueueType::Payment,
        ProcessingStatus::PendingApproval => QueueType::Approval,
        ProcessingStatus::Rejected | ProcessingStatus::Voided | ProcessingStatus::Paid => {
            // Terminal states - no queue needed
            return Ok(Json(serde_json::json!({
                "message": "Invoice submitted for processing",
                "invoice_id": id,
                "status": format!("{:?}", final_status).to_lowercase()
            })));
        }
        _ => QueueType::Review, // Default fallback to AP review queue
    };

    let queue_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let queue_id = match billforge_core::traits::WorkQueueRepository::get_by_type(
        &queue_repo,
        &tenant.tenant_id,
        target_queue_type,
    ).await {
        Ok(Some(queue)) => Some(queue.id),
        Ok(None) => {
            tracing::warn!(
                invoice_id = %invoice_id,
                queue_type = ?target_queue_type,
                "No queue found for tenant, invoice will not appear in workflow"
            );
            None
        }
        Err(e) => {
            tracing::warn!(
                invoice_id = %invoice_id,
                error = %e,
                "Failed to look up queue, invoice will not appear in workflow"
            );
            None
        }
    };

    let mut response_queue_id: Option<String> = None;
    if let Some(ref qid) = queue_id {
        // Create queue_items entry via move_item
        if let Err(e) = billforge_core::traits::WorkQueueRepository::move_item(
            &queue_repo,
            &tenant.tenant_id,
            &invoice_id,
            qid,
            None,
        ).await {
            tracing::warn!(
                invoice_id = %invoice_id,
                queue_id = %qid,
                error = %e,
                "Failed to create queue item"
            );
        }

        // Update invoice's current_queue_id (matching OCR-error pattern)
        sqlx::query(
            "UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2"
        )
        .bind(qid.0)
        .bind(invoice_id.0)
        .execute(&*pool)
        .await
        .ok();

        response_queue_id = Some(qid.to_string());
    }

    Ok(Json(serde_json::json!({
        "message": "Invoice submitted for processing",
        "invoice_id": id,
        "status": format!("{:?}", final_status).to_lowercase(),
        "queue_id": response_queue_id
    })))
}

#[derive(Deserialize)]
pub struct SuggestCategoriesRequest {
    pub vendor_id: Option<Uuid>,
    pub vendor_name: String,
    pub line_items: Vec<billforge_invoice_processing::categorization::LineItemInput>,
    pub total_amount: f64,
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/suggest-categories",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    request_body = String,
    responses(
        (status = 200, description = "Category suggestions returned"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn suggest_categories(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(_id): Path<String>,
    Json(req): Json<SuggestCategoriesRequest>,
) -> ApiResult<Json<billforge_invoice_processing::InvoiceCategorization>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let tenant_id_str = tenant.tenant_id.to_string();

    // Try ML-based categorization first if OpenAI API key is available
    let categorization = if let Ok(openai_api_key) = std::env::var("OPENAI_API_KEY") {
        // Use ML categorizer with fallback to rule-based engine
        let ml_categorizer = billforge_invoice_processing::MLCategorizer::new(
            (*pool).clone(),
            openai_api_key,
        );

        match ml_categorizer
            .suggest_categories_ml(
                &tenant_id_str,
                req.vendor_id,
                &req.vendor_name,
                &req.line_items,
                req.total_amount,
            )
            .await
        {
            Ok(ml_result) => {
                tracing::info!(
                    tenant_id = %tenant_id_str,
                    confidence = ml_result.overall_confidence,
                    source = ?ml_result.gl_code.as_ref().map(|g| &g.source),
                    "ML categorization succeeded"
                );
                ml_result
            }
            Err(e) => {
                tracing::warn!(
                    tenant_id = %tenant_id_str,
                    error = %e,
                    "ML categorization failed, falling back to rule-based engine"
                );
                // Fallback to rule-based categorization
                let engine = billforge_invoice_processing::CategorizationEngine::new((*pool).clone());
                engine
                    .suggest_categories(
                        &tenant_id_str,
                        req.vendor_id,
                        &req.vendor_name,
                        &req.line_items,
                        req.total_amount,
                    )
                    .await
                    .map_err(|e| billforge_core::Error::Database(format!("Categorization failed: {}", e)))?
            }
        }
    } else {
        // No OpenAI API key - use rule-based engine only
        tracing::info!(
            tenant_id = %tenant_id_str,
            "OpenAI API key not configured, using rule-based categorization"
        );
        let engine = billforge_invoice_processing::CategorizationEngine::new((*pool).clone());
        engine
            .suggest_categories(
                &tenant_id_str,
                req.vendor_id,
                &req.vendor_name,
                &req.line_items,
                req.total_amount,
            )
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Categorization failed: {}", e)))?
    };

    Ok(Json(categorization))
}

/// Build response message based on sync OCR result status.
fn sync_ocr_message(file_name: &str, status: CaptureStatus) -> String {
    if status == CaptureStatus::Failed {
        format!("File '{}' uploaded. OCR failed - invoice sent to error queue for manual review.", file_name)
    } else {
        format!("File '{}' uploaded and processed. Invoice ready for review.", file_name)
    }
}

/// Convert OCR-extracted line items into CreateLineItemInput values.
#[allow(dead_code)]
fn ocr_line_items_to_input(items: &[ExtractedLineItem]) -> Vec<CreateLineItemInput> {
    items
        .iter()
        .map(|item| CreateLineItemInput {
            description: item.description.value.clone().unwrap_or_default(),
            quantity: item.quantity.value,
            unit_price: item.unit_price.value.map(Money::usd),
            amount: Money::usd(item.amount.value.unwrap_or(0.0)),
            gl_code: None,
            department: None,
            project: None,
        })
        .collect()
}
