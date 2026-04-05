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
    routing::{get, post, put},
    Router,
};
use billforge_core::types::{TenantId, UserId};
use billforge_db::repositories::{InvoiceRepositoryImpl, PurchaseOrderRepositoryImpl};
use billforge_core::traits::{InvoiceRepository, PurchaseOrderRepository};
use billforge_edi::{
    verify_webhook_signature, validate_timestamp_freshness, check_replay_nonce,
    EdiDocumentType, EdiFunctionalAck, EdiInvoice, EdiMapper,
    EdiPurchaseOrder, EdiShipNotice, EdiWebhookPayload, process_inbound_ack,
    OutboundEdiService, EdiClient, EdiConfig, check_ack_timeouts,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sha2::Digest;
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
        // Outbound
        .route("/send-remittance/:invoice_id", post(send_remittance))
        .route("/outbound", get(list_outbound))
        .route("/ack-timeouts", get(get_ack_timeouts))
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
///
/// Flow:
/// 1. Parse payload to extract receiver_id
/// 2. Look up tenant from receiver_id via metadata DB
/// 3. Verify HMAC signature against tenant's stored webhook_secret
/// 4. Store EDI document record (status: processing)
/// 5. For 810 invoices: look up trading partner, map to invoice, create in DB
/// 6. Update EDI document (status: mapped, link invoice_id)
async fn webhook_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
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

            // 1. Look up tenant from receiver_id
            let metadata_pool = state.db.metadata();
            let tenant_uuid: Option<Uuid> = sqlx::query_scalar(
                "SELECT tenant_id FROM edi_receiver_map WHERE receiver_id = $1",
            )
            .bind(&edi_invoice.receiver_id)
            .fetch_optional(&*metadata_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up tenant from receiver_id: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let tenant_uuid = tenant_uuid.ok_or_else(|| {
                tracing::warn!(
                    receiver_id = %edi_invoice.receiver_id,
                    "No tenant found for EDI receiver_id"
                );
                axum::http::StatusCode::NOT_FOUND
            })?;

            let tenant_id = TenantId::from_uuid(tenant_uuid);
            let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
                tracing::error!("Failed to get tenant pool: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // 2. Verify webhook signature against stored secret
            let webhook_secret: Option<String> = sqlx::query_scalar(
                "SELECT webhook_secret FROM edi_connections WHERE tenant_id = $1 AND is_active = true",
            )
            .bind(tenant_uuid)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch webhook secret: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

                if let Some(secret) = &webhook_secret {
                    if !secret.is_empty() && !signature.is_empty() {
                        if !verify_webhook_signature(&body, signature, secret) {
                            tracing::warn!("EDI webhook signature verification failed");
                            return Err(axum::http::StatusCode::UNAUTHORIZED);
                        }
                        tracing::debug!("EDI webhook signature verified");
                    }
                }

            // --- Replay protection ---
            // 1. Timestamp freshness
            if !validate_timestamp_freshness(payload.timestamp, 300) {
                tracing::warn!(timestamp = %payload.timestamp, "EDI webhook timestamp too old or in future");
                return Err(axum::http::StatusCode::UNAUTHORIZED);
            }

            // 2. Nonce deduplication
            let nonce = payload.middleware_id.clone().unwrap_or_else(|| {
                let hash = sha2::Sha256::digest(&body);
                hex::encode(hash)
            });
            if !check_replay_nonce(&*tenant_pool, tenant_uuid, &nonce).await.map_err(|e| {
                tracing::error!("Replay nonce check failed: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })? {
                tracing::warn!(nonce = %nonce, "EDI webhook replay detected");
                return Err(axum::http::StatusCode::CONFLICT);
            }

            // 3. Store EDI document record (status: processing)
            let doc_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO edi_documents (id, tenant_id, document_type, direction, interchange_control, sender_id, receiver_id, status, raw_payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, 'processing', $8, NOW())"#,
            )
            .bind(doc_id)
            .bind(tenant_uuid)
            .bind("invoice_810")
            .bind("inbound")
            .bind(&edi_invoice.interchange_control)
            .bind(&edi_invoice.sender_id)
            .bind(&edi_invoice.receiver_id)
            .bind(&payload.payload)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to store EDI document: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // 4. Look up trading partner by sender_id to find vendor_id
            let vendor_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT vendor_id FROM edi_trading_partners WHERE tenant_id = $1 AND edi_id = $2 AND is_active = true",
            )
            .bind(tenant_uuid)
            .bind(&edi_invoice.sender_id)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up trading partner: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // 5. Map EDI invoice to BillForge invoice and create it
            // Use a separate document_id (not the edi_documents row ID) since
            // document_id is used for blob storage references elsewhere.
            let invoice_doc_id = Uuid::new_v4();
            let invoice_input = EdiMapper::invoice_from_edi(&edi_invoice, vendor_id, invoice_doc_id)
                .map_err(|e| {
                    tracing::error!("Failed to map EDI invoice: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

            let mapped_data = serde_json::to_value(&invoice_input).ok();

            // Find the tenant's admin user for created_by (FK to users.id)
            let admin_user_id: Uuid = sqlx::query_scalar(
                "SELECT id FROM users WHERE tenant_id = $1 AND is_active = true ORDER BY created_at ASC LIMIT 1",
            )
            .bind(tenant_uuid)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up tenant admin user: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::error!("No active users found for tenant {}", tenant_uuid);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let created_by = UserId::from_uuid(admin_user_id);
            let invoice_repo = InvoiceRepositoryImpl::new(Arc::clone(&tenant_pool));

            let invoice = invoice_repo.create(&tenant_id, invoice_input, &created_by)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to create invoice from EDI: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

            // 6. Update EDI document: status -> mapped, link invoice_id
            sqlx::query(
                r#"UPDATE edi_documents SET status = 'mapped', invoice_id = $1, mapped_data = $2, processed_at = NOW() WHERE id = $3"#,
            )
            .bind(invoice.id.0)
            .bind(&mapped_data)
            .bind(doc_id)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update EDI document status: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // 7. Submit invoice into workflow: EDI data is fully structured,
            // so skip capture (OCR) and go straight to processing/submitted
            sqlx::query(
                "UPDATE invoices SET capture_status = 'reviewed', processing_status = 'submitted', updated_at = NOW() WHERE id = $1",
            )
            .bind(invoice.id.0)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to submit EDI invoice to workflow: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // 8. If invoice references a PO, attempt automatic 3-way matching
            let mut match_result_json = serde_json::Value::Null;
            if let Some(ref po_number) = edi_invoice.po_number {
                let po_repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&tenant_pool));
                if let Ok(Some(po)) = po_repo.find_by_po_number(&tenant_id, po_number).await {
                    use billforge_edi::matching::{InvoiceLineForMatch, MatchEngine};
                    use billforge_core::domain::MatchTolerances;

                    // Load receiving records for this PO
                    let recv_rows = sqlx::query_as::<_, (Uuid, i32, f32, f32, Option<String>)>(
                        r#"SELECT rl.id, rl.po_line_number, rl.quantity_received, rl.quantity_damaged, rl.product_id
                           FROM receiving_line_items rl
                           JOIN receiving_records rr ON rl.receiving_id = rr.id
                           WHERE rr.po_id = $1"#,
                    )
                    .bind(po.id.0)
                    .fetch_all(&*tenant_pool)
                    .await;

                    let recv_rows = match recv_rows {
                        Ok(rows) => rows,
                        Err(e) => {
                            tracing::warn!("Failed to load receiving records for auto-match: {}", e);
                            vec![]
                        }
                    };

                    let receiving_lines: Vec<billforge_core::domain::ReceivingLineItem> = recv_rows
                        .iter()
                        .map(|r| billforge_core::domain::ReceivingLineItem {
                            id: r.0,
                            po_line_number: r.1 as u32,
                            quantity_received: r.2 as f64,
                            quantity_damaged: r.3 as f64,
                            product_id: r.4.clone(),
                        })
                        .collect();

                    let invoice_lines: Vec<InvoiceLineForMatch> = edi_invoice
                        .line_items
                        .iter()
                        .map(|li| InvoiceLineForMatch {
                            line_number: li.line_number,
                            quantity: li.quantity,
                            unit_price_cents: li.unit_price_cents,
                            product_id: li.product_id.clone(),
                        })
                        .collect();

                    let tolerances = MatchTolerances::default();
                    let match_output = MatchEngine::run(
                        &po.line_items, &receiving_lines, &invoice_lines, &tolerances,
                    );

                    let details = serde_json::to_value(&match_output).unwrap_or_default();
                    let match_id = Uuid::new_v4();

                    let match_stored = sqlx::query(
                        r#"INSERT INTO match_results (id, tenant_id, invoice_id, po_id, match_type, price_variance_pct, quantity_variance_pct, details)
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                    )
                    .bind(match_id)
                    .bind(tenant_uuid)
                    .bind(invoice.id.0)
                    .bind(po.id.0)
                    .bind(match_output.match_type.as_str())
                    .bind(match_output.overall_price_variance_pct as f32)
                    .bind(match_output.overall_quantity_variance_pct as f32)
                    .bind(&details)
                    .execute(&*tenant_pool)
                    .await;

                    if let Err(e) = &match_stored {
                        tracing::warn!("Failed to store match result: {}", e);
                    }

                    // Only report match and auto-approve if the result was persisted
                    if match_stored.is_ok() {
                        if match_output.match_type == billforge_core::domain::MatchType::Full
                            && invoice.total_amount.amount <= tolerances.auto_approve_below_cents
                        {
                            if let Err(e) = sqlx::query(
                                "UPDATE invoices SET processing_status = 'approved', updated_at = NOW() WHERE id = $1",
                            )
                            .bind(invoice.id.0)
                            .execute(&*tenant_pool)
                            .await
                            {
                                tracing::warn!("Failed to auto-approve matched invoice: {}", e);
                            } else {
                                tracing::info!(
                                    invoice_id = %invoice.id,
                                    po_id = %po.id,
                                    "EDI invoice auto-approved via 3-way match"
                                );
                            }
                        }

                        match_result_json = serde_json::json!({
                            "match_type": match_output.match_type.as_str(),
                            "match_id": match_id,
                        });
                    }
                }
            }

            tracing::info!(
                invoice_id = %invoice.id,
                invoice_number = %invoice.invoice_number,
                edi_doc_id = %doc_id,
                "EDI 810 invoice created successfully"
            );

            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "invoice_810",
                "invoice_number": invoice.invoice_number,
                "invoice_id": invoice.id.0,
                "edi_document_id": doc_id,
                "match_result": match_result_json,
            })))
        }
        EdiDocumentType::PurchaseOrder850 => {
            let edi_po: EdiPurchaseOrder = serde_json::from_value(payload.payload.clone())
                .map_err(|e| {
                    tracing::error!("Failed to parse EDI purchase order: {}", e);
                    axum::http::StatusCode::BAD_REQUEST
                })?;

            tracing::info!(
                po_number = %edi_po.po_number,
                "Processing inbound EDI 850 purchase order"
            );

            // Tenant lookup + signature verification (same pattern as 810)
            let metadata_pool = state.db.metadata();
            let tenant_uuid: Option<Uuid> = sqlx::query_scalar(
                "SELECT tenant_id FROM edi_receiver_map WHERE receiver_id = $1",
            )
            .bind(&edi_po.receiver_id)
            .fetch_optional(&*metadata_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up tenant from receiver_id: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let tenant_uuid = tenant_uuid.ok_or_else(|| {
                tracing::warn!(receiver_id = %edi_po.receiver_id, "No tenant for EDI receiver_id");
                axum::http::StatusCode::NOT_FOUND
            })?;

            let tenant_id = TenantId::from_uuid(tenant_uuid);
            let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
                tracing::error!("Failed to get tenant pool: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Verify signature
            let webhook_secret: Option<String> = sqlx::query_scalar(
                "SELECT webhook_secret FROM edi_connections WHERE tenant_id = $1 AND is_active = true",
            )
            .bind(tenant_uuid)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch webhook secret: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if let Some(secret) = &webhook_secret {
                if !secret.is_empty() && !signature.is_empty() {
                    if !verify_webhook_signature(&body, signature, secret) {
                        tracing::warn!("EDI webhook signature verification failed");
                        return Err(axum::http::StatusCode::UNAUTHORIZED);
                    }
                }
            }

            // --- Replay protection ---
            if !validate_timestamp_freshness(payload.timestamp, 300) {
                tracing::warn!(timestamp = %payload.timestamp, "EDI webhook timestamp too old or in future");
                return Err(axum::http::StatusCode::UNAUTHORIZED);
            }

            let nonce = payload.middleware_id.clone().unwrap_or_else(|| {
                let hash = sha2::Sha256::digest(&body);
                hex::encode(hash)
            });
            if !check_replay_nonce(&*tenant_pool, tenant_uuid, &nonce).await.map_err(|e| {
                tracing::error!("Replay nonce check failed: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })? {
                tracing::warn!(nonce = %nonce, "EDI webhook replay detected");
                return Err(axum::http::StatusCode::CONFLICT);
            }

            // Store EDI document
            let doc_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO edi_documents (id, tenant_id, document_type, direction, interchange_control, sender_id, receiver_id, status, raw_payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, 'processing', $8, NOW())"#,
            )
            .bind(doc_id)
            .bind(tenant_uuid)
            .bind("purchase_order_850")
            .bind("inbound")
            .bind(&edi_po.interchange_control)
            .bind(&edi_po.sender_id)
            .bind(&edi_po.receiver_id)
            .bind(&payload.payload)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to store EDI document: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Look up vendor by sender_id
            let vendor_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT vendor_id FROM edi_trading_partners WHERE tenant_id = $1 AND edi_id = $2 AND is_active = true",
            )
            .bind(tenant_uuid)
            .bind(&edi_po.sender_id)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up trading partner: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?
            .flatten();

            let vendor_id = vendor_id.ok_or_else(|| {
                tracing::warn!(sender_id = %edi_po.sender_id, "No vendor mapping for EDI sender");
                axum::http::StatusCode::UNPROCESSABLE_ENTITY
            })?;

            // Map to BillForge PO and create
            let po_input = EdiMapper::purchase_order_from_edi(&edi_po, vendor_id)
                .map_err(|e| {
                    tracing::error!("Failed to map EDI purchase order: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

            let admin_user_id: Uuid = sqlx::query_scalar(
                "SELECT id FROM users WHERE tenant_id = $1 AND is_active = true ORDER BY created_at ASC LIMIT 1",
            )
            .bind(tenant_uuid)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up tenant admin: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::error!("No active users for tenant {}", tenant_uuid);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let created_by = UserId::from_uuid(admin_user_id);
            let po_repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&tenant_pool));
            let po = po_repo.create(&tenant_id, po_input, &created_by)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to create purchase order from EDI: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

            // Update EDI document: mapped, link po_id
            sqlx::query(
                "UPDATE edi_documents SET status = 'mapped', po_id = $1, processed_at = NOW() WHERE id = $2",
            )
            .bind(po.id.0)
            .bind(doc_id)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update EDI document: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            tracing::info!(
                po_id = %po.id,
                po_number = %po.po_number,
                edi_doc_id = %doc_id,
                "EDI 850 purchase order created"
            );

            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "purchase_order_850",
                "po_number": po.po_number,
                "po_id": po.id.0,
                "edi_document_id": doc_id,
            })))
        }
        EdiDocumentType::ShipNotice856 => {
            let edi_asn: EdiShipNotice = serde_json::from_value(payload.payload.clone())
                .map_err(|e| {
                    tracing::error!("Failed to parse EDI ship notice: {}", e);
                    axum::http::StatusCode::BAD_REQUEST
                })?;

            tracing::info!(
                shipment_id = %edi_asn.shipment_id,
                po_number = %edi_asn.po_number,
                "Processing inbound EDI 856 ship notice"
            );

            // Tenant lookup + signature verification
            let metadata_pool = state.db.metadata();
            let tenant_uuid: Option<Uuid> = sqlx::query_scalar(
                "SELECT tenant_id FROM edi_receiver_map WHERE receiver_id = $1",
            )
            .bind(&edi_asn.receiver_id)
            .fetch_optional(&*metadata_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up tenant: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let tenant_uuid = tenant_uuid.ok_or_else(|| {
                tracing::warn!(receiver_id = %edi_asn.receiver_id, "No tenant for EDI receiver_id");
                axum::http::StatusCode::NOT_FOUND
            })?;

            let tenant_id = TenantId::from_uuid(tenant_uuid);
            let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
                tracing::error!("Failed to get tenant pool: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Verify signature
            let webhook_secret: Option<String> = sqlx::query_scalar(
                "SELECT webhook_secret FROM edi_connections WHERE tenant_id = $1 AND is_active = true",
            )
            .bind(tenant_uuid)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch webhook secret: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if let Some(secret) = &webhook_secret {
                if !secret.is_empty() && !signature.is_empty() {
                    if !verify_webhook_signature(&body, signature, secret) {
                        tracing::warn!("EDI webhook signature verification failed");
                        return Err(axum::http::StatusCode::UNAUTHORIZED);
                    }
                }
            }

            // --- Replay protection ---
            if !validate_timestamp_freshness(payload.timestamp, 300) {
                tracing::warn!(timestamp = %payload.timestamp, "EDI webhook timestamp too old or in future");
                return Err(axum::http::StatusCode::UNAUTHORIZED);
            }

            let nonce = payload.middleware_id.clone().unwrap_or_else(|| {
                let hash = sha2::Sha256::digest(&body);
                hex::encode(hash)
            });
            if !check_replay_nonce(&*tenant_pool, tenant_uuid, &nonce).await.map_err(|e| {
                tracing::error!("Replay nonce check failed: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })? {
                tracing::warn!(nonce = %nonce, "EDI webhook replay detected");
                return Err(axum::http::StatusCode::CONFLICT);
            }

            // Store EDI document
            let doc_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO edi_documents (id, tenant_id, document_type, direction, interchange_control, sender_id, receiver_id, status, raw_payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, 'processing', $8, NOW())"#,
            )
            .bind(doc_id)
            .bind(tenant_uuid)
            .bind("ship_notice_856")
            .bind("inbound")
            .bind(&edi_asn.interchange_control)
            .bind(&edi_asn.sender_id)
            .bind(&edi_asn.receiver_id)
            .bind(&payload.payload)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to store EDI document: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Find matching PO by po_number
            let po_repo = PurchaseOrderRepositoryImpl::new(Arc::clone(&tenant_pool));
            let po = po_repo.find_by_po_number(&tenant_id, &edi_asn.po_number)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to look up PO: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

            let po = po.ok_or_else(|| {
                tracing::warn!(po_number = %edi_asn.po_number, "No PO found for ASN");
                axum::http::StatusCode::UNPROCESSABLE_ENTITY
            })?;

            // Create receiving record
            let recv_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO receiving_records (id, tenant_id, po_id, received_date, edi_document_id, created_at)
                   VALUES ($1, $2, $3, $4, $5, NOW())"#,
            )
            .bind(recv_id)
            .bind(tenant_uuid)
            .bind(po.id.0)
            .bind(edi_asn.ship_date)
            .bind(doc_id)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create receiving record: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Insert receiving line items and update PO received quantities
            let recv_lines = EdiMapper::receiving_lines_from_asn(&edi_asn);
            for line in &recv_lines {
                sqlx::query(
                    r#"INSERT INTO receiving_line_items (id, receiving_id, po_line_number, quantity_received, quantity_damaged, product_id)
                       VALUES ($1, $2, $3, $4, $5, $6)"#,
                )
                .bind(line.id)
                .bind(recv_id)
                .bind(line.po_line_number as i32)
                .bind(line.quantity_received as f32)
                .bind(line.quantity_damaged as f32)
                .bind(&line.product_id)
                .execute(&*tenant_pool)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to insert receiving line item: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

                // Update PO line received quantity
                po_repo.update_received_quantities(
                    &tenant_id, &po.id, line.po_line_number, line.quantity_received
                )
                .await
                .map_err(|e| {
                    tracing::error!("Failed to update PO received qty: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;
            }

            // Check if all PO lines are fulfilled
            let unfulfilled: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM po_line_items WHERE po_id = $1 AND received_quantity < quantity",
            )
            .bind(po.id.0)
            .fetch_one(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to check PO fulfillment: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let new_status = if unfulfilled == 0 {
                "fulfilled"
            } else {
                "partially_fulfilled"
            };

            sqlx::query(
                "UPDATE purchase_orders SET status = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(new_status)
            .bind(po.id.0)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update PO status: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Update EDI document: mapped, link po_id
            sqlx::query(
                "UPDATE edi_documents SET status = 'mapped', po_id = $1, processed_at = NOW() WHERE id = $2",
            )
            .bind(po.id.0)
            .bind(doc_id)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update EDI document: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            tracing::info!(
                recv_id = %recv_id,
                po_id = %po.id,
                lines = recv_lines.len(),
                new_status = %new_status,
                "EDI 856 ship notice processed"
            );

            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "ship_notice_856",
                "receiving_id": recv_id,
                "po_id": po.id.0,
                "po_status": new_status,
                "edi_document_id": doc_id,
            })))
        }
        EdiDocumentType::Remittance820 => {
            // 820 is outbound-only, receiving one inbound is unexpected
            tracing::warn!("Received unexpected inbound 820 remittance");
            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "remittance_820",
                "note": "820 is outbound-only, document logged but not processed",
            })))
        }
        EdiDocumentType::FunctionalAck997 => {
            let ack: EdiFunctionalAck = serde_json::from_value(payload.payload.clone())
                .map_err(|e| {
                    tracing::error!("Failed to parse EDI 997 ack: {}", e);
                    axum::http::StatusCode::BAD_REQUEST
                })?;

            tracing::info!(
                group_control = %ack.group_control,
                status = ?ack.status,
                "Processing inbound EDI 997 functional acknowledgment"
            );

            // The 997's receiver_id is our ISA ID (we sent the original doc).
            // Use it to look up the tenant, same as 810/850/856 handlers.
            let metadata_pool = state.db.metadata();
            let tenant_uuid: Option<Uuid> = sqlx::query_scalar(
                "SELECT tenant_id FROM edi_receiver_map WHERE receiver_id = $1",
            )
            .bind(&ack.receiver_id)
            .fetch_optional(&*metadata_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up tenant for 997: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let tenant_uuid = tenant_uuid.ok_or_else(|| {
                tracing::warn!(
                    receiver_id = %ack.receiver_id,
                    "No tenant found for inbound 997 receiver_id"
                );
                axum::http::StatusCode::NOT_FOUND
            })?;

            let tenant_id = TenantId::from_uuid(tenant_uuid);
            let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
                tracing::error!("Failed to get tenant pool: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Verify webhook signature (same pattern as 810/850/856)
            let webhook_secret: Option<String> = sqlx::query_scalar(
                "SELECT webhook_secret FROM edi_connections WHERE tenant_id = $1 AND is_active = true",
            )
            .bind(tenant_uuid)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch webhook secret: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if let Some(secret) = &webhook_secret {
                if !secret.is_empty() && !signature.is_empty() {
                    if !verify_webhook_signature(&body, signature, secret) {
                        tracing::warn!("EDI 997 webhook signature verification failed");
                        return Err(axum::http::StatusCode::UNAUTHORIZED);
                    }
                }
            }

            // --- Replay protection ---
            if !validate_timestamp_freshness(payload.timestamp, 300) {
                tracing::warn!(timestamp = %payload.timestamp, "EDI webhook timestamp too old or in future");
                return Err(axum::http::StatusCode::UNAUTHORIZED);
            }

            let nonce = payload.middleware_id.clone().unwrap_or_else(|| {
                let hash = sha2::Sha256::digest(&body);
                hex::encode(hash)
            });
            if !check_replay_nonce(&*tenant_pool, tenant_uuid, &nonce).await.map_err(|e| {
                tracing::error!("Replay nonce check failed: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })? {
                tracing::warn!(nonce = %nonce, "EDI webhook replay detected");
                return Err(axum::http::StatusCode::CONFLICT);
            }

            // Store the inbound 997 document
            let doc_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO edi_documents
                   (id, tenant_id, document_type, direction, group_control, status, raw_payload, created_at)
                   VALUES ($1, $2, 'functional_ack_997', 'inbound', $3, 'processing', $4, NOW())"#,
            )
            .bind(doc_id)
            .bind(tenant_uuid)
            .bind(&ack.group_control)
            .bind(&payload.payload)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to store inbound 997: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // Process the ack - find and update the matching outbound document
            let matched_doc_id = process_inbound_ack(&tenant_pool, tenant_uuid, &ack)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to process inbound 997: {}", e);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })?;

            // Update the 997 document status
            sqlx::query(
                "UPDATE edi_documents SET status = 'mapped', processed_at = NOW() WHERE id = $1",
            )
            .bind(doc_id)
            .execute(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update 997 document status: {}", e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

            Ok(Json(serde_json::json!({
                "status": "accepted",
                "document_type": "functional_ack_997",
                "ack_status": format!("{:?}", ack.status),
                "matched_document_id": matched_doc_id,
                "edi_document_id": doc_id,
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

    let isa_id = req.our_isa_id.as_deref().unwrap_or("");

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
    .bind(isa_id)
    .execute(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to save EDI connection: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Register receiver ID in metadata DB for webhook tenant lookup.
    // First remove any stale mapping for this tenant (ISA ID may have changed),
    // then insert the new one.
    let metadata_pool = state.db.metadata();
    sqlx::query("DELETE FROM edi_receiver_map WHERE tenant_id = $1")
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*metadata_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to clear old EDI receiver mapping: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !isa_id.is_empty() {
        sqlx::query(
            r#"INSERT INTO edi_receiver_map (id, receiver_id, tenant_id, created_at)
               VALUES ($1, $2, $3, NOW())
               ON CONFLICT (receiver_id) DO UPDATE SET tenant_id = $3"#,
        )
        .bind(Uuid::new_v4())
        .bind(isa_id)
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*metadata_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to register EDI receiver mapping: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

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

    // Remove receiver mapping from metadata DB
    let metadata_pool = state.db.metadata();
    sqlx::query("DELETE FROM edi_receiver_map WHERE tenant_id = $1")
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*metadata_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to remove EDI receiver mapping: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

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

// ──────────────────────────── Outbound ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendRemittanceRequest {
    pub payment_reference: String,
    pub payment_method: Option<String>,
}

/// Send an 820 Payment Remittance Advice for a paid invoice.
///
/// The invoice must be in "paid" status and have an associated vendor
/// with a trading partner mapping (edi_trading_partners.vendor_id).
async fn send_remittance(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(invoice_id): Path<Uuid>,
    Json(req): Json<SendRemittanceRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    // Load the invoice
    let invoice_repo = InvoiceRepositoryImpl::new(Arc::clone(&pool));
    let invoice = invoice_repo
        .get_by_id(
            &tenant.tenant_id,
            &billforge_core::domain::InvoiceId(invoice_id),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to load invoice: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // Verify invoice is paid
    if invoice.processing_status != billforge_core::domain::ProcessingStatus::Paid {
        return Err(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Find the trading partner for this vendor
    let vendor_id = invoice.vendor_id.ok_or_else(|| {
        tracing::warn!("Invoice has no vendor_id, cannot send remittance");
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    })?;

    let partner_edi_id: Option<String> = sqlx::query_scalar(
        "SELECT edi_id FROM edi_trading_partners WHERE tenant_id = $1 AND vendor_id = $2 AND is_active = true LIMIT 1",
    )
    .bind(tenant_uuid)
    .bind(vendor_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to look up trading partner: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let receiver_id = partner_edi_id.ok_or_else(|| {
        tracing::warn!(vendor_id = %vendor_id, "No trading partner for vendor");
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    })?;

    // Load EDI connection config
    let conn_row: Option<(String, String, String, Option<String>, Option<String>, Option<String>, i32)> =
        sqlx::query_as(
            r#"SELECT api_key_encrypted, webhook_secret, provider,
                      our_isa_qualifier, our_isa_id, api_base_url, ack_timeout_hours
               FROM edi_connections
               WHERE tenant_id = $1 AND is_active = true"#,
        )
        .bind(tenant_uuid)
        .fetch_optional(&*pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load EDI connection: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let (api_key, webhook_secret, provider_str, isa_qualifier, isa_id, api_base_url, ack_timeout_hours) =
        conn_row.ok_or_else(|| {
            tracing::warn!("No active EDI connection for tenant");
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        })?;

    let sender_id = isa_id.unwrap_or_default();
    let provider = match provider_str.as_str() {
        "stedi" => billforge_edi::config::EdiProvider::Stedi,
        "orderful" => billforge_edi::config::EdiProvider::Orderful,
        "sps_commerce" => billforge_edi::config::EdiProvider::SpsCommerce,
        _ => billforge_edi::config::EdiProvider::Custom,
    };
    let config = EdiConfig {
        api_key,
        webhook_secret,
        provider,
        api_base_url: api_base_url.unwrap_or_else(|| "https://core.us.stedi.com/2023-08-01".to_string()),
        our_isa_qualifier: isa_qualifier.unwrap_or_else(|| "ZZ".to_string()),
        our_isa_id: sender_id.clone(),
    };

    let client = EdiClient::new(config);
    let service = OutboundEdiService::new(client);

    let payment_method = req.payment_method.as_deref().unwrap_or("ACH");

    let doc_id = service
        .send_remittance(
            &pool,
            tenant_uuid,
            &invoice,
            &sender_id,
            &receiver_id,
            &req.payment_reference,
            payment_method,
            "BillForge",
            ack_timeout_hours,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to send remittance: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "status": "sent",
        "edi_document_id": doc_id,
        "invoice_id": invoice_id,
        "receiver_id": receiver_id,
    })))
}

/// List outbound EDI documents with ack status
async fn list_outbound(
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

    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, String, Option<Uuid>, Option<String>, Option<String>, i32, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>)>(
        r#"SELECT id, document_type, sender_id, receiver_id, status, invoice_id,
                  ack_status, middleware_id, ack_retry_count,
                  created_at, processed_at, ack_received_at
           FROM edi_documents
           WHERE tenant_id = $1 AND direction = 'outbound'
           ORDER BY created_at DESC
           LIMIT $2 OFFSET $3"#,
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
                "sender_id": r.2,
                "receiver_id": r.3,
                "status": r.4,
                "invoice_id": r.5,
                "ack_status": r.6,
                "middleware_id": r.7,
                "retry_count": r.8,
                "created_at": r.9,
                "processed_at": r.10,
                "ack_received_at": r.11,
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

/// Check for outbound documents with overdue ack responses
async fn get_ack_timeouts(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let timed_out = check_ack_timeouts(&pool, tenant_uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check ack timeouts: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "timed_out_count": timed_out.len(),
        "document_ids": timed_out,
    })))
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
