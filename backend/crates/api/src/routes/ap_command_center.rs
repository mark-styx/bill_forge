//! AP Command Center — this-week cash obligations & bottleneck command view.
//!
//! Provides a single consolidated JSON payload for the AP manager's daily
//! standup: invoices due this week and next, who is blocking each, late-fee
//! risk $, and expiring early-payment discount $.
//!
//! Also provides session-authenticated `nudge` and `reassign` action endpoints
//! so the UI can trigger those inline without going through the Teams/Slack
//! chat-approval callback surface.

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use billforge_core::Error;
use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/this-week", get(get_this_week_command_center))
        .route("/{invoice_id}/nudge", post(nudge_approver))
        .route("/{invoice_id}/reassign", post(reassign_approver))
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level response payload for the AP Command Center standup view.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApCommandCenterResponse {
    /// Two buckets: this week (index 0) and next week (index 1).
    pub week_buckets: Vec<Bucket>,
    /// Aggregate late-fee risk across all in-window invoices (in cents).
    pub late_fee_risk_total_cents: i64,
    /// Aggregate expiring discount $ across all in-window invoices (in cents).
    pub discount_expiring_total_cents: i64,
    /// ISO-8601 timestamp when this payload was generated.
    pub generated_at: chrono::DateTime<Utc>,
}

/// One week bucket (this week or next week).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Bucket {
    /// Human label, e.g. "This week" or "Next week".
    pub label: String,
    /// Start of the bucket range (Monday).
    pub range_start: NaiveDate,
    /// End of the bucket range (Sunday).
    pub range_end: NaiveDate,
    /// Sum of `amount_cents` for invoices in this bucket.
    pub total_payable_cents: i64,
    /// Invoices sorted descending by payable amount.
    pub invoices: Vec<BlockingInvoice>,
}

/// A single invoice row enriched with blocking-approver and discount metadata.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlockingInvoice {
    pub invoice_id: Uuid,
    pub invoice_number: String,
    pub vendor_name: String,
    pub amount_cents: i64,
    pub due_date: NaiveDate,
    pub blocking_approver_id: Option<Uuid>,
    pub blocking_approver_name: Option<String>,
    /// Days since the current pending approval request was created.
    pub days_stuck: i32,
    /// Estimated late-fee exposure = invoice amount × vendor late-fee percent.
    pub late_fee_risk_cents: i64,
    /// Discount $ that will expire within this bucket window.
    pub discount_expiring_cents: i64,
    /// Date the early-payment discount window closes, if applicable.
    pub discount_expires_at: Option<NaiveDate>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/dashboard/ap-command-center/this-week",
    tag = "Dashboard",
    responses(
        (status = 200, description = "AP Command Center standup view", body = ApCommandCenterResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_this_week_command_center(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Determine week boundaries (Monday..Sunday).
    let today = Utc::now().date_naive();
    let this_week_start = today - chrono::Duration::days((today.weekday().num_days_from_monday()) as i64);
    let this_week_end = this_week_start + chrono::Duration::days(6);
    let next_week_start = this_week_start + chrono::Duration::days(7);
    let next_week_end = this_week_start + chrono::Duration::days(13);

    // Fetch invoices due within the 14-day window, enriched with approval
    // blocker and discount metadata.
    let rows = sqlx::query_as::<_, (
        Uuid,        // invoice_id
        String,      // invoice_number
        String,      // vendor_name
        i64,         // amount_cents
        NaiveDate,   // due_date
        Option<Uuid>,   // blocking_approver_id
        Option<String>, // blocking_approver_name
        i32,           // days_stuck
        i64,           // late_fee_risk_cents
        Option<f64>,   // discount_percent
        Option<NaiveDate>, // discount_deadline
        Option<i64>,   // discount_amount_cents
    )>(
        r#"
        SELECT
            i.id                    AS invoice_id,
            i.invoice_number,
            i.vendor_name,
            i.total_amount_cents    AS amount_cents,
            i.due_date,
            blocker.approver_id     AS blocking_approver_id,
            blocker.approver_name   AS blocking_approver_name,
            COALESCE(blocker.days_stuck, 0) AS days_stuck,
            CASE
                WHEN v.late_fee_percent IS NULL OR v.late_fee_percent = 0
                THEN 0::bigint
                ELSE ROUND(i.total_amount_cents * v.late_fee_percent / 100.0)::bigint
            END                     AS late_fee_risk_cents,
            i.discount_percent,
            i.discount_deadline,
            CASE
                WHEN i.discount_percent IS NOT NULL
                     AND i.discount_deadline IS NOT NULL
                     AND i.discount_deadline <= $4
                     AND i.discount_captured_at IS NULL
                     AND i.discount_missed_at IS NULL
                THEN ROUND(i.total_amount_cents * i.discount_percent / 100.0)
                ELSE NULL
            END                     AS discount_amount_cents
        FROM invoices i
        LEFT JOIN vendors v ON v.id = i.vendor_id AND v.tenant_id = i.tenant_id
        LEFT JOIN LATERAL (
            SELECT
                u.id   AS approver_id,
                u.name AS approver_name,
                EXTRACT(DAY FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at)))::int AS days_stuck
            FROM approval_requests ar
            JOIN users u ON u.id = NULLIF(ar.requested_from->>'User', '')::uuid
            WHERE ar.invoice_id = i.id
              AND ar.tenant_id  = i.tenant_id
              AND ar.status     = 'pending'
            ORDER BY ar.created_at DESC
            LIMIT 1
        ) blocker ON true
        WHERE i.tenant_id = $1
          AND i.processing_status IN ('pending_approval', 'approved', 'ready_for_payment')
          AND i.due_date IS NOT NULL
          AND i.due_date >= $2
          AND i.due_date <= $3
        ORDER BY i.total_amount_cents DESC
        "#,
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(this_week_start)
    .bind(next_week_end)
    .bind(next_week_end)
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to query AP command center: {}", e)))?;

    let mut this_week_invoices: Vec<BlockingInvoice> = Vec::new();
    let mut next_week_invoices: Vec<BlockingInvoice> = Vec::new();
    let mut late_fee_total: i64 = 0;
    let mut discount_total: i64 = 0;

    for (
        invoice_id,
        invoice_number,
        vendor_name,
        amount_cents,
        due_date,
        blocking_approver_id,
        blocking_approver_name,
        days_stuck,
        late_fee_risk_cents,
        _discount_percent,
        discount_deadline,
        discount_amount_cents,
    ) in rows
    {
        let disc_expiring = discount_amount_cents.unwrap_or(0);
        let disc_expires_at = if discount_amount_cents.is_some() {
            discount_deadline
        } else {
            None
        };

        let inv = BlockingInvoice {
            invoice_id,
            invoice_number,
            vendor_name,
            amount_cents,
            due_date,
            blocking_approver_id,
            blocking_approver_name,
            days_stuck,
            late_fee_risk_cents,
            discount_expiring_cents: disc_expiring,
            discount_expires_at: disc_expires_at,
        };

        late_fee_total += late_fee_risk_cents;
        discount_total += disc_expiring;

        if due_date <= this_week_end {
            this_week_invoices.push(inv);
        } else {
            next_week_invoices.push(inv);
        }
    }

    let this_total: i64 = this_week_invoices.iter().map(|i| i.amount_cents).sum();
    let next_total: i64 = next_week_invoices.iter().map(|i| i.amount_cents).sum();

    let response = ApCommandCenterResponse {
        week_buckets: vec![
            Bucket {
                label: "This week".to_string(),
                range_start: this_week_start,
                range_end: this_week_end,
                total_payable_cents: this_total,
                invoices: this_week_invoices,
            },
            Bucket {
                label: "Next week".to_string(),
                range_start: next_week_start,
                range_end: next_week_end,
                total_payable_cents: next_total,
                invoices: next_week_invoices,
            },
        ],
        late_fee_risk_total_cents: late_fee_total,
        discount_expiring_total_cents: discount_total,
        generated_at: Utc::now(),
    };

    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Action endpoints (session-authenticated, unlike chat_approvals)
// ---------------------------------------------------------------------------

/// Request body for the nudge action.
#[derive(Debug, Deserialize, ToSchema)]
struct NudgeBody {
    comment_body: Option<String>,
}

/// Request body for the reassign action.
#[derive(Debug, Deserialize, ToSchema)]
struct ReassignBody {
    new_approver_id: Uuid,
}

#[derive(Debug, Serialize)]
struct ActionOk {
    ok: bool,
}

/// POST `/api/v1/dashboard/ap-command-center/{invoice_id}/nudge`
///
/// Writes a nudge/comment audit log entry for the invoice, attributed to the
/// logged-in AP manager. This is the session-authenticated counterpart to the
/// Teams/Slack `comment` action in `chat_approvals.rs`.
#[utoipa::path(
    post,
    path = "/api/v1/dashboard/ap-command-center/{invoice_id}/nudge",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Nudge recorded"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn nudge_approver(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(invoice_id): Path<Uuid>,
    Json(body): Json<NudgeBody>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify the invoice belongs to this tenant
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM invoices WHERE id = $1 AND tenant_id = $2)",
    )
    .bind(invoice_id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to verify invoice ownership: {}", e)))?;

    if !belongs {
        return Err(Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: invoice_id.to_string(),
        }.into());
    }

    let comment = body
        .comment_body
        .unwrap_or_else(|| "Gentle reminder: please review this invoice.".to_string());

    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type,
                metadata, source_channel)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'nudge_via_ap_command_center', $5, 'ap_command_center')"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id)
    .bind(user.user_id.as_uuid())
    .bind(serde_json::to_string(&serde_json::json!({ "comment_body": comment }))
        .unwrap_or_default())
    .execute(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to write nudge audit: {}", e)))?;

    Ok(Json(ActionOk { ok: true }))
}

/// POST `/api/v1/dashboard/ap-command-center/{invoice_id}/reassign`
///
/// Reassigns the current pending approval request on an invoice to a different
/// user. Updates `requested_from` on the active `approval_requests` row and
/// writes an audit log entry. Session-authenticated — the actor is the
/// logged-in AP manager, not a Teams/Slack bot mapping.
#[utoipa::path(
    post,
    path = "/api/v1/dashboard/ap-command-center/{invoice_id}/reassign",
    tag = "Dashboard",
    responses(
        (status = 200, description = "Approval reassigned"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn reassign_approver(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(invoice_id): Path<Uuid>,
    Json(body): Json<ReassignBody>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify the invoice belongs to this tenant
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM invoices WHERE id = $1 AND tenant_id = $2)",
    )
    .bind(invoice_id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to verify invoice ownership: {}", e)))?;

    if !belongs {
        return Err(Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: invoice_id.to_string(),
        }.into());
    }

    // Verify the target approver exists in this tenant
    let approver_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2)",
    )
    .bind(body.new_approver_id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to verify approver: {}", e)))?;

    if !approver_exists {
        return Err(Error::NotFound {
            resource_type: "User".to_string(),
            id: body.new_approver_id.to_string(),
        }.into());
    }

    // Update the pending approval request(s) for this invoice
    let updated = sqlx::query(
        r#"UPDATE approval_requests
           SET requested_from = jsonb_build_object('User', $1::text),
               updated_at = NOW()
           WHERE invoice_id = $2
             AND tenant_id = $3
             AND status = 'pending'"#,
    )
    .bind(body.new_approver_id)
    .bind(invoice_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to reassign approval: {}", e)))?;

    // Write audit log
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type,
                metadata, source_channel)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'reassign_via_ap_command_center', $5, 'ap_command_center')"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id)
    .bind(user.user_id.as_uuid())
    .bind(serde_json::to_string(&serde_json::json!({
        "reassign_to_user_id": body.new_approver_id.to_string(),
        "rows_updated": updated.rows_affected(),
    }))
    .unwrap_or_default())
    .execute(&*pool)
    .await
    .map_err(|e| Error::Internal(format!("Failed to write reassign audit: {}", e)))?;

    Ok(Json(ActionOk { ok: true }))
}
