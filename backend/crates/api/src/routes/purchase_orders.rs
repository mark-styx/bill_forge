//! Purchase Order management endpoints
//!
//! CRUD for purchase orders and 3-way matching trigger.
//!
//! Endpoints:
//! - GET    /purchase-orders          - List purchase orders
//! - POST   /purchase-orders          - Create a purchase order
//! - GET    /purchase-orders/:id      - Get PO detail
//! - DELETE /purchase-orders/:id      - Delete a PO
//! - POST   /purchase-orders/:id/match - Run 3-way match for an invoice against this PO

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use billforge_core::domain::*;
use billforge_core::traits::{InvoiceRepository, POFilters, PurchaseOrderRepository};
use billforge_core::types::*;
use billforge_db::repositories::{InvoiceRepositoryImpl, PurchaseOrderRepositoryImpl};
use billforge_edi::matching::{InvoiceLineForMatch, MatchEngine};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_purchase_orders).post(create_purchase_order))
        .route("/:id", get(get_purchase_order).delete(delete_purchase_order))
        .route("/:id/match", post(run_match))
}

// ──────────────────────────── Types ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListPOQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub vendor_id: Option<Uuid>,
    pub status: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunMatchRequest {
    pub invoice_id: Uuid,
    pub tolerances: Option<MatchTolerancesInput>,
}

#[derive(Debug, Deserialize)]
pub struct MatchTolerancesInput {
    pub price_variance_pct: Option<f64>,
    pub quantity_variance_pct: Option<f64>,
    pub auto_approve_below_cents: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MatchResponse {
    pub match_type: String,
    pub price_variance_pct: f64,
    pub quantity_variance_pct: f64,
    pub match_result_id: Uuid,
    pub details: serde_json::Value,
}

// ──────────────────────────── Handlers ────────────────────────────

async fn list_purchase_orders(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<ListPOQuery>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state
        .db
        .tenant(&tenant.tenant_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&pool));
    let filters = POFilters {
        vendor_id: query.vendor_id,
        status: query.status.as_deref().and_then(POStatus::from_str),
        search: query.search,
        ..Default::default()
    };
    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(25).min(100),
    };

    let result = repo
        .list(&tenant.tenant_id, &filters, &pagination)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list purchase orders: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "data": result.data,
        "pagination": result.pagination,
    })))
}

async fn create_purchase_order(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<CreatePurchaseOrderInput>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state
        .db
        .tenant(&tenant.tenant_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&pool));
    let po = repo
        .create(&tenant.tenant_id, input, &user.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create purchase order: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!(po)))
}

async fn get_purchase_order(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state
        .db
        .tenant(&tenant.tenant_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&pool));
    let po_id = PurchaseOrderId(id);
    let po = repo
        .get_by_id(&tenant.tenant_id, &po_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get purchase order: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match po {
        Some(po) => Ok(Json(serde_json::json!(po))),
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn delete_purchase_order(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state
        .db
        .tenant(&tenant.tenant_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&pool));
    let po_id = PurchaseOrderId(id);
    repo.delete(&tenant.tenant_id, &po_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete purchase order: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Run 3-way match: compare an invoice against this PO (and any receiving records)
async fn run_match(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(po_uuid): Path<Uuid>,
    Json(req): Json<RunMatchRequest>,
) -> Result<Json<MatchResponse>, axum::http::StatusCode> {
    let pool = state
        .db
        .tenant(&tenant.tenant_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    // 1. Load PO with line items
    let po_repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&pool));
    let po_id = PurchaseOrderId(po_uuid);
    let po = po_repo
        .get_by_id(&tenant.tenant_id, &po_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load PO for matching: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // 2. Load invoice with line items
    let inv_repo = InvoiceRepositoryImpl::new(Arc::clone(&pool));
    let invoice_id = InvoiceId(req.invoice_id);
    let invoice = inv_repo
        .get_by_id(&tenant.tenant_id, &invoice_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load invoice for matching: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // 3. Load receiving records for this PO
    let recv_rows = sqlx::query_as::<_, (Uuid, i32, f32, f32, Option<String>)>(
        r#"SELECT rl.id, rl.po_line_number, rl.quantity_received, rl.quantity_damaged, rl.product_id
           FROM receiving_line_items rl
           JOIN receiving_records rr ON rl.receiving_id = rr.id
           WHERE rr.po_id = $1 AND rr.tenant_id = $2"#,
    )
    .bind(po_uuid)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load receiving records: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let receiving_lines: Vec<ReceivingLineItem> = recv_rows
        .iter()
        .map(|r| ReceivingLineItem {
            id: r.0,
            po_line_number: r.1 as u32,
            quantity_received: r.2 as f64,
            quantity_damaged: r.3 as f64,
            product_id: r.4.clone(),
        })
        .collect();

    // 4. Convert invoice line items to match format
    let invoice_lines: Vec<InvoiceLineForMatch> = invoice
        .line_items
        .iter()
        .map(|li| InvoiceLineForMatch {
            line_number: li.line_number,
            quantity: li.quantity.unwrap_or(1.0),
            unit_price_cents: li.unit_price.as_ref().map(|m| m.amount).unwrap_or(0),
            product_id: None, // invoice line items don't carry product_id directly
        })
        .collect();

    // 5. Build tolerances
    let tolerances = match &req.tolerances {
        Some(t) => MatchTolerances {
            price_variance_pct: t.price_variance_pct.unwrap_or(2.0),
            quantity_variance_pct: t.quantity_variance_pct.unwrap_or(5.0),
            auto_approve_below_cents: t.auto_approve_below_cents.unwrap_or(100_000),
        },
        None => MatchTolerances::default(),
    };

    // 6. Run match engine
    let match_output = MatchEngine::run(&po.line_items, &receiving_lines, &invoice_lines, &tolerances);
    let details = serde_json::to_value(&match_output).unwrap_or_default();

    // 7. Store match result
    let match_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO match_results (id, tenant_id, invoice_id, po_id, match_type, price_variance_pct, quantity_variance_pct, details)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(match_id)
    .bind(*tenant.tenant_id.as_uuid())
    .bind(req.invoice_id)
    .bind(po_uuid)
    .bind(match_output.match_type.as_str())
    .bind(match_output.overall_price_variance_pct as f32)
    .bind(match_output.overall_quantity_variance_pct as f32)
    .bind(&details)
    .execute(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to store match result: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 8. If full match and under auto-approve threshold, auto-approve
    //    (unless pending approval_requests exist - must not bypass workflow)
    let pending_approvals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approval_requests WHERE invoice_id = $1 AND status = 'pending'"
    )
    .bind(req.invoice_id)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check pending approval_requests for invoice {}: {}", req.invoice_id, e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if match_output.match_type == MatchType::Full
        && invoice.total_amount.amount <= tolerances.auto_approve_below_cents
        && pending_approvals == 0
    {
        sqlx::query(
            "UPDATE invoices SET processing_status = 'approved', updated_at = NOW() WHERE id = $1",
        )
        .bind(req.invoice_id)
        .execute(&*pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to auto-approve matched invoice: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

        tracing::info!(
            invoice_id = %req.invoice_id,
            po_id = %po_uuid,
            "Invoice auto-approved via 3-way match"
        );
    } else if match_output.match_type == MatchType::Full
        && invoice.total_amount.amount <= tolerances.auto_approve_below_cents
        && pending_approvals > 0
    {
        tracing::warn!(
            invoice_id = %req.invoice_id,
            pending_approvals = pending_approvals,
            "Skipping auto-approve for invoice with pending approval requests"
        );
    }

    Ok(Json(MatchResponse {
        match_type: match_output.match_type.as_str().to_string(),
        price_variance_pct: match_output.overall_price_variance_pct,
        quantity_variance_pct: match_output.overall_quantity_variance_pct,
        match_result_id: match_id,
        details,
    }))
}
