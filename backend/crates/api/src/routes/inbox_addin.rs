//! Inbox-Native AP add-in routes.
//!
//! Surfaces a Gmail / Outlook add-in (#406) with a JSON contract that turns the
//! active email message in an approver's inbox into a full BillForge AP triage
//! surface: invoice preview metadata, line items, GL coding, vendor history,
//! policy warnings, comments, and one-click approve/reject. Falls back to an
//! `ingest-attachment` action that pushes the message's attachment into the
//! shared inbound-email intake pipeline when no matching invoice exists.
//!
//! All routes authenticate via the standard JWT bearer extractor (the same
//! token the dashboard uses, fetched via `getAddinToken` on the client) and
//! derive the tenant from the extractor, never from the request body.
//!
//! The add-in surface is gated on the InvoiceProcessing module — the same
//! entitlement that backs the in-app inbox.

use crate::error::{ApiError, ApiResult};
use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;
use crate::state_machine::{transition, InvoiceStatus};
use axum::{
    extract::{Multipart, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ADDIN_SOURCE: &str = "outlook_addin";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/lookup", get(lookup))
        .route("/approve", post(approve))
        .route("/reject", post(reject))
        .route("/ingest-attachment", post(ingest_attachment))
}

// ---------------------------------------------------------------------------
// GET /addin/lookup
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    pub message_id: Option<String>,
    pub from_address: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LookupResponse {
    pub invoice_id: Uuid,
    pub vendor: VendorSummary,
    pub totals: InvoiceTotals,
    pub line_items: Vec<LineItem>,
    pub gl_coding: Vec<GlCodingEntry>,
    pub policy_warnings: Vec<PolicyWarning>,
    pub vendor_history_summary: VendorHistorySummary,
    pub comments: Vec<Comment>,
    pub approval_state: String,
}

#[derive(Debug, Serialize)]
pub struct VendorSummary {
    pub id: Option<Uuid>,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvoiceTotals {
    pub subtotal_cents: Option<i64>,
    pub tax_amount_cents: Option<i64>,
    pub total_amount_cents: i64,
    pub currency: String,
    pub invoice_number: String,
    pub invoice_date: Option<String>,
    pub due_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LineItem {
    pub description: Option<String>,
    pub quantity: Option<f64>,
    pub unit_price_cents: Option<i64>,
    pub total_cents: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct GlCodingEntry {
    pub gl_code: Option<String>,
    pub department: Option<String>,
    pub cost_center: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PolicyWarning {
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct VendorHistorySummary {
    pub invoice_count_last_90d: i64,
    pub total_spend_cents_last_90d: i64,
    pub last_invoice_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

async fn lookup(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Query(query): Query<LookupQuery>,
) -> ApiResult<Json<LookupResponse>> {
    let tenant_id = tenant.tenant_id.clone();
    let tenant_uuid = *tenant_id.as_uuid();
    let pool = state.db.tenant(&tenant_id).await?;
    let metadata_pool = state.db.metadata();

    // 1. Try resolving by inbound message_id (links to inbound_email_messages
    //    in the metadata DB, then to invoices via invoices.source_email_id in
    //    the tenant DB).
    let mut invoice_id: Option<Uuid> = None;

    if let Some(message_id) = query.message_id.as_deref().filter(|s| !s.is_empty()) {
        let inbound_row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM inbound_email_messages \
             WHERE tenant_id = $1 AND message_id = $2 \
             ORDER BY received_at DESC LIMIT 1",
        )
        .bind(tenant_uuid)
        .bind(message_id)
        .fetch_optional(&*metadata_pool)
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

        if let Some((email_id,)) = inbound_row {
            let inv: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM invoices \
                 WHERE tenant_id = $1 AND source_email_id = $2 \
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(tenant_uuid)
            .bind(email_id)
            .fetch_optional(&*pool)
            .await
            .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;
            invoice_id = inv.map(|r| r.0);
        }
    }

    // 2. Fall back to the most recent open invoice whose source vendor has the
    //    sender's email on file. Tenant isolation comes from the WHERE clause.
    if invoice_id.is_none() {
        if let Some(from) = query.from_address.as_deref().filter(|s| !s.is_empty()) {
            let inv: Option<(Uuid,)> = sqlx::query_as(
                "SELECT i.id FROM invoices i \
                 LEFT JOIN vendors v ON v.id = i.vendor_id AND v.tenant_id = i.tenant_id \
                 WHERE i.tenant_id = $1 \
                   AND i.status IN ('received','in_review','pending_approval') \
                   AND v.contact_email = $2 \
                 ORDER BY i.created_at DESC LIMIT 1",
            )
            .bind(tenant_uuid)
            .bind(from)
            .fetch_optional(&*pool)
            .await
            .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;
            invoice_id = inv.map(|r| r.0);
        }
    }

    let invoice_id = invoice_id.ok_or_else(|| {
        ApiError(billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: query
                .message_id
                .clone()
                .or_else(|| query.from_address.clone())
                .unwrap_or_else(|| "unknown".to_string()),
        })
    })?;

    // 3. Hydrate the response from the invoice row.
    #[allow(clippy::type_complexity)]
    let invoice_row: (
        Uuid,
        Option<Uuid>,
        String,
        String,
        Option<i64>,
        Option<i64>,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<serde_json::Value>,
        String,
    ) = sqlx::query_as(
        "SELECT i.id, i.vendor_id, i.vendor_name, i.invoice_number, \
                i.subtotal_cents, i.tax_amount_cents, i.total_amount_cents, i.currency, \
                i.invoice_date::text, i.due_date::text, \
                i.gl_code, i.department, i.cost_center, \
                i.line_items, COALESCE(i.status, 'received') \
         FROM invoices i \
         WHERE i.id = $1 AND i.tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_uuid)
    .fetch_one(&*pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => ApiError(billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: invoice_id.to_string(),
        }),
        other => ApiError(billforge_core::Error::Database(other.to_string())),
    })?;

    let (
        _id,
        vendor_id,
        vendor_name,
        invoice_number,
        subtotal_cents,
        tax_amount_cents,
        total_amount_cents,
        currency,
        invoice_date,
        due_date,
        gl_code,
        department,
        cost_center,
        line_items_json,
        approval_state,
    ) = invoice_row;

    // Vendor email (best-effort)
    let vendor_email: Option<String> = if let Some(vid) = vendor_id {
        sqlx::query_scalar(
            "SELECT contact_email FROM vendors WHERE id = $1 AND tenant_id = $2",
        )
        .bind(vid)
        .bind(tenant_uuid)
        .fetch_optional(&*pool)
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?
        .flatten()
    } else {
        None
    };

    let line_items = line_items_from_json(line_items_json);

    // Policy warnings: surface budget guardrail signal as the primary warning.
    let policy_warnings = match crate::routes::budgets::check_invoice_against_budgets(
        &pool,
        tenant_uuid,
        invoice_id,
    )
    .await
    {
        Ok(check) => {
            let mut out: Vec<PolicyWarning> = Vec::new();
            for v in check.violations {
                out.push(PolicyWarning {
                    severity: "error".to_string(),
                    message: serde_json::to_string(&v).unwrap_or_else(|_| "budget violation".into()),
                });
            }
            for w in check.warnings {
                out.push(PolicyWarning {
                    severity: "warning".to_string(),
                    message: serde_json::to_string(&w).unwrap_or_else(|_| "budget warning".into()),
                });
            }
            out
        }
        Err(_) => Vec::new(),
    };

    // Vendor history (rolling 90 days).
    let history_row: Option<(Option<i64>, Option<i64>, Option<String>)> = if let Some(vid) =
        vendor_id
    {
        sqlx::query_as(
            "SELECT COUNT(*)::BIGINT, COALESCE(SUM(total_amount_cents), 0)::BIGINT, \
                    MAX(invoice_date)::text \
             FROM invoices \
             WHERE tenant_id = $1 AND vendor_id = $2 \
               AND created_at >= NOW() - INTERVAL '90 days'",
        )
        .bind(tenant_uuid)
        .bind(vid)
        .fetch_optional(&*pool)
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?
    } else {
        None
    };

    let vendor_history_summary = if let Some((count, spend, last_date)) = history_row {
        VendorHistorySummary {
            invoice_count_last_90d: count.unwrap_or(0),
            total_spend_cents_last_90d: spend.unwrap_or(0),
            last_invoice_date: last_date,
        }
    } else {
        VendorHistorySummary {
            invoice_count_last_90d: 0,
            total_spend_cents_last_90d: 0,
            last_invoice_date: None,
        }
    };

    // Comment thread: surface the recent comment-style audit rows.
    let comment_rows: Vec<(Option<Uuid>, serde_json::Value, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            "SELECT actor_id, metadata, created_at FROM invoice_audit_log \
             WHERE tenant_id = $1 AND invoice_id = $2 \
               AND event_type LIKE '%comment%' \
             ORDER BY created_at DESC LIMIT 20",
        )
        .bind(tenant_uuid)
        .bind(invoice_id)
        .fetch_all(&*pool)
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    let mut comments = Vec::with_capacity(comment_rows.len());
    for (actor_id, metadata, created_at) in comment_rows {
        let body = metadata
            .get("comment_body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let author = actor_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "system".to_string());
        comments.push(Comment {
            author,
            body,
            created_at: created_at.to_rfc3339(),
        });
    }

    Ok(Json(LookupResponse {
        invoice_id,
        vendor: VendorSummary {
            id: vendor_id,
            name: vendor_name,
            email: vendor_email,
        },
        totals: InvoiceTotals {
            subtotal_cents,
            tax_amount_cents,
            total_amount_cents,
            currency,
            invoice_number,
            invoice_date,
            due_date,
        },
        line_items,
        gl_coding: vec![GlCodingEntry {
            gl_code,
            department,
            cost_center,
        }],
        policy_warnings,
        vendor_history_summary,
        comments,
        approval_state,
    }))
}

fn line_items_from_json(value: Option<serde_json::Value>) -> Vec<LineItem> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .into_iter()
        .map(|item| LineItem {
            description: item
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            quantity: item.get("quantity").and_then(|v| v.as_f64()),
            unit_price_cents: item
                .get("unit_price_cents")
                .and_then(|v| v.as_i64())
                .or_else(|| item.get("unit_price").and_then(|v| v.as_i64())),
            total_cents: item
                .get("total_cents")
                .and_then(|v| v.as_i64())
                .or_else(|| item.get("total").and_then(|v| v.as_i64())),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// POST /addin/approve and /addin/reject
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub invoice_id: Uuid,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub invoice_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct DecisionResponse {
    pub invoice_id: Uuid,
    pub new_status: String,
}

async fn approve(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(req): Json<ApproveRequest>,
) -> ApiResult<Json<DecisionResponse>> {
    let tenant_id = tenant.tenant_id.clone();
    let pool = state.db.tenant(&tenant_id).await?;
    let actor_id = *user.user_id.as_uuid();

    // Budget guardrail check (mirrors approval_links / chat_approvals).
    let budget_check = crate::routes::budgets::check_invoice_against_budgets(
        &pool,
        *tenant_id.as_uuid(),
        req.invoice_id,
    )
    .await?;

    if budget_check.blocked {
        return Err(ApiError(billforge_core::Error::Conflict(format!(
            "BUDGET_EXCEEDED: {}",
            serde_json::to_string(&budget_check.violations)
                .unwrap_or_else(|_| "budget exceeded".to_string())
        ))));
    }

    transition(
        &pool,
        &tenant_id,
        &req.invoice_id,
        &actor_id,
        InvoiceStatus::Approved,
        "approve_via_outlook_addin",
        serde_json::json!({
            "channel": ADDIN_SOURCE,
            "source": ADDIN_SOURCE,
            "comment": req.comment,
        }),
    )
    .await
    .map_err(ApiError)?;

    super::email_actions::update_approval_request(
        &pool,
        &tenant_id,
        req.invoice_id,
        &user.user_id,
        "approved",
    )
    .await
    .map_err(ApiError)?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;
    super::workflows::resolve_invoice_approval_status(
        &mut conn,
        Some(&state.db.metadata()),
        &tenant_id,
        req.invoice_id,
    )
    .await
    .map_err(ApiError)?;

    if let Some(body) = req.comment.as_deref().filter(|s| !s.is_empty()) {
        record_addin_comment(&pool, *tenant_id.as_uuid(), req.invoice_id, actor_id, body)
            .await?;
    }

    Ok(Json(DecisionResponse {
        invoice_id: req.invoice_id,
        new_status: "approved".to_string(),
    }))
}

async fn reject(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(req): Json<RejectRequest>,
) -> ApiResult<Json<DecisionResponse>> {
    let tenant_id = tenant.tenant_id.clone();
    let pool = state.db.tenant(&tenant_id).await?;
    let actor_id = *user.user_id.as_uuid();

    transition(
        &pool,
        &tenant_id,
        &req.invoice_id,
        &actor_id,
        InvoiceStatus::Rejected,
        "reject_via_outlook_addin",
        serde_json::json!({
            "channel": ADDIN_SOURCE,
            "source": ADDIN_SOURCE,
            "reason": req.reason,
        }),
    )
    .await
    .map_err(ApiError)?;

    super::email_actions::update_approval_request(
        &pool,
        &tenant_id,
        req.invoice_id,
        &user.user_id,
        "rejected",
    )
    .await
    .map_err(ApiError)?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;
    super::workflows::resolve_invoice_approval_status(
        &mut conn,
        Some(&state.db.metadata()),
        &tenant_id,
        req.invoice_id,
    )
    .await
    .map_err(ApiError)?;

    Ok(Json(DecisionResponse {
        invoice_id: req.invoice_id,
        new_status: "rejected".to_string(),
    }))
}

async fn record_addin_comment(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    invoice_id: Uuid,
    actor_id: Uuid,
    body: &str,
) -> ApiResult<()> {
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type,
                metadata, source_channel)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval',
                   'comment_via_outlook_addin', $5, $6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(actor_id)
    .bind(serde_json::json!({
        "comment_body": body,
        "channel": ADDIN_SOURCE,
    }))
    .bind(ADDIN_SOURCE)
    .execute(pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// POST /addin/ingest-attachment
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct IngestAttachmentResponse {
    pub intake_id: Uuid,
    pub invoice_id: Uuid,
    pub queue_position: i64,
}

async fn ingest_attachment(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    mut multipart: Multipart,
) -> ApiResult<Json<IngestAttachmentResponse>> {
    let tenant_id = tenant.tenant_id.clone();
    let tenant_uuid = *tenant_id.as_uuid();
    let pool = state.db.tenant(&tenant_id).await?;
    let metadata_pool = state.db.metadata();

    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;
    let mut source_message_id: Option<String> = None;
    let mut from_address: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(billforge_core::Error::Validation(format!(
            "Invalid multipart payload: {}",
            e
        )))
    })? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "bytes" | "file" => {
                if filename.is_none() {
                    filename = field.file_name().map(|s| s.to_string());
                }
                if content_type.is_none() {
                    content_type = field.content_type().map(|s| s.to_string());
                }
                let data = field.bytes().await.map_err(|e| {
                    ApiError(billforge_core::Error::Validation(format!(
                        "Failed to read attachment bytes: {}",
                        e
                    )))
                })?;
                bytes = Some(data.to_vec());
            }
            "filename" => {
                filename = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError(billforge_core::Error::Validation(e.to_string())))?,
                );
            }
            "content_type" => {
                content_type = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError(billforge_core::Error::Validation(e.to_string())))?,
                );
            }
            "source_message_id" => {
                source_message_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError(billforge_core::Error::Validation(e.to_string())))?,
                );
            }
            "from_address" => {
                from_address = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError(billforge_core::Error::Validation(e.to_string())))?,
                );
            }
            _ => {}
        }
    }

    let filename = filename.unwrap_or_else(|| format!("outlook-{}.pdf", Uuid::new_v4().as_simple()));
    let content_type =
        content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let bytes = bytes.ok_or_else(|| {
        ApiError(billforge_core::Error::Validation(
            "Missing attachment bytes".to_string(),
        ))
    })?;
    let from_address = from_address.unwrap_or_default();

    // 1. Record an inbound_email_messages row tagged with the add-in source so
    //    downstream observability can attribute the intake.
    let inbound_email_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO inbound_email_messages
               (tenant_id, message_id, from_address, from_domain, subject, status, raw_payload)
           VALUES ($1, $2, $3, $4, $5, 'processed', $6)
           RETURNING id"#,
    )
    .bind(tenant_uuid)
    .bind(source_message_id.as_deref().unwrap_or(""))
    .bind(&from_address)
    .bind(extract_domain(&from_address))
    .bind(format!("Outlook add-in: {}", filename))
    .bind(serde_json::json!({
        "source": ADDIN_SOURCE,
        "actor_user_id": user.user_id.as_uuid().to_string(),
        "source_message_id": source_message_id,
        "from_address": from_address,
        "filename": filename,
    }))
    .fetch_one(&*metadata_pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    // 2. Create capture + invoice in the tenant DB so the existing OCR pipeline
    //    picks up the document. Mirrors `inbound_email::handle_inbound_email`.
    let document_id = Uuid::new_v4();
    let _capture_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO invoice_captures
               (id, tenant_id, original_filename, mime_type, provider, status, uploaded_by)
           VALUES ($1, $2, $3, $4, $5, 'processing', $6)
           RETURNING id"#,
    )
    .bind(document_id)
    .bind(tenant_uuid)
    .bind(&filename)
    .bind(&content_type)
    .bind(ADDIN_SOURCE)
    .bind(user.user_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    let invoice_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO invoices
               (id, tenant_id, vendor_id, vendor_name, invoice_number,
                total_amount_cents, currency, capture_status, processing_status,
                document_id, source_email_id, created_by, created_at, updated_at)
           VALUES ($1, $2, NULL, $3, $4, 0, 'USD', 'processing', 'draft',
                   $5, $6, $7, NOW(), NOW())
           RETURNING id"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .bind(if from_address.is_empty() {
        "Unknown (Outlook add-in)".to_string()
    } else {
        format!("Unknown ({})", extract_domain(&from_address))
    })
    .bind(format!("ADDIN-{}", Uuid::new_v4().as_simple()))
    .bind(document_id)
    .bind(inbound_email_id)
    .bind(user.user_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    // 3. Persist attachment bytes to local storage. Mirrors the inbound webhook.
    let storage_path =
        std::env::var("LOCAL_STORAGE_PATH").unwrap_or_else(|_| "./data/files".to_string());
    let dir = std::path::Path::new(&storage_path)
        .join(tenant_uuid.to_string())
        .join("documents");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "Failed to create storage dir for add-in attachment");
    }
    let file_path = dir.join(document_id.to_string());
    if let Err(e) = std::fs::write(&file_path, &bytes) {
        tracing::warn!(error = %e, document_id = %document_id, "Failed to store add-in attachment");
    }

    // 4. Enqueue the OCR job (best-effort) and report queue depth.
    let mut queue_position: i64 = 0;
    if let Some(redis_client) = &state.redis {
        let job_payload = serde_json::json!({
            "invoice_id": invoice_id.to_string(),
            "document_id": document_id.to_string(),
            "content_type": content_type,
            "source": ADDIN_SOURCE,
        });
        let queue_key = format!("billforge:jobs:{}:ocr_processing", tenant_uuid);
        if let Ok(mut conn) = redis_client.get_connection() {
            let _ = redis::cmd("RPUSH")
                .arg(&queue_key)
                .arg(job_payload.to_string())
                .query::<i64>(&mut conn);
            if let Ok(depth) = redis::cmd("LLEN").arg(&queue_key).query::<i64>(&mut conn) {
                queue_position = depth;
            }
        }
    }

    Ok(Json(IngestAttachmentResponse {
        intake_id: inbound_email_id,
        invoice_id,
        queue_position,
    }))
}

fn extract_domain(email: &str) -> String {
    email
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_string())
        .unwrap_or_default()
}
