//! Payment request routes

use crate::error::ApiResult;
use crate::extractors::InvoiceCaptureAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use billforge_core::types::PaginationMeta;
use billforge_db::repositories::{PaymentRequestRepositoryImpl, PaymentRequest, PaymentRequestItem};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_payment_request))
        .route("/", get(list_payment_requests))
        .route("/:id", get(get_payment_request))
        .route("/:id/invoices", post(add_invoices))
        .route("/:id/submit", post(submit_request))
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreatePaymentRequestBody {
    pub invoice_ids: Vec<Uuid>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddInvoicesBody {
    pub invoice_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,
    pub vendor_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PaymentRequestResponse {
    pub id: Uuid,
    pub request_number: String,
    pub status: String,
    pub vendor_id: Option<Uuid>,
    pub vendor_name: Option<String>,
    pub total_amount_cents: i64,
    pub currency: String,
    pub invoice_count: i32,
    pub earliest_due_date: Option<chrono::NaiveDate>,
    pub latest_due_date: Option<chrono::NaiveDate>,
    pub items: Vec<PaymentRequestItemResponse>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub submitted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct PaymentRequestItemResponse {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub invoice_number: String,
    pub vendor_name: String,
    pub amount_cents: i64,
    pub currency: String,
    pub due_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct PaymentRequestListResponse {
    pub data: Vec<PaymentRequestSummaryResponse>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize)]
pub struct PaymentRequestSummaryResponse {
    pub id: Uuid,
    pub request_number: String,
    pub status: String,
    pub vendor_id: Option<Uuid>,
    pub total_amount_cents: i64,
    pub currency: String,
    pub invoice_count: i32,
    pub earliest_due_date: Option<chrono::NaiveDate>,
    pub latest_due_date: Option<chrono::NaiveDate>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub submitted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post, path = "/api/v1/payment-requests", tag = "Payment Requests", request_body = serde_json::Value,
    responses((status = 200, description = "Payment request created"), (status = 401, description = "Unauthorized")))]
async fn create_payment_request(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Json(body): Json<CreatePaymentRequestBody>,
) -> ApiResult<Json<PaymentRequestResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = PaymentRequestRepositoryImpl::new(pool);

    let request = repo
        .create_payment_request(
            &tenant.tenant_id,
            user.user_id.0,
            &body.invoice_ids,
            body.notes,
        )
        .await?;

    // Fetch with items for the response
    let (_, items) = repo
        .get_payment_request(&tenant.tenant_id, request.id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "PaymentRequest".to_string(),
            id: request.id.to_string(),
        })?;

    Ok(Json(to_response(request, items)))
}

#[utoipa::path(get, path = "/api/v1/payment-requests", tag = "Payment Requests",
    params(("page" = Option<u32>, Query,), ("per_page" = Option<u32>, Query,), ("status" = Option<String>, Query,)),
    responses((status = 200, description = "Payment request list")))]
async fn list_payment_requests(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<PaymentRequestListResponse>> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = PaymentRequestRepositoryImpl::new(pool);

    let (requests, total) = repo
        .list_payment_requests(
            &tenant.tenant_id,
            query.status.as_deref(),
            query.vendor_id,
            page,
            per_page,
        )
        .await?;

    let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;

    Ok(Json(PaymentRequestListResponse {
        data: requests.into_iter().map(to_summary).collect(),
        pagination: PaginationMeta {
            page,
            per_page,
            total_items: total,
            total_pages,
        },
    }))
}

#[utoipa::path(get, path = "/api/v1/payment-requests/{id}", tag = "Payment Requests",
    params(("id" = String, Path, description = "Payment request ID")),
    responses((status = 200, description = "Payment request details"), (status = 404, description = "Not found")))]
async fn get_payment_request(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PaymentRequestResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = PaymentRequestRepositoryImpl::new(pool);

    let (request, items) = repo
        .get_payment_request(&tenant.tenant_id, id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "PaymentRequest".to_string(),
            id: id.to_string(),
        })?;

    Ok(Json(to_response(request, items)))
}

#[utoipa::path(post, path = "/api/v1/payment-requests/{id}/invoices", tag = "Payment Requests", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Payment request ID")),
    responses((status = 200, description = "Invoices added")))]
async fn add_invoices(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<Uuid>,
    Json(body): Json<AddInvoicesBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = PaymentRequestRepositoryImpl::new(pool);

    repo.add_invoices_to_request(&tenant.tenant_id, id, &body.invoice_ids)
        .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/payment-requests/{id}/submit", tag = "Payment Requests", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Payment request ID")),
    responses((status = 200, description = "Payment request submitted")))]
async fn submit_request(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PaymentRequestResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = PaymentRequestRepositoryImpl::new(pool);

    let request = repo.submit_payment_request(&tenant.tenant_id, id).await?;

    let (_, items) = repo
        .get_payment_request(&tenant.tenant_id, id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "PaymentRequest".to_string(),
            id: id.to_string(),
        })?;

    Ok(Json(to_response(request, items)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_response(
    request: PaymentRequest,
    items: Vec<PaymentRequestItem>,
) -> PaymentRequestResponse {
    PaymentRequestResponse {
        id: request.id,
        request_number: request.request_number,
        status: request.status,
        vendor_id: request.vendor_id,
        vendor_name: None, // could be joined from vendors table if needed
        total_amount_cents: request.total_amount_cents,
        currency: request.currency,
        invoice_count: request.invoice_count,
        earliest_due_date: request.earliest_due_date,
        latest_due_date: request.latest_due_date,
        items: items
            .into_iter()
            .map(|i| PaymentRequestItemResponse {
                id: i.id,
                invoice_id: i.invoice_id,
                invoice_number: i.invoice_number,
                vendor_name: i.vendor_name,
                amount_cents: i.amount_cents,
                currency: i.currency,
                due_date: i.due_date,
            })
            .collect(),
        notes: request.notes,
        created_by: request.created_by,
        submitted_at: request.submitted_at,
        created_at: request.created_at,
    }
}

fn to_summary(
    request: PaymentRequest,
) -> PaymentRequestSummaryResponse {
    PaymentRequestSummaryResponse {
        id: request.id,
        request_number: request.request_number,
        status: request.status,
        vendor_id: request.vendor_id,
        total_amount_cents: request.total_amount_cents,
        currency: request.currency,
        invoice_count: request.invoice_count,
        earliest_due_date: request.earliest_due_date,
        latest_due_date: request.latest_due_date,
        notes: request.notes,
        created_by: request.created_by,
        submitted_at: request.submitted_at,
        created_at: request.created_at,
    }
}
