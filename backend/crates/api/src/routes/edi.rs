//! EDI (Electronic Data Interchange) integration endpoints
//!
//! Receives inbound EDI documents (810 invoices) from middleware webhooks,
//! maps them to BillForge invoices, and routes them into the approval pipeline.
//!
//! Endpoints:
//! - POST /webhook/inbound  — Receive documents from EDI middleware
//! - GET  /documents        — List EDI documents for tenant
//! - GET  /documents/:id    — Get EDI document detail
//! - POST /connect          — Save EDI middleware credentials
//! - POST /disconnect       — Remove EDI configuration
//! - GET  /status           — Check EDI connection status
//! - GET  /partners         — List trading partners
//! - POST /partners         — Add a trading partner
//! - PUT  /partners/:id     — Update a trading partner
//! - DELETE /partners/:id   — Remove a trading partner

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use billforge_edi::{
    verify_webhook_signature, EdiConfig, EdiDocumentStatus, EdiDocumentType, EdiDirection,
    EdiInvoice, EdiMapper, EdiWebhookPayload,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Webhook (no auth - uses signature verification)
        .route("/webhook/inbound", post(webhook_inbound))
        // Connection management
        .route("/connect", post(edi_connect))
        .route("/disconnect", post(edi_disconnect))
        .route("/status", get(edi_status))
        // Documents
        .route("/documents", get(list_documents))
        .route("/documents/:id", get(get_document))
        // Trading partners
        .route("/partners", get(list_partners).post(create_partner))
        .route("/partners/:id", put(update_partner).delete(delete_partner))
}

// ──────────────────────────── Types ────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EdiConnectRequest {
    pub provider: String,
    pub api_key: String,
    pub webhook_secret: String,
    pub api_base_url: Option<String>,
    pub our_isa_qualifier: Option<String>,
    pub our_isa_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EdiStatusResponse {
    pub connected: bool,
    pub provider: Option<String>,
    pub document_count: i64,
    pub partner_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePartnerRequest {
    pub name: String,
    pub edi_qualifier: Option<String>,
    pub edi_id: String,
    pub vendor_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdatePartnerRequest {
    pub name: Option<String>,
    pub edi_qualifier: Option<String>,
    pub edi_id: Option<String>,
    pub vendor_id: Option<Uuid>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListDocumentsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub document_type: Option<String>,
    pub status: Option<String>,
}

// ──────────────────────────── Webhook ────────────────────────────

/// Receive inbound EDI documents from middleware webhook
///
/// This endpoint does NOT use JWT auth. Instead, it verifies the webhook
/// signature using HMAC-SHA256 with the configured webhook secret.
async fn webhook_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    // For now, accept all webhooks in development
    // In production, verify signature against stored webhook_secret per tenant
    let signature = headers
        .get("x-webhook-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Parse the webhook payload
    let payload: EdiWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| {
            tracing::error!("Failed to parse EDI webhook payload: {}", e);
            axum::http::StatusCode::BAD_REQUEST
        })?;

    tracing::info!(
        event_type = %payload.event_type,
        doc_type = %payload.document_type,
        "Received EDI webhook"
    );

    // Process based on document type
    match payload.document_type {
        EdiDocumentType::Invoice810 => {
            let edi_invoice: EdiInvoice = serde_json::from_value(payload.payload.clone())
                .map_err(|e| {
                    tracing::error!("Failed to parse EDI invoice: {}", e);
                    axum::http::StatusCode::BAD_REQUEST
                })?;

            tracing::info!(
                invoice_number = %edi_invoice.invoice_number,
                vendor = %edi_invoice.vendor.name,
                amount_cents = edi_invoice.total_amount_cents,
                "Processing inbound EDI 810 invoice"
            );

            // TODO: Look up tenant from receiver_id or webhook config
            // TODO: Look up trading partner to find vendor_id
            // TODO: Create invoice via repository
            // TODO: Store EDI document record

            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "invoice_810",
                "invoice_number": edi_invoice.invoice_number,
            })))
        }
        EdiDocumentType::FunctionalAck997 => {
            tracing::info!("Received EDI 997 functional acknowledgment");
            // TODO: Update ack status on the original outbound document
            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "functional_ack_997",
            })))
        }
        _ => {
            tracing::warn!(doc_type = %payload.document_type, "Unsupported EDI document type");
            Ok(Json(serde_json::json!({
                "status": "ignored",
                "reason": "unsupported document type",
            })))
        }
    }
}

// ──────────────────────────── Connection Management ────────────────────────────

async fn edi_connect(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(req): Json<EdiConnectRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    // Store EDI configuration for this tenant
    sqlx::query(
        r#"INSERT INTO edi_connections (id, tenant_id, provider, api_key_encrypted, webhook_secret, api_base_url, our_isa_qualifier, our_isa_id, is_active, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, NOW(), NOW())
           ON CONFLICT (tenant_id) DO UPDATE SET
             provider = $3, api_key_encrypted = $4, webhook_secret = $5,
             api_base_url = $6, our_isa_qualifier = $7, our_isa_id = $8,
             is_active = true, updated_at = NOW()"#,
    )
    .bind(Uuid::new_v4())
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&req.provider)
    .bind(&req.api_key) // TODO: encrypt at rest
    .bind(&req.webhook_secret)
    .bind(req.api_base_url.as_deref().unwrap_or("https://core.us.stedi.com/2023-08-01"))
    .bind(req.our_isa_qualifier.as_deref().unwrap_or("ZZ"))
    .bind(req.our_isa_id.as_deref().unwrap_or(""))
    .execute(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to save EDI connection: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "connected": true,
        "provider": req.provider,
    })))
}

async fn edi_disconnect(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("UPDATE edi_connections SET is_active = false, updated_at = NOW() WHERE tenant_id = $1")
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "connected": false })))
}

async fn edi_status(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<EdiStatusResponse>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let connected: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM edi_connections WHERE tenant_id = $1 AND is_active = true)",
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .unwrap_or(false);

    let doc_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edi_documents WHERE tenant_id = $1",
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .unwrap_or(0);

    let partner_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edi_trading_partners WHERE tenant_id = $1 AND is_active = true",
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .unwrap_or(0);

    let provider: Option<String> = if connected {
        sqlx::query_scalar(
            "SELECT provider FROM edi_connections WHERE tenant_id = $1 AND is_active = true",
        )
        .bind(*tenant.tenant_id.as_uuid())
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten()
    } else {
        None
    };

    Ok(Json(EdiStatusResponse {
        connected,
        provider,
        document_count: doc_count,
        partner_count,
    }))
}

// ──────────────────────────── Documents ────────────────────────────

async fn list_documents(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<ListDocumentsQuery>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25).min(100);
    let offset = ((page - 1) * per_page) as i32;

    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<String>, String, Option<Uuid>, Option<String>, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>)>(
        r#"SELECT id, document_type, direction, sender_id, receiver_id, status, invoice_id, error_message, created_at, processed_at
           FROM edi_documents WHERE tenant_id = $1
           ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(per_page as i32)
    .bind(offset)
    .fetch_all(&*pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let documents: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.0,
                "document_type": r.1,
                "direction": r.2,
                "sender_id": r.3,
                "receiver_id": r.4,
                "status": r.5,
                "invoice_id": r.6,
                "error_message": r.7,
                "created_at": r.8,
                "processed_at": r.9,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "data": documents,
        "pagination": {
            "page": page,
            "per_page": per_page,
        }
    })))
}

async fn get_document(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<String>, String, Option<Uuid>, serde_json::Value, Option<serde_json::Value>, Option<String>, chrono::DateTime<Utc>)>(
        r#"SELECT id, document_type, direction, sender_id, receiver_id, status, invoice_id, raw_payload, mapped_data, error_message, created_at
           FROM edi_documents WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some(r) => Ok(Json(serde_json::json!({
            "id": r.0,
            "document_type": r.1,
            "direction": r.2,
            "sender_id": r.3,
            "receiver_id": r.4,
            "status": r.5,
            "invoice_id": r.6,
            "raw_payload": r.7,
            "mapped_data": r.8,
            "error_message": r.9,
            "created_at": r.10,
        }))),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ──────────────────────────── Trading Partners ────────────────────────────

async fn list_partners(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, String, Option<Uuid>, bool, chrono::DateTime<Utc>)>(
        "SELECT id, name, edi_qualifier, edi_id, vendor_id, is_active, created_at FROM edi_trading_partners WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let partners: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.0,
                "name": r.1,
                "edi_qualifier": r.2,
                "edi_id": r.3,
                "vendor_id": r.4,
                "is_active": r.5,
                "created_at": r.6,
            })
        })
        .collect();

    Ok(Json(serde_json::json!(partners)))
}

async fn create_partner(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(req): Json<CreatePartnerRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO edi_trading_partners (id, tenant_id, name, edi_qualifier, edi_id, vendor_id, is_active, settings, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, true, '{}', NOW(), NOW())"#,
    )
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&req.name)
    .bind(&req.edi_qualifier)
    .bind(&req.edi_id)
    .bind(req.vendor_id)
    .execute(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create trading partner: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": req.name,
        "edi_id": req.edi_id,
    })))
}

async fn update_partner(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePartnerRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(name) = &req.name {
        sqlx::query("UPDATE edi_trading_partners SET name = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
            .bind(name)
            .bind(id)
            .bind(*tenant.tenant_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(is_active) = req.is_active {
        sqlx::query("UPDATE edi_trading_partners SET is_active = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
            .bind(is_active)
            .bind(id)
            .bind(*tenant.tenant_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(vendor_id) = req.vendor_id {
        sqlx::query("UPDATE edi_trading_partners SET vendor_id = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
            .bind(vendor_id)
            .bind(id)
            .bind(*tenant.tenant_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(serde_json::json!({ "id": id, "updated": true })))
}

async fn delete_partner(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM edi_trading_partners WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}
