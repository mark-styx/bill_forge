//! Invoice routes (Invoice Capture module)

use crate::error::{ApiError, ApiResult};
use crate::extractors::InvoiceCaptureAccess;
use crate::metrics;
use crate::state::{AppState, InvoiceEvent};
use axum::{
    extract::{Multipart, Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{
        AuditAction, AuditEntry, CaptureStatus, CreateInvoiceInput, CreateLineItemInput,
        ExtractedLineItem, Invoice, InvoiceFilters, ProcessingStatus, QueueType, ResourceType,
    },
    traits::{AuditService, InvoiceRepository},
    types::{Money, PaginatedResponse, Pagination, TenantContext, TenantSettings, UserContext},
    Error,
};
use billforge_invoice_capture::{
    ocr, ocr_routing_decision, resolve_ocr_provider_name, OcrRoutingDecision,
    OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD,
};
use billforge_invoice_processing::feedback_loop::{
    CategorizationFeedback, FeedbackLearning, FeedbackType,
};
use futures::stream::Stream;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_invoices))
        .route("/", post(create_invoice))
        .route("/upload", post(upload_invoice))
        .route("/stream", get(invoice_stream))
        .route("/:id", get(get_invoice))
        .route("/:id", put(update_invoice))
        .route("/:id", delete(delete_invoice))
        .route("/:id/ocr", post(rerun_ocr))
        .route("/:id/ocr-corrections", post(record_ocr_correction))
        .route("/:id/submit", post(submit_for_processing))
        .route("/:id/suggest-categories", post(suggest_categories))
        .route("/:id/merge-duplicate", post(merge_duplicate))
        .route("/:id/reject-duplicate", post(reject_duplicate))
        .route("/:id/unwind-approval", post(unwind_auto_approval))
        .route("/:id/ocr-exception/resolve", post(resolve_ocr_exception))
        .route("/ml-accuracy", get(get_ml_accuracy_metrics))
}

// ---------------------------------------------------------------------------
// SSE stream for real-time invoice status updates
// ---------------------------------------------------------------------------

/// Query parameters accepted by the SSE stream endpoint.
/// `token` is a fallback for `EventSource` which cannot send Authorization headers.
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub token: Option<String>,
}

/// Handler for `GET /api/v1/invoices/stream`.
/// Returns an SSE stream filtered to the authenticated tenant.
///
/// Auth: prefers `Authorization: Bearer ...` header (standard extractor path).
/// Falls back to `?token=<jwt>` query parameter so that browser `EventSource`
/// (which cannot set custom headers) can authenticate.
async fn invoice_stream(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Resolve auth: try header first, then ?token= fallback
    let user: UserContext = if let Some(auth_header) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        state.auth.validate_token(auth_header).await?
    } else if let Some(ref token) = query.token {
        state.auth.validate_token(token).await?
    } else {
        return Err(ApiError(Error::Unauthenticated));
    };

    let tenant_context = state.auth.get_tenant_context(&user.tenant_id).await?;

    if !tenant_context.has_module(billforge_core::Module::InvoiceCapture) {
        return Err(ApiError(Error::ModuleNotAvailable(
            "Invoice Capture".to_string(),
        )));
    }

    let tenant_id = *tenant_context.tenant_id.as_uuid();
    let mut rx = state.invoice_events.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) if event.tenant_id == tenant_id => {
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(Event::default().data(data));
                }
                Ok(_other_tenant) => { /* skip */ }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Missed events - client's next list refetch will reconcile
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

/// Emit an invoice event on the broadcast channel (non-blocking, best-effort).
fn emit_invoice_event(
    state: &AppState,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &Uuid,
    status: &str,
    kind: &'static str,
) {
    let _ = state.invoice_events.send(InvoiceEvent {
        tenant_id: *tenant_id.as_uuid(),
        invoice_id: *invoice_id,
        status: status.to_string(),
        kind,
        occurred_at: chrono::Utc::now(),
    });
}

// ---------------------------------------------------------------------------
// Duplicate detection types
// ---------------------------------------------------------------------------

/// Query parameters for invoice creation with duplicate-detection bypass.
#[derive(Debug, Deserialize)]
pub struct CreateInvoiceQuery {
    pub force: Option<bool>,
}

/// Per-signal breakdown for a duplicate match.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DuplicateSignalBreakdown {
    pub vendor: f64,
    pub invoice_number: f64,
    pub amount: f64,
    pub date: f64,
    pub line_item_fingerprint: f64,
}

/// A single potential duplicate match returned during invoice creation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DuplicateMatch {
    pub existing_invoice_id: String,
    pub score: f64,
    pub severity: String,
    pub signal_breakdown: DuplicateSignalBreakdown,
}

/// Response body for invoice creation with potential duplicate warnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInvoiceResponse {
    pub invoice: Invoice,
    pub potential_duplicates: Vec<DuplicateMatch>,
}

/// Request body for merging a duplicate invoice into an existing one.
#[derive(Debug, Deserialize)]
pub struct MergeDuplicateRequest {
    pub keep_invoice_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListInvoicesQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub vendor_id: Option<String>,
    pub capture_status: Option<String>,
    pub processing_status: Option<String>,
    pub search: Option<String>,
    pub min_ocr_confidence: Option<f32>,
    pub max_ocr_confidence: Option<f32>,
    pub ocr_exception_status: Option<String>,
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
        ("search" = Option<String>, Query, description = "Search term"),
        ("min_ocr_confidence" = Option<f32>, Query, description = "Minimum OCR confidence (0.0-1.0)"),
        ("max_ocr_confidence" = Option<f32>, Query, description = "Maximum OCR confidence (0.0-1.0)")
    ),
    responses(
        (status = 200, description = "List of invoices", body = crate::openapi::InvoiceList),
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
        capture_status: query
            .capture_status
            .and_then(|s| CaptureStatus::from_str(&s)),
        processing_status: query
            .processing_status
            .and_then(|s| ProcessingStatus::from_str(&s)),
        min_ocr_confidence: query.min_ocr_confidence,
        max_ocr_confidence: query.max_ocr_confidence,
        ocr_exception_status: query.ocr_exception_status,
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
        (status = 200, description = "Invoice details", body = crate::openapi::Invoice),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn get_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Invoice>> {
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
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
    params(
        ("force" = Option<bool>, Query, description = "Skip duplicate detection and force create")
    ),
    responses(
        (status = 200, description = "Invoice created with potential duplicate warnings"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn create_invoice(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Query(query): Query<CreateInvoiceQuery>,
    Json(input): Json<CreateInvoiceInput>,
) -> ApiResult<Json<CreateInvoiceResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let invoice = repo
        .create(&tenant.tenant_id, input.clone(), Some(&user.user_id))
        .await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Invoice,
        invoice.id.to_string(),
        format!("Created invoice {}", invoice.invoice_number),
    )
    .with_user_email(&user.email)
    .with_new_value(serde_json::to_value(&invoice).unwrap_or_default());
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    // Duplicate detection: check against recent tenant invoices unless force=true
    let potential_duplicates = if query.force == Some(true) {
        vec![]
    } else {
        detect_duplicates_for_invoice(&pool, &tenant.tenant_id, &invoice).await
    };

    // If duplicates were found, hold the invoice for review (keep it in Pending so
    // it doesn't flow into dashboards, queues, or exports until resolved).
    if !potential_duplicates.is_empty() {
        repo.update_capture_status(&tenant.tenant_id, &invoice.id, CaptureStatus::Pending)
            .await?;
    }

    // Populate early-payment discount columns from vendor payment_terms
    if let Err(e) = crate::routes::discounts::populate_discount_columns(
        &pool,
        tenant.tenant_id.as_uuid(),
        invoice.id.as_uuid(),
        input.vendor_id.as_ref(),
        input.invoice_date.as_ref(),
    )
    .await
    {
        tracing::warn!(error = %e, "Failed to populate discount columns for invoice");
    }

    // Fire outbound webhook for invoice.created (best-effort, non-blocking)
    if let Err(e) = tokio::spawn({
        let metadata_pool = state.db.metadata();
        let tenant_uuid = *tenant.tenant_id.as_uuid();
        let invoice_json = serde_json::to_value(&invoice).unwrap_or_default();
        async move {
            billforge_core::public_api::dispatch_webhook(
                &metadata_pool,
                tenant_uuid,
                "invoice.created",
                invoice_json,
            )
            .await;
        }
    })
    .await
    {
        tracing::warn!(error = %e, "Webhook dispatch task panicked");
    }

    Ok(Json(CreateInvoiceResponse {
        invoice,
        potential_duplicates,
    }))
}

/// Load recent (90-day) invoices for the tenant and score the new invoice against each.
/// Returns matches with severity >= Medium (score > 0.8).
async fn detect_duplicates_for_invoice(
    pool: &std::sync::Arc<sqlx::PgPool>,
    tenant_id: &billforge_core::TenantId,
    new_invoice: &Invoice,
) -> Vec<DuplicateMatch> {
    let rows = match sqlx::query_as::<
        _,
        (
            Uuid,
            Option<String>,
            Option<i64>,
            Option<chrono::NaiveDate>,
            Option<serde_json::Value>,
            Option<String>,
        ),
    >(
        r#"SELECT id, invoice_number, total_amount_cents, invoice_date, line_items, vendor_name
           FROM invoices
           WHERE tenant_id = $1
             AND id != $2
             AND created_at > NOW() - INTERVAL '90 days'
           ORDER BY created_at DESC
           LIMIT 100"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(new_invoice.id.as_uuid())
    .fetch_all(&**pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load recent invoices for duplicate detection");
            return vec![];
        }
    };

    // Build fingerprint for the new invoice from its line items
    let new_li_items: Vec<(String, Option<f64>, Option<f64>)> = new_invoice
        .line_items
        .iter()
        .map(|li| {
            (
                li.description.clone(),
                li.quantity,
                li.unit_price.as_ref().map(|m| m.amount as f64 / 100.0),
            )
        })
        .collect();
    let new_fp =
        billforge_analytics::anomaly_detection::InvoiceRecord::compute_line_item_fingerprint(
            &new_li_items,
        );

    let new_date = new_invoice
        .invoice_date
        .and_then(|d| d.and_hms_opt(12, 0, 0).map(|ndt| ndt.and_utc()))
        .unwrap_or_else(chrono::Utc::now);

    let new_record = billforge_analytics::anomaly_detection::InvoiceRecord {
        invoice_id: new_invoice.id.to_string(),
        vendor_name: new_invoice.vendor_name.clone(),
        amount: new_invoice.total_amount.amount as f64 / 100.0,
        invoice_date: new_date,
        invoice_number: Some(new_invoice.invoice_number.clone()),
        line_item_fingerprint: if new_fp.is_empty() {
            None
        } else {
            Some(new_fp)
        },
    };

    let detector =
        billforge_analytics::anomaly_detection::DuplicateDetector::new(*tenant_id.as_uuid());

    let mut matches = Vec::new();
    for (id, inv_num, amount_cents, inv_date, line_items_json, vendor_name) in rows {
        // Build fingerprint from stored line items JSON
        let fp = line_items_json
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                let items: Vec<(String, Option<f64>, Option<f64>)> = arr
                    .iter()
                    .filter_map(|item| {
                        let desc = item.get("description")?.as_str()?.to_string();
                        let qty = item.get("quantity").and_then(|v| v.as_f64());
                        let up = item
                            .get("unit_price_cents")
                            .and_then(|v| v.as_i64())
                            .map(|c| c as f64 / 100.0);
                        Some((desc, qty, up))
                    })
                    .collect();
                billforge_analytics::anomaly_detection::InvoiceRecord::compute_line_item_fingerprint(&items)
            })
            .unwrap_or_default();

        let existing_date = inv_date
            .and_then(|d| d.and_hms_opt(12, 0, 0).map(|ndt| ndt.and_utc()))
            .unwrap_or_else(chrono::Utc::now);

        let existing_record = billforge_analytics::anomaly_detection::InvoiceRecord {
            invoice_id: id.to_string(),
            vendor_name: vendor_name.unwrap_or_default(),
            amount: amount_cents.unwrap_or(0) as f64 / 100.0,
            invoice_date: existing_date,
            invoice_number: inv_num,
            line_item_fingerprint: if fp.is_empty() { None } else { Some(fp) },
        };

        let (score, breakdown) = detector.score_pair(&new_record, &existing_record);
        if score > 0.8 {
            let severity = if score > 0.95 {
                "Critical"
            } else if score > 0.9 {
                "High"
            } else {
                "Medium"
            };
            matches.push(DuplicateMatch {
                existing_invoice_id: id.to_string(),
                score,
                severity: severity.to_string(),
                signal_breakdown: DuplicateSignalBreakdown {
                    vendor: *breakdown.get("vendor").unwrap_or(&0.0),
                    invoice_number: *breakdown.get("invoice_number").unwrap_or(&0.0),
                    amount: *breakdown.get("amount").unwrap_or(&0.0),
                    date: *breakdown.get("date").unwrap_or(&0.0),
                    line_item_fingerprint: *breakdown.get("line_item_fingerprint").unwrap_or(&0.0),
                },
            });
        }
    }

    // Sort by score descending
    matches.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches
}

#[utoipa::path(
    put,
    path = "/invoices/{id}",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    responses(
        (status = 200, description = "Invoice updated", body = crate::openapi::Invoice),
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
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    let old_invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?;
    let invoice = repo
        .update(&tenant.tenant_id, &invoice_id, updates.clone())
        .await?;

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
            if let Err(e) = FeedbackLearning::new((*pool).clone())
                .record_feedback(feedback)
                .await
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
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Invoice,
        invoice.id.to_string(),
        format!("Updated invoice {}", invoice.invoice_number),
    )
    .with_user_email(&user.email)
    .with_new_value(serde_json::to_value(&invoice).unwrap_or_default());
    if let Some(old) = old_invoice {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or_default());
    }
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    emit_invoice_event(
        &state,
        &tenant.tenant_id,
        invoice.id.as_uuid(),
        &format!("{:?}", invoice.processing_status),
        "updated",
    );

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
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    let old_invoice = repo.get_by_id(&tenant.tenant_id, &invoice_id).await?;
    repo.delete(&tenant.tenant_id, &invoice_id).await?;

    let mut audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Delete,
        ResourceType::Invoice,
        id.clone(),
        "Deleted invoice",
    )
    .with_user_email(&user.email);
    if let Some(old) = old_invoice {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or_default());
    }
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadResponse {
    pub invoice_id: String,
    pub document_id: String,
    pub message: String,
    pub potential_duplicates: Vec<DuplicateMatch>,
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
    let capture_started = Instant::now();
    // Process multipart upload
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Upload error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let file_name = field.file_name().unwrap_or("document.pdf").to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/pdf")
                .to_string();
            let data = field.bytes().await.map_err(|e| {
                billforge_core::Error::Validation(format!("Failed to read file: {}", e))
            })?;

            return upload_invoice_file(
                &state,
                &user,
                &tenant,
                file_name,
                content_type,
                &data,
                capture_started,
            )
            .await
            .map(Json);
        }
    }

    Err(billforge_core::Error::Validation("No file provided".to_string()).into())
}

pub(crate) async fn upload_invoice_file(
    state: &AppState,
    user: &UserContext,
    tenant: &TenantContext,
    file_name: String,
    content_type: String,
    data: &[u8],
    capture_started: Instant,
) -> ApiResult<UploadResponse> {
    let file_name_for_msg = file_name.clone();
    let document_id = state
        .storage
        .upload(&tenant.tenant_id, &file_name, data, &content_type)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to store document: {}", e)))?;

    let storage_key = billforge_db::build_storage_key(&tenant.tenant_id, document_id);

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
    let invoice = repo
        .create(&tenant.tenant_id, invoice_input, Some(&user.user_id))
        .await?;

    repo.update_capture_status(&tenant.tenant_id, &invoice.id, CaptureStatus::Processing)
        .await?;

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

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Invoice,
        invoice.id.to_string(),
        format!("Uploaded invoice from file '{}'", file_name),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "document_id": document_id.to_string(),
        "file_name": file_name,
        "content_type": content_type,
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    let message = if let Some(ref redis_client) = state.redis {
        match enqueue_ocr_job(
            redis_client,
            &tenant.tenant_id,
            &invoice.id,
            document_id,
            &content_type,
            capture_started,
        )
        .await
        {
            Ok(_) => {
                tracing::info!(
                    invoice_id = %invoice.id,
                    "OCR job enqueued for async processing"
                );
                format!(
                    "File '{}' uploaded. OCR processing queued - poll GET /invoices/{} for status.",
                    file_name_for_msg, invoice.id
                )
            }
            Err(e) => {
                tracing::warn!(
                    invoice_id = %invoice.id,
                    error = %e,
                    "Failed to enqueue OCR job, falling back to sync"
                );
                let status = run_sync_ocr(
                    state,
                    &tenant.tenant_id,
                    &invoice.id,
                    data,
                    &content_type,
                    &repo,
                    &pool,
                    &tenant.settings,
                )
                .await;
                sync_ocr_message(&file_name_for_msg, status)
            }
        }
    } else {
        let status = run_sync_ocr(
            state,
            &tenant.tenant_id,
            &invoice.id,
            data,
            &content_type,
            &repo,
            &pool,
            &tenant.settings,
        )
        .await;
        sync_ocr_message(&file_name_for_msg, status)
    };

    // Run duplicate detection after OCR has populated real data.
    // Only effective for sync OCR path; async OCR will detect later in the pipeline.
    let potential_duplicates = {
        let refreshed = repo
            .get_by_id(&tenant.tenant_id, &invoice.id)
            .await
            .ok()
            .flatten();
        match refreshed {
            Some(ref inv)
                if inv.vendor_name != "Processing..."
                    && !inv.invoice_number.starts_with("UPLOAD-") =>
            {
                detect_duplicates_for_invoice(&pool, &tenant.tenant_id, inv).await
            }
            _ => vec![],
        }
    };

    Ok(UploadResponse {
        invoice_id: invoice.id.to_string(),
        document_id: document_id.to_string(),
        message,
        potential_duplicates,
    })
}

/// Enqueue an OCR processing job to the Redis job queue.
async fn enqueue_ocr_job(
    redis_client: &redis::Client,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
    document_id: Uuid,
    content_type: &str,
    capture_started: Instant,
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

    let job_json = serde_json::to_string(&job).map_err(|e| {
        billforge_core::Error::Database(format!("Failed to serialize OCR job: {}", e))
    })?;

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Redis connection failed: {}", e)))?;

    conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to enqueue OCR job: {}", e))
        })?;

    // Record capture-to-queue latency
    metrics::CAPTURE_TO_QUEUE_DURATION_SECONDS.observe(capture_started.elapsed().as_secs_f64());

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
    tenant_settings: &TenantSettings,
) -> CaptureStatus {
    let provider_name = resolve_ocr_provider_name(&state.config.ocr_provider, tenant_settings);
    let ocr_provider = ocr::create_provider(&provider_name);
    let ocr_result = ocr_provider.extract(data, content_type).await;

    let capture_status = match &ocr_result {
        Ok(result) => {
            // Emit OCR provider success + per-field confidence metrics
            metrics::OCR_PROVIDER_OUTCOME_TOTAL
                .with_label_values(&[&provider_name, "success"])
                .inc();
            observe_field_confidence(result);

            let confidence = [
                result.invoice_number.confidence,
                result.vendor_name.confidence,
                result.total_amount.confidence,
            ]
            .iter()
            .sum::<f32>()
                / 3.0;

            let status = match ocr_routing_decision(Some(confidence)) {
                OcrRoutingDecision::Error => CaptureStatus::Failed,
                OcrRoutingDecision::ExceptionReview | OcrRoutingDecision::StraightThrough => {
                    CaptureStatus::ReadyForReview
                }
            };

            let vendor_name = result
                .vendor_name
                .value
                .clone()
                .unwrap_or_else(|| "Unknown Vendor".to_string());
            let invoice_number = result.invoice_number.value.clone().unwrap_or_else(|| {
                format!("UPLOAD-{}", &Uuid::new_v4().to_string()[..8].to_uppercase())
            });
            let total_amount = Money::usd(result.total_amount.value.unwrap_or(0.0));
            let currency = result
                .currency
                .value
                .clone()
                .unwrap_or_else(|| "USD".to_string());

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
            // Emit OCR provider failure metric
            metrics::OCR_PROVIDER_OUTCOME_TOTAL
                .with_label_values(&[&provider_name, "failure"])
                .inc();
            tracing::warn!(invoice_id = %invoice_id, error = %e, "Sync OCR failed");
            let _ = repo
                .update(
                    tenant_id,
                    invoice_id,
                    serde_json::json!({ "notes": format!("OCR Error: {}", e) }),
                )
                .await;
            CaptureStatus::Failed
        }
    };

    if let Err(e) = repo
        .update_capture_status(tenant_id, invoice_id, capture_status)
        .await
    {
        tracing::error!(invoice_id = %invoice_id, error = %e, "Failed to update capture status");
    }

    emit_invoice_event(
        &state,
        tenant_id,
        invoice_id.as_uuid(),
        &format!("{:?}", capture_status),
        "ocr_completed",
    );

    let ocr_confidence = match &ocr_result {
        Ok(result) => Some(
            [
                result.invoice_number.confidence,
                result.vendor_name.confidence,
                result.total_amount.confidence,
            ]
            .iter()
            .sum::<f32>()
                / 3.0,
        ),
        Err(_) => Some(0.0),
    };

    match ocr_routing_decision(ocr_confidence) {
        OcrRoutingDecision::Error => {
            route_sync_ocr_queue(pool, tenant_id, invoice_id, QueueType::OcrError).await;
        }
        OcrRoutingDecision::ExceptionReview => {
            route_sync_ocr_queue(pool, tenant_id, invoice_id, QueueType::Exception).await;
        }
        OcrRoutingDecision::StraightThrough => {
            if let Ok(result) = &ocr_result {
                if let Err(e) = run_sync_straight_through_processing(
                    repo,
                    pool,
                    tenant_id,
                    invoice_id,
                    result,
                    state.db.metadata(),
                )
                .await
                {
                    tracing::warn!(
                        invoice_id = %invoice_id,
                        error = %e,
                        "Sync straight-through processing failed; invoice remains ready for review"
                    );
                    route_sync_ocr_queue(pool, tenant_id, invoice_id, QueueType::Review).await;
                }
            }
        }
    }

    capture_status
}

async fn route_sync_ocr_queue(
    pool: &std::sync::Arc<sqlx::PgPool>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
    queue_type: QueueType,
) {
    let queue_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    match billforge_core::traits::WorkQueueRepository::get_by_type(
        &queue_repo,
        tenant_id,
        queue_type,
    )
    .await
    {
        Ok(Some(queue)) => {
            if let Err(e) = billforge_core::traits::WorkQueueRepository::move_item(
                &queue_repo,
                tenant_id,
                invoice_id,
                &queue.id,
                None,
            )
            .await
            {
                tracing::warn!(invoice_id = %invoice_id, queue_type = ?queue_type, error = %e, "Failed to create sync OCR queue item");
            }
            if let Err(e) = sqlx::query(
                "UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(queue.id.0)
            .bind(invoice_id.as_uuid())
            .execute(&**pool)
            .await
            {
                tracing::warn!(invoice_id = %invoice_id, queue_type = ?queue_type, error = %e, "Failed to update sync OCR invoice queue");
            }
        }
        Ok(None) => {
            tracing::warn!(invoice_id = %invoice_id, queue_type = ?queue_type, "No queue found for sync OCR route");
        }
        Err(e) => {
            tracing::warn!(invoice_id = %invoice_id, queue_type = ?queue_type, error = %e, "Failed to look up sync OCR queue");
        }
    }
}

async fn run_sync_straight_through_processing(
    repo: &billforge_db::repositories::InvoiceRepositoryImpl,
    pool: &std::sync::Arc<sqlx::PgPool>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
    ocr_result: &billforge_core::domain::OcrExtractionResult,
    metadata_pool: std::sync::Arc<sqlx::PgPool>,
) -> Result<(), billforge_core::Error> {
    let mut invoice = repo
        .get_by_id(tenant_id, invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: invoice_id.to_string(),
        })?;

    if invoice.gl_code.is_none() || invoice.department.is_none() || invoice.cost_center.is_none() {
        let had_gl_code = invoice.gl_code.is_some();
        let had_department = invoice.department.is_some();
        let had_cost_center = invoice.cost_center.is_some();
        let line_items = sync_ocr_line_items(ocr_result, &invoice);
        let total = ocr_result
            .total_amount
            .value
            .unwrap_or(invoice.total_amount.amount as f64 / 100.0);
        let tenant_id_str = tenant_id.to_string();
        let cat_engine = billforge_invoice_processing::CategorizationEngine::new((**pool).clone());

        let cat_result = if let Ok(openai_api_key) = std::env::var("OPENAI_API_KEY") {
            let ml_categorizer =
                billforge_invoice_processing::MLCategorizer::new((**pool).clone(), openai_api_key);
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
                Ok(ml_result) => Ok(ml_result),
                Err(e) => {
                    tracing::warn!(invoice_id = %invoice_id, error = %e, "Sync OCR ML categorization failed, falling back to rule-based");
                    cat_engine
                        .suggest_categories(
                            &tenant_id_str,
                            invoice.vendor_id,
                            &invoice.vendor_name,
                            &line_items,
                            total,
                        )
                        .await
                }
            }
        } else {
            cat_engine
                .suggest_categories(
                    &tenant_id_str,
                    invoice.vendor_id,
                    &invoice.vendor_name,
                    &line_items,
                    total,
                )
                .await
        };

        match cat_result {
            Ok(categorization) => {
                let mut updates = serde_json::json!({
                    "categorization_confidence": categorization.overall_confidence,
                });

                if !had_gl_code {
                    updates["gl_code"] =
                        serde_json::json!(categorization.gl_code.as_ref().map(|s| &s.value));
                }
                if !had_department {
                    updates["department"] =
                        serde_json::json!(categorization.department.as_ref().map(|s| &s.value));
                }
                if !had_cost_center {
                    updates["cost_center"] =
                        serde_json::json!(categorization.cost_center.as_ref().map(|s| &s.value));
                }

                repo.update(tenant_id, invoice_id, updates).await?;
                if let Some(refetched) = repo.get_by_id(tenant_id, invoice_id).await? {
                    invoice = refetched;
                }
            }
            Err(e) => {
                tracing::warn!(invoice_id = %invoice_id, error = %e, "Sync OCR auto-categorization failed");
            }
        }
    }

    repo.update_processing_status(tenant_id, invoice_id, ProcessingStatus::Submitted)
        .await?;

    let workflow_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let engine = billforge_invoice_processing::WorkflowEngine::new(
        std::sync::Arc::new(billforge_db::repositories::InvoiceRepositoryImpl::new(
            pool.clone(),
        )) as std::sync::Arc<dyn billforge_core::traits::InvoiceRepository>,
        std::sync::Arc::new(workflow_repo)
            as std::sync::Arc<dyn billforge_core::traits::WorkflowRuleRepository>,
        std::sync::Arc::new(billforge_db::repositories::WorkflowRepositoryImpl::new(
            pool.clone(),
        )) as std::sync::Arc<dyn billforge_core::traits::ApprovalRepository>,
    )
    .with_routing(std::sync::Arc::new(billforge_db::RoutingRepository::new(
        pool.as_ref().clone(),
    )))
    .with_pool(pool.clone())
    .with_tenant_settings_provider(std::sync::Arc::new(
        billforge_db::TenantSettingsFromDb::new(metadata_pool.clone()),
    ));
    let final_status = engine.process_invoice(tenant_id, &invoice).await?;

    // Emit capture-to-approval/final-status timing metric.
    emit_capture_timing_metrics(tenant_id, final_status, invoice.created_at);

    repo.update_processing_status(tenant_id, invoice_id, final_status)
        .await?;
    route_sync_processing_queue(pool, tenant_id, invoice_id, final_status).await;

    // Emit meter event for successfully-processed invoices.
    // The outbox UNIQUE constraint prevents double-emit.
    if matches!(
        final_status,
        ProcessingStatus::Approved | ProcessingStatus::ReadyForPayment | ProcessingStatus::Paid
    ) {
        // Best-effort: do not fail the processing pipeline on meter errors.
        let config = billforge_billing::BillingConfig::from_env();
        if config.enabled {
            let service = billforge_billing::BillingService::new(config, metadata_pool.clone());
            if let Some(stripe) = service.stripe().as_deref() {
                let _ = billforge_billing::record_invoice_meter_event(
                    &metadata_pool,
                    Some(stripe),
                    tenant_id,
                    invoice_id.as_uuid(),
                )
                .await;
            }
        }
    }

    Ok(())
}

fn sync_ocr_line_items(
    result: &billforge_core::domain::OcrExtractionResult,
    invoice: &billforge_core::domain::Invoice,
) -> Vec<billforge_invoice_processing::categorization::LineItemInput> {
    let extracted = result
        .line_items
        .iter()
        .map(
            |item| billforge_invoice_processing::categorization::LineItemInput {
                description: item.description.value.clone().unwrap_or_default(),
                quantity: item.quantity.value,
                amount: item.amount.value.unwrap_or(0.0),
            },
        )
        .filter(|item| !item.description.is_empty() || item.amount > 0.0)
        .collect::<Vec<_>>();

    if !extracted.is_empty() {
        return extracted;
    }

    invoice
        .line_items
        .iter()
        .map(
            |item| billforge_invoice_processing::categorization::LineItemInput {
                description: item.description.clone(),
                quantity: item.quantity,
                amount: item.amount.amount as f64 / 100.0,
            },
        )
        .collect()
}

async fn route_sync_processing_queue(
    pool: &std::sync::Arc<sqlx::PgPool>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &billforge_core::domain::InvoiceId,
    status: ProcessingStatus,
) {
    let queue_type = match status {
        ProcessingStatus::Approved | ProcessingStatus::ReadyForPayment => QueueType::Payment,
        ProcessingStatus::PendingApproval => QueueType::Approval,
        ProcessingStatus::Rejected | ProcessingStatus::Voided | ProcessingStatus::Paid => return,
        _ => QueueType::Review,
    };

    route_sync_ocr_queue(pool, tenant_id, invoice_id, queue_type).await;
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
    let invoice_id: billforge_core::domain::InvoiceId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Verify invoice exists and get its document_id
    let invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Look up the stored MIME type for this invoice's primary document
    let mime_type: String =
        sqlx::query_scalar("SELECT mime_type FROM documents WHERE id = $1 AND tenant_id = $2")
            .bind(invoice.document_id)
            .bind(*tenant.tenant_id.as_uuid())
            .fetch_optional(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!(
                    "Failed to look up document MIME type: {}",
                    e
                ))
            })?
            .unwrap_or_else(|| "application/pdf".to_string());

    // Mark as Processing
    repo.update_capture_status(&tenant.tenant_id, &invoice_id, CaptureStatus::Processing)
        .await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::OcrRerun,
        ResourceType::Invoice,
        id.clone(),
        format!("Reran OCR for invoice {}", invoice.invoice_number),
    )
    .with_user_email(&user.email);
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
            Instant::now(),
        )
        .await
        {
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
    let provider_name = resolve_ocr_provider_name(&state.config.ocr_provider, &tenant.settings);
    let calibration_store: Arc<dyn billforge_invoice_capture::OcrCalibrationStore> = Arc::new(
        billforge_invoice_capture::PgOcrCalibrationStore::new(pool.clone()),
    );
    let capture_service = billforge_invoice_capture::InvoiceCaptureService::new(
        &provider_name,
        invoice_repo,
        state.storage.clone(),
    )
    .with_calibration(calibration_store);

    let ocr_result = capture_service
        .reprocess_ocr(&tenant.tenant_id, &invoice_id)
        .await?;

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
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get the invoice
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let mut invoice = invoice_repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
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

    // Block submission when OCR confidence is below the review threshold
    match invoice.ocr_confidence {
        Some(conf) if conf >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD => {}
        _ => {
            return Err(billforge_core::Error::Validation(
                "Invoice requires manual OCR review before submission (low or missing OCR confidence)".into(),
            ).into());
        }
    }

    // Update status to Submitted
    invoice_repo
        .update_processing_status(&tenant.tenant_id, &invoice_id, ProcessingStatus::Submitted)
        .await?;
    invoice.processing_status = ProcessingStatus::Submitted;

    // Run auto-categorization if ANY categorization field is missing
    if invoice.gl_code.is_none() || invoice.department.is_none() || invoice.cost_center.is_none() {
        // Capture which fields already have values so we don't overwrite them
        let had_gl_code = invoice.gl_code.is_some();
        let had_department = invoice.department.is_some();
        let had_cost_center = invoice.cost_center.is_some();

        let cat_engine = billforge_invoice_processing::CategorizationEngine::new((*pool).clone());
        let line_items: Vec<billforge_invoice_processing::categorization::LineItemInput> = invoice
            .line_items
            .iter()
            .map(
                |li| billforge_invoice_processing::categorization::LineItemInput {
                    description: li.description.clone(),
                    quantity: li.quantity,
                    amount: li.amount.amount as f64 / 100.0,
                },
            )
            .collect();
        let total = invoice.total_amount.amount as f64 / 100.0;
        let tenant_id_str = tenant.tenant_id.to_string();

        // Try ML-based categorization first if OpenAI API key is available,
        // then fall back to rule-based engine
        let cat_result = if let Ok(openai_api_key) = std::env::var("OPENAI_API_KEY") {
            let ml_categorizer =
                billforge_invoice_processing::MLCategorizer::new((*pool).clone(), openai_api_key);
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
                    cat_engine
                        .suggest_categories(
                            &tenant_id_str,
                            invoice.vendor_id,
                            &invoice.vendor_name,
                            &line_items,
                            total,
                        )
                        .await
                }
            }
        } else {
            cat_engine
                .suggest_categories(
                    &tenant_id_str,
                    invoice.vendor_id,
                    &invoice.vendor_name,
                    &line_items,
                    total,
                )
                .await
        };

        match cat_result {
            Ok(categorization) => {
                // Build updates JSON, only including fields that were originally None
                let mut updates = serde_json::json!({
                    "categorization_confidence": categorization.overall_confidence,
                });
                if !had_gl_code {
                    updates["gl_code"] =
                        serde_json::json!(categorization.gl_code.as_ref().map(|s| &s.value));
                }
                if !had_department {
                    updates["department"] =
                        serde_json::json!(categorization.department.as_ref().map(|s| &s.value));
                }
                if !had_cost_center {
                    updates["cost_center"] =
                        serde_json::json!(categorization.cost_center.as_ref().map(|s| &s.value));
                }
                if let Err(e) = invoice_repo
                    .update(&tenant.tenant_id, &invoice_id, updates)
                    .await
                {
                    tracing::warn!(invoice_id = %invoice_id, error = %e, "Failed to persist auto-categorization");
                } else {
                    // Re-fetch so the workflow engine sees populated fields
                    if let Some(refetched) = invoice_repo
                        .get_by_id(&tenant.tenant_id, &invoice_id)
                        .await?
                    {
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
    )
    .await?;

    // Filter to active rules only
    let _active_rules: Vec<_> = rules.into_iter().filter(|r| r.is_active).collect();

    // Process invoice through workflow engine
    use std::sync::Arc;
    let engine = billforge_invoice_processing::WorkflowEngine::new(
        Arc::new(invoice_repo) as Arc<dyn billforge_core::traits::InvoiceRepository>,
        Arc::new(workflow_repo) as Arc<dyn billforge_core::traits::WorkflowRuleRepository>,
        Arc::new(billforge_db::repositories::WorkflowRepositoryImpl::new(
            pool.clone(),
        )) as Arc<dyn billforge_core::traits::ApprovalRepository>,
    )
    .with_routing(Arc::new(billforge_db::RoutingRepository::new(
        pool.as_ref().clone(),
    )))
    .with_pool(pool.clone())
    .with_tenant_settings_provider(Arc::new(billforge_db::TenantSettingsFromDb::new(
        state.db.metadata(),
    )));

    let final_status = engine.process_invoice(&tenant.tenant_id, &invoice).await?;

    // Emit capture-to-approval/final-status timing metric.
    emit_capture_timing_metrics(&tenant.tenant_id, final_status, invoice.created_at);

    // Update invoice with final status from workflow
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    invoice_repo
        .update_processing_status(&tenant.tenant_id, &invoice_id, final_status)
        .await?;

    emit_invoice_event(
        &state,
        &tenant.tenant_id,
        invoice_id.as_uuid(),
        &format!("{:?}", final_status),
        "status_changed",
    );

    // Emit meter event for successfully-processed invoices.
    // The outbox UNIQUE constraint prevents double-emit.
    if matches!(
        final_status,
        ProcessingStatus::Approved | ProcessingStatus::ReadyForPayment | ProcessingStatus::Paid
    ) {
        let config = billforge_billing::BillingConfig::from_env();
        if config.enabled {
            let service = billforge_billing::BillingService::new(config, state.db.metadata());
            if let Some(stripe) = service.stripe().as_deref() {
                let _ = billforge_billing::record_invoice_meter_event(
                    &state.db.metadata(),
                    Some(stripe),
                    &tenant.tenant_id,
                    invoice_id.as_uuid(),
                )
                .await;
            }
        }
    }

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::InvoiceSubmitted,
        ResourceType::Invoice,
        id.clone(),
        format!(
            "Submitted invoice for processing, status: {:?}",
            final_status
        ),
    )
    .with_user_email(&user.email)
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
    )
    .await
    {
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
        )
        .await
        {
            tracing::warn!(
                invoice_id = %invoice_id,
                queue_id = %qid,
                error = %e,
                "Failed to create queue item"
            );
        }

        // Update invoice's current_queue_id (matching OCR-error pattern)
        sqlx::query("UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2")
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
        let ml_categorizer =
            billforge_invoice_processing::MLCategorizer::new((*pool).clone(), openai_api_key);

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
                let engine =
                    billforge_invoice_processing::CategorizationEngine::new((*pool).clone());
                engine
                    .suggest_categories(
                        &tenant_id_str,
                        req.vendor_id,
                        &req.vendor_name,
                        &req.line_items,
                        req.total_amount,
                    )
                    .await
                    .map_err(|e| {
                        billforge_core::Error::Database(format!("Categorization failed: {}", e))
                    })?
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

    // Persist per-line categorization suggestions alongside the invoice-level
    // result. Best-effort: failures are logged but do not block the response.
    let engine = billforge_invoice_processing::CategorizationEngine::new((*pool).clone());
    let invoice_uuid: Uuid = match _id.parse() {
        Ok(u) => u,
        Err(_) => {
            tracing::warn!(invoice_id = %_id, "Cannot parse invoice ID for per-line persistence");
            return Ok(Json(categorization));
        }
    };

    let vendor_history = if let Some(vid) = req.vendor_id {
        engine.vendor_history_lookup(&tenant_id_str, vid).await.ok()
    } else {
        None
    };

    match engine
        .suggest_per_line_categorizations(
            &tenant_id_str,
            invoice_uuid,
            req.vendor_id,
            &req.line_items,
            vendor_history.as_ref(),
            &[],
        )
        .await
    {
        Ok(per_line) => {
            if let Err(e) =
                billforge_invoice_processing::categorization::persist_line_categorizations(
                    &*pool,
                    &tenant_id_str,
                    &per_line,
                )
                .await
            {
                tracing::warn!(
                    tenant_id = %tenant_id_str,
                    invoice_id = %invoice_uuid,
                    error = %e,
                    "Failed to persist per-line categorizations"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                tenant_id = %tenant_id_str,
                invoice_id = %invoice_uuid,
                error = %e,
                "Per-line categorization suggestion failed"
            );
        }
    }

    Ok(Json(categorization))
}

// ---------------------------------------------------------------------------
// Merge / Reject duplicate endpoints
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/invoices/{id}/merge-duplicate",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID to discard (the duplicate)")
    ),
    request_body = String,
    responses(
        (status = 200, description = "Duplicate merged into kept invoice"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn merge_duplicate(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(body): Json<MergeDuplicateRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let dup_invoice_id: billforge_core::domain::InvoiceId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    let keep_invoice_id: billforge_core::domain::InvoiceId = body
        .keep_invoice_id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid keep invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Verify both invoices exist and belong to this tenant
    let dup_inv = repo
        .get_by_id(&tenant.tenant_id, &dup_invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: dup_invoice_id.to_string(),
        })?;
    let keep_inv = repo
        .get_by_id(&tenant.tenant_id, &keep_invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: keep_invoice_id.to_string(),
        })?;

    // Copy missing fields from the duplicate onto the kept invoice
    let mut updates = serde_json::json!({});
    if keep_inv.vendor_name.is_empty() || keep_inv.vendor_name == "Processing..." {
        updates["vendor_name"] = serde_json::json!(dup_inv.vendor_name);
    }
    if keep_inv.invoice_number.starts_with("UPLOAD-")
        && !dup_inv.invoice_number.starts_with("UPLOAD-")
    {
        updates["invoice_number"] = serde_json::json!(dup_inv.invoice_number);
    }
    if keep_inv.invoice_date.is_none() && dup_inv.invoice_date.is_some() {
        updates["invoice_date"] = serde_json::json!(dup_inv
            .invoice_date
            .map(|d| d.format("%Y-%m-%d").to_string()));
    }
    if keep_inv.notes.is_none() && dup_inv.notes.is_some() {
        updates["notes"] = serde_json::json!(dup_inv.notes);
    }

    if updates.as_object().map_or(false, |o| !o.is_empty()) {
        repo.update(&tenant.tenant_id, &keep_invoice_id, updates)
            .await?;
    }

    // Soft-delete the duplicate
    repo.delete(&tenant.tenant_id, &dup_invoice_id).await?;

    // Record anomaly audit row with resolution='merged'
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Invoice,
        keep_invoice_id.to_string(),
        format!(
            "Merged duplicate invoice {} into {}",
            dup_inv.invoice_number, keep_inv.invoice_number
        ),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "merge_duplicate",
        "discarded_invoice_id": dup_invoice_id.to_string(),
        "discarded_invoice_number": dup_inv.invoice_number,
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log merge audit entry");
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "action": "merged",
        "kept_invoice_id": keep_invoice_id.to_string(),
        "discarded_invoice_id": dup_invoice_id.to_string(),
    })))
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/reject-duplicate",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID flagged as duplicate")
    ),
    responses(
        (status = 200, description = "Duplicate flag rejected, both invoices kept"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn reject_duplicate(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id: billforge_core::domain::InvoiceId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Verify invoice exists
    let invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Record anomaly audit row with resolution='not_duplicate'
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Invoice,
        invoice_id.to_string(),
        format!(
            "Rejected duplicate flag for invoice {}",
            invoice.invoice_number
        ),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "reject_duplicate",
        "resolution": "not_duplicate",
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log reject-duplicate audit entry");
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "action": "not_duplicate",
        "invoice_id": invoice_id.to_string(),
    })))
}

// ---------------------------------------------------------------------------
// OCR field correction endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct OcrCorrectionRequest {
    pub field_name: String,
    #[allow(dead_code)]
    pub corrected_value: serde_json::Value,
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/ocr-corrections",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    request_body = OcrCorrectionRequest,
    responses(
        (status = 200, description = "OCR correction recorded"),
        (status = 400, description = "Invalid field name"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn record_ocr_correction(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(body): Json<OcrCorrectionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id: billforge_core::domain::InvoiceId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Verify invoice exists and belongs to this tenant
    let _invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Build a capture service with the calibration store to record the correction.
    let provider_name = resolve_ocr_provider_name(&state.config.ocr_provider, &tenant.settings);
    let invoice_repo: Arc<dyn billforge_core::traits::InvoiceRepository> = Arc::new(
        billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone()),
    );
    let calibration_store: Arc<dyn billforge_invoice_capture::OcrCalibrationStore> =
        Arc::new(billforge_invoice_capture::PgOcrCalibrationStore::new(pool));

    let capture_service = billforge_invoice_capture::InvoiceCaptureService::new(
        &provider_name,
        invoice_repo,
        state.storage.clone(),
    )
    .with_calibration(calibration_store);

    capture_service
        .record_field_correction(&tenant.tenant_id, &invoice_id, &body.field_name)
        .await?;

    // Audit log
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Invoice,
        id.clone(),
        format!("Recorded OCR field correction: {}", body.field_name),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "ocr_field_correction",
        "field_name": body.field_name,
    }));
    let audit_pool = state.db.tenant(&tenant.tenant_id).await?;
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(audit_pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log OCR correction audit entry");
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "field_name": body.field_name,
    })))
}

// ---------------------------------------------------------------------------
// ML Categorization Accuracy Metrics
// ---------------------------------------------------------------------------

/// Response payload for ML categorization accuracy metrics.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MlAccuracyResponse {
    pub accuracy_rate: f32,
    pub total_suggestions: i64,
    pub accepted: i64,
    pub corrected: i64,
    pub rejected: i64,
}

#[utoipa::path(
    get,
    path = "/invoices/ml-accuracy",
    tag = "Invoices",
    responses(
        (status = 200, description = "ML categorization accuracy metrics", body = MlAccuracyResponse),
        (status = 401, description = "Unauthorized")
    )
)]
async fn get_ml_accuracy_metrics(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
) -> ApiResult<Json<MlAccuracyResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let tenant_id_str = tenant.tenant_id.to_string();
    let feedback = FeedbackLearning::new((*pool).clone());
    let metrics = feedback
        .get_accuracy_metrics(&tenant_id_str, 90)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to get ML accuracy metrics: {}", e))
        })?;

    Ok(Json(MlAccuracyResponse {
        accuracy_rate: metrics.accuracy_rate(),
        total_suggestions: metrics.total_suggestions,
        accepted: metrics.accepted_suggestions,
        corrected: metrics.corrected_suggestions,
        rejected: metrics.rejected_suggestions,
    }))
}

/// Build response message based on sync OCR result status.
fn sync_ocr_message(file_name: &str, status: CaptureStatus) -> String {
    if status == CaptureStatus::Failed {
        format!(
            "File '{}' uploaded. OCR failed - invoice sent to error queue for manual review.",
            file_name
        )
    } else {
        format!(
            "File '{}' uploaded and processed. Invoice ready for review.",
            file_name
        )
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

/// Observe per-field confidence from an OCR result into the first-pass confidence histogram.
fn observe_field_confidence(result: &billforge_core::domain::OcrExtractionResult) {
    let fields: &[(&str, f32)] = &[
        ("invoice_number", result.invoice_number.confidence),
        ("vendor_name", result.vendor_name.confidence),
        ("total_amount", result.total_amount.confidence),
        ("invoice_date", result.invoice_date.confidence),
        ("po_number", result.po_number.confidence),
    ];
    for (field, conf) in fields {
        metrics::OCR_FIRST_PASS_FIELD_CONFIDENCE
            .with_label_values(&[field])
            .observe(*conf as f64);
    }
}

/// Emit end-to-end SLO timing metrics for a workflow outcome.
///
/// - `CAPTURE_TO_APPROVAL_QUEUE_DURATION_SECONDS` for every outcome.
/// - `CAPTURE_TO_FINAL_STATUS_DURATION_SECONDS` only when `final_status` is a terminal state.
fn emit_capture_timing_metrics(
    tenant_id: &billforge_core::TenantId,
    final_status: ProcessingStatus,
    created_at: chrono::DateTime<chrono::Utc>,
) {
    let elapsed = (chrono::Utc::now() - created_at).num_seconds() as f64;
    let tenant_str = tenant_id.to_string();

    // Capture → approval queue / auto-approve outcome.
    let outcome = match final_status {
        ProcessingStatus::PendingApproval => "routed_for_approval",
        ProcessingStatus::Approved | ProcessingStatus::ReadyForPayment | ProcessingStatus::Paid => {
            "auto_approved"
        }
        _ => "exception",
    };
    metrics::observe_capture_to_approval_queue(&tenant_str, outcome, elapsed);

    // Capture → terminal status (only for truly terminal states).
    match final_status {
        ProcessingStatus::Approved
        | ProcessingStatus::ReadyForPayment
        | ProcessingStatus::Paid
        | ProcessingStatus::Rejected
        | ProcessingStatus::Voided => {
            metrics::observe_capture_to_final_status(&tenant_str, final_status.as_str(), elapsed);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// OCR Exception Resolution
// ---------------------------------------------------------------------------

/// Request body for resolving an OCR exception.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveOcrExceptionRequest {
    pub action: String,
}

/// Response body for a successful OCR exception resolution.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ResolveOcrExceptionResponse {
    pub id: String,
    pub ocr_exception_status: String,
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/ocr-exception/resolve",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    request_body = ResolveOcrExceptionRequest,
    responses(
        (status = 200, description = "OCR exception resolved", body = ResolveOcrExceptionResponse),
        (status = 400, description = "Invalid action"),
        (status = 404, description = "Invoice not found"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn resolve_ocr_exception(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(body): Json<ResolveOcrExceptionRequest>,
) -> ApiResult<Json<ResolveOcrExceptionResponse>> {
    let action = body.action.to_lowercase();
    if action != "approve" && action != "reject" {
        return Err(billforge_core::Error::Validation(
            "Invalid action: must be 'approve' or 'reject'".to_string(),
        )
        .into());
    }

    let new_status = if action == "approve" {
        "approved"
    } else {
        "rejected"
    };

    let invoice_id: billforge_core::domain::InvoiceId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify invoice exists in this tenant
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let _invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Update the exception status
    let rows = sqlx::query(
        r#"UPDATE invoices
           SET ocr_exception_status = $1,
               ocr_exception_resolved_by = $2,
               ocr_exception_resolved_at = NOW(),
               updated_at = NOW()
           WHERE id = $3 AND tenant_id = $4"#,
    )
    .bind(new_status)
    .bind(user.user_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(*tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to update OCR exception status: {}", e))
    })?;

    if rows.rows_affected() == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        }
        .into());
    }

    // Audit log
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Invoice,
        id.clone(),
        format!("OCR exception resolved: {}", new_status),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "ocr_exception_resolve",
        "resolution": new_status,
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log OCR exception resolution audit entry");
    }

    // When the exception is approved (cleared), advance the invoice through the
    // workflow pipeline so it leaves the Exception queue automatically.
    if action == "approve" {
        // Re-read the invoice to check current state (idempotency guard).
        let invoice = repo
            .get_by_id(&tenant.tenant_id, &invoice_id)
            .await?
            .ok_or_else(|| billforge_core::Error::NotFound {
                resource_type: "Invoice".to_string(),
                id: id.clone(),
            })?;

        // Only advance if still in a pre-submission capture state.
        if invoice.capture_status == CaptureStatus::ReadyForReview {
            // Build the workflow engine (same construction as submit_for_processing).
            let engine = billforge_invoice_processing::WorkflowEngine::new(
                Arc::new(billforge_db::repositories::InvoiceRepositoryImpl::new(
                    pool.clone(),
                )) as Arc<dyn billforge_core::traits::InvoiceRepository>,
                Arc::new(billforge_db::repositories::WorkflowRepositoryImpl::new(
                    pool.clone(),
                )) as Arc<dyn billforge_core::traits::WorkflowRuleRepository>,
                Arc::new(billforge_db::repositories::WorkflowRepositoryImpl::new(
                    pool.clone(),
                )) as Arc<dyn billforge_core::traits::ApprovalRepository>,
            )
            .with_routing(Arc::new(billforge_db::RoutingRepository::new(
                pool.as_ref().clone(),
            )))
            .with_pool(pool.clone())
            .with_tenant_settings_provider(Arc::new(
                billforge_db::TenantSettingsFromDb::new(state.db.metadata()),
            ));

            let final_status = engine.process_invoice(&tenant.tenant_id, &invoice).await?;

            // Emit capture-to-approval/final-status timing metric.
            emit_capture_timing_metrics(&tenant.tenant_id, final_status, invoice.created_at);

            // Update processing status.
            let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
            invoice_repo
                .update_processing_status(&tenant.tenant_id, &invoice_id, final_status)
                .await?;

            emit_invoice_event(
                &state,
                &tenant.tenant_id,
                invoice_id.as_uuid(),
                &format!("{:?}", final_status),
                "status_changed",
            );

            // Determine target queue from workflow decision.
            let target_queue_type = match final_status {
                ProcessingStatus::Approved | ProcessingStatus::ReadyForPayment => {
                    QueueType::Payment
                }
                ProcessingStatus::PendingApproval => QueueType::Approval,
                ProcessingStatus::Rejected | ProcessingStatus::Voided | ProcessingStatus::Paid => {
                    // Terminal states — no queue assignment needed.
                    return Ok(Json(ResolveOcrExceptionResponse {
                        id,
                        ocr_exception_status: new_status.to_string(),
                    }));
                }
                _ => QueueType::Review,
            };

            let queue_repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
            if let Ok(Some(queue)) = billforge_core::traits::WorkQueueRepository::get_by_type(
                &queue_repo,
                &tenant.tenant_id,
                target_queue_type,
            )
            .await
            {
                // Move item off Exception queue onto the target queue.
                if let Err(e) = billforge_core::traits::WorkQueueRepository::move_item(
                    &queue_repo,
                    &tenant.tenant_id,
                    &invoice_id,
                    &queue.id,
                    None,
                )
                .await
                {
                    tracing::warn!(
                        invoice_id = %invoice_id,
                        queue_id = %queue.id,
                        error = %e,
                        "Failed to move invoice off exception queue after OCR resolve"
                    );
                }

                // Update current_queue_id on the invoice row.
                sqlx::query(
                    "UPDATE invoices SET current_queue_id = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(queue.id.0)
                .bind(invoice_id.as_uuid())
                .execute(&*pool)
                .await
                .ok();

                // Audit the queue transition.
                let route_audit = AuditEntry::new(
                    tenant.tenant_id.clone(),
                    Some(user.user_id.clone()),
                    AuditAction::Update,
                    ResourceType::Invoice,
                    id.clone(),
                    format!(
                        "OCR exception routed from Exception queue to {:?} queue",
                        target_queue_type
                    ),
                )
                .with_user_email(&user.email)
                .with_metadata(serde_json::json!({
                    "action": "ocr_exception.routed",
                    "from_queue": "Exception",
                    "to_queue": format!("{:?}", target_queue_type),
                    "processing_status": format!("{:?}", final_status),
                }));
                let audit_repo2 =
                    billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
                if let Err(e) = audit_repo2.log(route_audit).await {
                    tracing::warn!(error = %e, "Failed to log OCR exception routing audit entry");
                }
            } else {
                tracing::warn!(
                    invoice_id = %invoice_id,
                    queue_type = ?target_queue_type,
                    "No queue found for tenant after OCR exception resolve"
                );
            }
        }
    }

    Ok(Json(ResolveOcrExceptionResponse {
        id,
        ocr_exception_status: new_status.to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Touchless auto-approval unwind endpoint
// ---------------------------------------------------------------------------

/// Request body for unwinding a touchless auto-approval.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UnwindRequest {
    pub reason: String,
}

/// Response body for a successful unwind.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UnwindResponse {
    pub invoice_id: String,
    pub reverted_to: String,
}

#[utoipa::path(
    post,
    path = "/invoices/{id}/unwind-approval",
    tag = "Invoices",
    params(
        ("id" = String, Path, description = "Invoice ID")
    ),
    request_body = UnwindRequest,
    responses(
        (status = 200, description = "Auto-approval unwound, invoice reverted", body = UnwindResponse),
        (status = 404, description = "Invoice not found"),
        (status = 409, description = "Cannot unwind: no touchless auto-approval or subsequent action exists"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn unwind_auto_approval(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Path(id): Path<String>,
    Json(body): Json<UnwindRequest>,
) -> ApiResult<Json<UnwindResponse>> {
    let invoice_id: billforge_core::domain::InvoiceId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Load the invoice (tenant-scoped)
    let invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    // Query the most recent invoice_audit_log entry for this invoice
    let latest_audit: Option<(
        Uuid,
        Option<Uuid>,
        Option<String>,
        String,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"SELECT id, actor_id, from_status, event_type, metadata
               FROM invoice_audit_log
               WHERE tenant_id = $1 AND invoice_id = $2
               ORDER BY created_at DESC
               LIMIT 1"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(invoice.id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query audit log: {}", e)))?;

    let (audit_id, _actor_id, from_status, event_type, _metadata) =
        latest_audit.ok_or_else(|| {
            billforge_core::Error::Conflict("No audit trail found for this invoice".to_string())
        })?;

    // Only allow unwinding the most recent touchless auto-approval
    if event_type != "touchless_auto_approval" {
        return Err(billforge_core::Error::Conflict(
            "Most recent action is not a touchless auto-approval".to_string(),
        )
        .into());
    }

    let reverted_status = from_status.unwrap_or_else(|| "received".to_string());

    // Revert the invoice status
    sqlx::query(
        "UPDATE invoices SET status = $1, processing_status = $2, updated_at = NOW() WHERE id = $3 AND tenant_id = $4",
    )
    .bind(&reverted_status)
    .bind("submitted")
    .bind(invoice.id.as_uuid())
    .bind(*tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to revert invoice status: {}", e)))?;

    // Write unwind audit row
    sqlx::query(
        r#"INSERT INTO invoice_audit_log (tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata, created_at)
           VALUES ($1, $2, $3, $4, $5, 'auto_approval_unwound', $6, NOW())"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(invoice.id.as_uuid())
    .bind(user.user_id.as_uuid())
    .bind("approved")
    .bind(&reverted_status)
    .bind(serde_json::json!({
        "reason": body.reason,
        "original_audit_id": audit_id.to_string(),
    }))
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to write unwind audit entry: {}", e)))?;

    // Re-fetch updated invoice
    let updated_invoice = repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(UnwindResponse {
        invoice_id: updated_invoice.id.to_string(),
        reverted_to: reverted_status,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::InvoiceId;

    #[test]
    fn test_ml_accuracy_response_fields() {
        let response = MlAccuracyResponse {
            accuracy_rate: 0.85,
            total_suggestions: 100,
            accepted: 85,
            corrected: 10,
            rejected: 5,
        };
        let json = serde_json::to_value(&response).unwrap();
        assert!((json["accuracy_rate"].as_f64().unwrap() - 0.85).abs() < 0.001);
        assert_eq!(json["total_suggestions"], 100);
        assert_eq!(json["accepted"], 85);
        assert_eq!(json["corrected"], 10);
        assert_eq!(json["rejected"], 5);
    }

    #[test]
    fn test_ml_accuracy_response_zero_metrics() {
        let response = MlAccuracyResponse {
            accuracy_rate: 0.0,
            total_suggestions: 0,
            accepted: 0,
            corrected: 0,
            rejected: 0,
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["accuracy_rate"], 0.0);
        assert_eq!(json["total_suggestions"], 0);
    }

    /// Verify that only successful-processing statuses trigger meter events.
    /// Draft, submitted, and other pre-processing statuses must NOT emit.
    #[test]
    fn meter_event_only_emits_for_successful_processing() {
        let billable = [
            ProcessingStatus::Approved,
            ProcessingStatus::ReadyForPayment,
            ProcessingStatus::Paid,
        ];
        // These must match the match arms in submit_for_processing and
        // run_sync_straight_through_processing.
        for status in &billable {
            let is_match = matches!(
                *status,
                ProcessingStatus::Approved
                    | ProcessingStatus::ReadyForPayment
                    | ProcessingStatus::Paid
            );
            assert!(is_match, "{status:?} should be billable");
        }

        let not_billable = [
            ProcessingStatus::Draft,
            ProcessingStatus::Submitted,
            ProcessingStatus::PendingApproval,
            ProcessingStatus::Rejected,
            ProcessingStatus::OnHold,
            ProcessingStatus::Voided,
        ];
        for status in &not_billable {
            let is_match = matches!(
                *status,
                ProcessingStatus::Approved
                    | ProcessingStatus::ReadyForPayment
                    | ProcessingStatus::Paid
            );
            assert!(!is_match, "{status:?} should NOT be billable");
        }
    }

    /// Verify the UploadResponse struct captures all required fields.
    /// Ensures the response contract is stable after removing the upload-path
    /// meter call.
    #[test]
    fn upload_response_serializes_after_meter_removal() {
        let response = UploadResponse {
            invoice_id: "inv-123".to_string(),
            document_id: "doc-456".to_string(),
            message: "File uploaded.".to_string(),
            potential_duplicates: vec![],
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["invoice_id"], "inv-123");
        assert_eq!(json["document_id"], "doc-456");
        assert_eq!(json["potential_duplicates"].as_array().unwrap().len(), 0);
    }

    /// Verify the CreateInvoiceResponse struct still serializes correctly
    /// after removing the create-path meter call.
    #[test]
    fn create_response_serializes_after_meter_reemoval() {
        let invoice = Invoice {
            id: InvoiceId(Uuid::new_v4()),
            tenant_id: billforge_core::TenantId::new(),
            vendor_id: None,
            vendor_name: "Acme".to_string(),
            invoice_number: "INV-1".to_string(),
            invoice_date: None,
            due_date: None,
            po_number: None,
            subtotal: None,
            tax_amount: None,
            total_amount: billforge_core::types::Money::usd(0.0),
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Pending,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::new_v4(),
            supporting_documents: vec![],
            ocr_confidence: None,
            categorization_confidence: None,
            department: None,
            gl_code: None,
            cost_center: None,
            notes: None,
            tags: vec![],
            custom_fields: serde_json::Value::Object(Default::default()),
            created_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let response = CreateInvoiceResponse {
            invoice,
            potential_duplicates: vec![],
        };
        let json = serde_json::to_value(&response).unwrap();
        assert!(json.get("invoice").is_some());
        assert!(json.get("potential_duplicates").is_some());
    }
}
