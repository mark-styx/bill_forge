//! Inbound email webhook endpoint.
//!
//! `POST /webhooks/inbound-email` — authenticated via shared secret header
//! (`INBOUND_EMAIL_WEBHOOK_SECRET` env var). Parses a provider-agnostic
//! inbound-parse payload, invokes `InboundEmailHandler`, and returns 200 on
//! accept (even for triage cases — providers retry on 5xx).
//!
//! Database routing:
//! - `inbound_email_messages` and `email_triage_queue` live in the **metadata** DB
//!   (they FK to `tenants(id)` which only exists there).
//! - `invoice_captures`, `invoices`, and `vendors` live in the **tenant** DB.

use crate::state::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/inbound-email", axum::routing::post(handle_inbound_email))
}

/// Shared-secret header name expected from the inbound email provider.
const SECRET_HEADER: &str = "x-inbound-email-secret";

#[derive(serde::Serialize)]
struct InboundEmailResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    capture_ids: Vec<uuid::Uuid>,
}

async fn handle_inbound_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<billforge_email::InboundEmailPayload>,
) -> Result<(StatusCode, Json<InboundEmailResponse>), StatusCode> {
    // 1. Validate shared secret (fail-closed: reject if env var is missing or empty)
    let expected_secret = match std::env::var("INBOUND_EMAIL_WEBHOOK_SECRET") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            tracing::warn!(
                "Inbound email webhook rejected: INBOUND_EMAIL_WEBHOOK_SECRET not configured"
            );
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
    };

    let provided = headers
        .get(SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected_secret {
        tracing::warn!(
            provided_secret = provided.len(),
            "Inbound email webhook rejected: invalid secret"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // 2. Resolve recipient → tenant via metadata DB
    let metadata_pool = state.db.metadata();

    let to_email = billforge_email::extract_email(&payload.to);
    let tenant_uuid: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT tenant_id FROM tenant_forwarding_addresses WHERE full_address = $1",
    )
    .bind(to_email)
    .fetch_optional(&*metadata_pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to resolve tenant for inbound email");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let tenant_uuid = match tenant_uuid {
        Some(id) => id,
        None => {
            tracing::warn!(
                to = %payload.to,
                "Inbound email for unknown recipient; no tenant found"
            );
            return Ok((
                StatusCode::OK,
                Json(InboundEmailResponse {
                    status: "triage".to_string(),
                    reason: Some("No tenant found for recipient address".to_string()),
                    capture_ids: Vec::new(),
                }),
            ));
        }
    };

    let tenant_id = billforge_core::TenantId::from_uuid(tenant_uuid);
    let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to get tenant pool");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 3. Persist inbound_email_messages row in the METADATA DB
    let from_domain = billforge_email::extract_domain(&payload.from);

    let email_id: uuid::Uuid = sqlx::query_scalar(
        r#"INSERT INTO inbound_email_messages
               (tenant_id, message_id, from_address, from_domain, subject, status, raw_payload)
           VALUES ($1, $2, $3, $4, $5, 'processed', $6)
           RETURNING id"#,
    )
    .bind(tenant_uuid)
    .bind(&payload.message_id)
    .bind(&payload.from)
    .bind(&from_domain)
    .bind(&payload.subject)
    .bind(serde_json::to_value(&payload).ok())
    .fetch_one(&*metadata_pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to persist inbound email message");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 4. Filter usable attachments (PDF or image)
    let usable: Vec<&billforge_email::InboundAttachment> = payload
        .attachments
        .iter()
        .filter(|a| billforge_email::is_usable_attachment(&a.content_type))
        .collect();

    if usable.is_empty() {
        // No usable attachments → triage (METADATA DB)
        sqlx::query(
            r#"INSERT INTO email_triage_queue (id, inbound_email_id, reason)
               VALUES (gen_random_uuid(), $1, $2)"#,
        )
        .bind(email_id)
        .bind("No usable PDF/image attachments found")
        .execute(&*metadata_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create triage entry");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query(
            "UPDATE inbound_email_messages SET status = 'triage', triage_reason = $2 WHERE id = $1",
        )
        .bind(email_id)
        .bind("No usable PDF/image attachments found")
        .execute(&*metadata_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update email status");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        return Ok((
            StatusCode::OK,
            Json(InboundEmailResponse {
                status: "triage".to_string(),
                reason: Some("No usable PDF/image attachments found".to_string()),
                capture_ids: Vec::new(),
            }),
        ));
    }

    // 5. Suggest vendor from sender domain (TENANT DB — vendors table)
    let suggested_vendor_id: Option<uuid::Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM vendors
           WHERE tenant_id = $1
             AND status = 'active'
             AND (email LIKE $2 OR email LIKE $3)
           LIMIT 1"#,
    )
    .bind(tenant_uuid)
    .bind(format!("%@{}", from_domain))
    .bind(format!("%@%.{}", from_domain))
    .fetch_optional(&*tenant_pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to match vendor by domain");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 6. For each usable attachment: create capture + invoice in TENANT DB
    let mut capture_ids = Vec::new();
    for attachment in &usable {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let bytes = match BASE64.decode(&attachment.content) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    attachment = %attachment.name,
                    error = %e,
                    "Failed to decode attachment"
                );
                continue;
            }
        };

        let document_id = uuid::Uuid::new_v4();

        // Create invoice capture row (TENANT DB)
        let _capture_id: uuid::Uuid = sqlx::query_scalar(
            r#"INSERT INTO invoice_captures
                   (id, tenant_id, original_filename, mime_type, provider, status, uploaded_by)
               VALUES ($1, $2, $3, $4, 'tesseract', 'processing', NULL)
               RETURNING id"#,
        )
        .bind(document_id)
        .bind(tenant_uuid)
        .bind(&attachment.name)
        .bind(&attachment.content_type)
        .fetch_one(&*tenant_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create invoice capture");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Create invoice row with source_email_id and suggested vendor (TENANT DB)
        let invoice_id: uuid::Uuid = sqlx::query_scalar(
            r#"INSERT INTO invoices
                   (id, tenant_id, vendor_id, vendor_name, invoice_number,
                    total_amount_cents, currency, capture_status, processing_status,
                    document_id, source_email_id, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, 0, 'USD', 'processing', 'draft',
                       $6, $7, NOW(), NOW())
               RETURNING id"#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(tenant_uuid)
        .bind(suggested_vendor_id)
        .bind(
            suggested_vendor_id
                .map_or_else(|| "Unknown (email)".to_string(), |_| from_domain.clone()),
        )
        .bind(format!("EMAIL-{}", uuid::Uuid::new_v4().as_simple()))
        .bind(document_id)
        .bind(email_id)
        .fetch_one(&*tenant_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create invoice from email");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Store attachment bytes
        let storage_path =
            std::env::var("LOCAL_STORAGE_PATH").unwrap_or_else(|_| "./data/files".to_string());
        let dir = std::path::Path::new(&storage_path)
            .join(tenant_uuid.to_string())
            .join("documents");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, "Failed to create storage dir");
        }
        let file_path = dir.join(document_id.to_string());
        if let Err(e) = std::fs::write(&file_path, &bytes) {
            tracing::warn!(error = %e, document_id = %document_id, "Failed to store attachment");
        }

        // Enqueue OCR job via Redis
        if let Some(redis_client) = &state.redis {
            let job_payload = serde_json::json!({
                "invoice_id": invoice_id.to_string(),
                "document_id": document_id.to_string(),
                "content_type": attachment.content_type,
            });
            let queue_key = format!("billforge:jobs:{}:ocr_processing", tenant_uuid);
            match redis_client.get_connection() {
                Ok(mut conn) => {
                    redis::cmd("RPUSH")
                        .arg(&queue_key)
                        .arg(job_payload.to_string())
                        .execute(&mut conn);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to get Redis connection for OCR enqueue");
                }
            }
        }

        capture_ids.push(invoice_id);
    }

    Ok((
        StatusCode::OK,
        Json(InboundEmailResponse {
            status: "processed".to_string(),
            reason: None,
            capture_ids,
        }),
    ))
}
