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
    domain::{CreateInvoiceInput, CreateLineItemInput, ExtractedLineItem, Invoice, InvoiceFilters, CaptureStatus, ProcessingStatus, QueueType},
    traits::InvoiceRepository,
    types::{Money, PaginatedResponse, Pagination},
};
use billforge_invoice_capture::ocr;
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
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let invoice = repo.create(&tenant.tenant_id, input, &user.user_id).await?;

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
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(updates): Json<serde_json::Value>,
) -> ApiResult<Json<Invoice>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let invoice = repo.update(&tenant.tenant_id, &invoice_id, updates).await?;

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
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    repo.delete(&tenant.tenant_id, &invoice_id).await?;

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
        (status = 200, description = "Invoice uploaded and processed", body = UploadResponse),
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

            // Run OCR on the document (use configured provider)
            let ocr_provider = ocr::create_provider(&state.config.ocr_provider);
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

            // Extract structured OCR fields (line items, subtotal, tax, currency)
            let (line_items, subtotal, tax_amount, currency) = match &ocr_result {
                Ok(result) => (
                    ocr_line_items_to_input(&result.line_items),
                    result.subtotal.value.map(Money::usd),
                    result.tax_amount.value.map(Money::usd),
                    result.currency.value.clone().unwrap_or_else(|| "USD".to_string()),
                ),
                Err(_) => (vec![], None, None, "USD".to_string()),
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
                subtotal,
                tax_amount,
                total_amount,
                currency,
                line_items,
                document_id,
                ocr_confidence,
                notes,
                tags: vec![],
                department: None,
                gl_code: None,
                cost_center: None,
            };

            let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
            let invoice = repo.create(&tenant.tenant_id, invoice_input, &user.user_id).await?;

            // Update capture status
            repo.update_capture_status(&tenant.tenant_id, &invoice.id, capture_status).await?;

            // If OCR failed, move to error queue
            if capture_status == CaptureStatus::Failed {
                let pool = state.db.tenant(&tenant.tenant_id).await?;
                let queue_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

                match billforge_core::traits::WorkQueueRepository::get_by_type(
                    &queue_repo,
                    &tenant.tenant_id,
                    QueueType::OcrError,
                ).await {
                    Ok(Some(error_queue)) => {
                        if let Err(e) = billforge_core::traits::WorkQueueRepository::move_item(
                            &queue_repo,
                            &tenant.tenant_id,
                            &invoice.id,
                            &error_queue.id,
                            None,
                        ).await {
                            tracing::warn!(
                                invoice_id = %invoice.id,
                                queue_id = %error_queue.id,
                                error = %e,
                                "Failed to create OCR error queue item"
                            );
                        }

                        if let Err(e) = sqlx::query(
                            "UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2"
                        )
                        .bind(error_queue.id.0)
                        .bind(invoice.id.as_uuid())
                        .execute(&*pool)
                        .await
                        {
                            tracing::warn!(
                                invoice_id = %invoice.id,
                                error = %e,
                                "Failed to update invoice current_queue_id"
                            );
                        }
                    }
                    Ok(None) => {
                        tracing::error!(
                            invoice_id = %invoice.id,
                            "No OcrError queue found for tenant, invoice will not appear in any queue"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            invoice_id = %invoice.id,
                            error = %e,
                            "Failed to look up OcrError queue, invoice will not appear in any queue"
                        );
                    }
                }
            }

            // Store document metadata in the tenant database
            let pool = state.db.tenant(&tenant.tenant_id).await?;

            sqlx::query(
                "INSERT INTO documents (id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'invoice_original', $8, NOW())"
            )
            .bind(document_id)
            .bind(*tenant.tenant_id.as_uuid())
            .bind(file_name)
            .bind(content_type)
            .bind(data.len() as i64)
            .bind(storage_key)
            .bind(invoice.id.as_uuid())
            .bind(user.user_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to store document metadata: {}", e)))?;

            let message = if capture_status == CaptureStatus::Failed {
                format!("File '{}' uploaded. OCR failed - invoice sent to error queue for manual review.", file_name_for_msg)
            } else {
                format!("File '{}' uploaded and processed. Invoice ready for review.", file_name_for_msg)
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

#[utoipa::path(
    post,
    path = "/invoices/{id}/ocr",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "OCR reprocessing completed"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn rerun_ocr(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Create the invoice capture service
    let invoice_repo = std::sync::Arc::new(billforge_db::repositories::InvoiceRepositoryImpl::new(pool));
    let capture_service = billforge_invoice_capture::InvoiceCaptureService::new(
        &state.config.ocr_provider,
        invoice_repo,
        state.storage.clone(),
    );

    // Run OCR reprocessing
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
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
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

/// Convert OCR-extracted line items into CreateLineItemInput values.
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
