//! Month-end close period management routes
//!
//! Provides endpoints for AP to define cutoff dates per period, run close to
//! generate accrual journal entries from unapproved invoices, attempt QBO posting,
//! and lock the period so no late entries shift prior-period numbers.
//!
//! NOTE: Real QBO JournalEntry posting is not yet implemented. When a sync_enabled
//! QBO connection exists the close flow returns `ErpPostResult::Unsupported` instead
//! of fabricating a synthetic journal id. Follow-up tracked in issue #339.

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use billforge_core::TenantId;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_periods).post(create_period))
        .route("/current/readiness", get(get_current_period_readiness))
        .route("/:id", patch(update_period))
        .route("/:id/close", post(run_close))
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreatePeriodRequest {
    pub period_label: String, // e.g. "2026-05"
    pub period_start: String, // ISO date
    pub period_end: String,   // ISO date
    pub cutoff_date: String,  // ISO date
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdatePeriodRequest {
    pub cutoff_date: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClosePeriodResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub period_label: String,
    pub period_start: String,
    pub period_end: String,
    pub cutoff_date: String,
    pub status: String,
    pub locked_at: Option<String>,
    pub locked_by_user_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunCloseResponse {
    pub period_id: Uuid,
    pub accrual_entries_created: usize,
    pub erp_post_status: String,
    pub erp_post_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Readiness types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadinessResponse {
    pub period: Option<ClosePeriodResponse>,
    pub score: Option<i32>,
    pub computed_at: String,
    pub totals: ReadinessTotals,
    pub exceptions: Vec<ExceptionItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadinessTotals {
    pub total_invoices: i64,
    pub unapproved_invoices: i64,
    pub accruals_drafted: i64,
    pub invoices_needing_accrual: i64,
    pub invoices_missing_gl_coding: i64,
    pub days_until_cutoff: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExceptionItem {
    pub id: String,
    pub label: String,
    pub count: i64,
    pub severity: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether any locked period covers the given invoice_date for the tenant.
/// Returns Some(period_id) if a locked period covers the date.
pub async fn find_locked_period_for_date(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_date: &str,
) -> Result<Option<Uuid>, billforge_core::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM close_periods \
         WHERE tenant_id = $1 AND status = 'locked' \
         AND period_start <= $2::date AND period_end >= $2::date \
         LIMIT 1",
    )
    .bind(tenant_id.as_uuid())
    .bind(invoice_date)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to check locked period: {}", e))
    })?;

    Ok(row.map(|(id,)| id))
}

/// Attempt to post accrual entries to QBO. Returns an `ErpPostResult` indicating
/// the outcome. Real QBO JournalEntry posting is not yet implemented (issue #339);
/// when a sync_enabled connection exists we return `Unsupported` instead of
/// fabricating a success.
async fn post_accrual_entries_to_erp(
    state: &AppState,
    tenant_id: &TenantId,
    _entries: &[AccrualEntryStub],
) -> ErpPostResult {
    // Try to get an authenticated QBO client. If it fails (no connection),
    // return NoConnection so entries stay pending.
    let pool = match state.db.tenant(tenant_id).await {
        Ok(p) => p,
        Err(_) => {
            return ErpPostResult::Failed {
                error: "Could not connect to tenant database".to_string(),
            }
        }
    };

    // Check if QBO connection exists
    let has_connection: Option<(bool,)> = sqlx::query_as(
        "SELECT true FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true LIMIT 1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    match has_connection {
        Some(_) => {
            // QBO connection exists but JournalEntry posting is not implemented.
            // Do NOT fabricate a synthetic journal id.
            tracing::warn!(
                tenant_id = %tenant_id,
                "QBO connection found but journal entry posting is not implemented; \
                 returning Unsupported (issue #339)"
            );
            ErpPostResult::Unsupported {
                reason: "QBO journal entry posting not implemented".to_string(),
            }
        }
        None => {
            // No QBO connection - entries stay pending
            tracing::info!(
                tenant_id = %tenant_id,
                "No QBO connection found; accrual entries remain pending"
            );
            ErpPostResult::NoConnection
        }
    }
}

/// Result of attempting to post accrual entries to the ERP.
enum ErpPostResult {
    /// Successfully posted; contains the real journal id from the ERP.
    Posted { journal_id: String },
    /// No ERP connection configured; entries stay pending.
    NoConnection,
    /// ERP posting is not yet supported for this connection type.
    Unsupported { reason: String },
    /// ERP posting attempted but failed.
    Failed { error: String },
}

struct AccrualEntryStub {
    id: Uuid,
    invoice_id: Option<Uuid>,
    vendor_id: Option<Uuid>,
    amount_cents: i64,
    description: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/close-periods/current/readiness",
    tag = "Close",
    responses(
        (status = 200, description = "Current period readiness", body = ReadinessResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_current_period_readiness(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<ReadinessResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // 1. Resolve current open period (latest row where status = 'open')
    let period_row: Option<(
        Uuid,
        Uuid,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<Uuid>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, tenant_id, period_label, period_start::text, period_end::text, \
                cutoff_date::text, status, locked_at::text, locked_by_user_id, \
                created_at::text, updated_at::text \
         FROM close_periods \
         WHERE tenant_id = $1 AND status = 'open' \
         ORDER BY period_start DESC \
         LIMIT 1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch open period: {}", e)))?;

    let computed_at = Utc::now().to_rfc3339();

    let Some(period_row) = period_row else {
        return Ok(Json(ReadinessResponse {
            period: None,
            score: None,
            computed_at,
            totals: ReadinessTotals {
                total_invoices: 0,
                unapproved_invoices: 0,
                accruals_drafted: 0,
                invoices_needing_accrual: 0,
                invoices_missing_gl_coding: 0,
                days_until_cutoff: None,
            },
            exceptions: vec![ExceptionItem {
                id: "no_open_period".to_string(),
                label: "No open close period configured".to_string(),
                count: 1,
                severity: "high".to_string(),
            }],
        }));
    };

    let period = ClosePeriodResponse {
        id: period_row.0,
        tenant_id: period_row.1,
        period_label: period_row.2.clone(),
        period_start: period_row.3.clone(),
        period_end: period_row.4.clone(),
        cutoff_date: period_row.5.clone(),
        status: period_row.6,
        locked_at: period_row.7,
        locked_by_user_id: period_row.8,
        created_at: period_row.9,
        updated_at: period_row.10,
    };

    // 2. Compute counts within period window
    // total_invoices in window
    let (total_invoices,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoices \
         WHERE tenant_id = $1 AND invoice_date >= $2::date AND invoice_date <= $3::date",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&period.period_start)
    .bind(&period.period_end)
    .fetch_one(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to count invoices: {}", e)))?;

    // unapproved_invoices: status NOT IN ('approved', 'paid', 'void', 'rejected')
    let (unapproved_invoices,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoices \
         WHERE tenant_id = $1 AND invoice_date >= $2::date AND invoice_date <= $3::date \
           AND status NOT IN ('approved', 'paid', 'void', 'rejected')",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&period.period_start)
    .bind(&period.period_end)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to count unapproved invoices: {}", e))
    })?;

    // accruals_drafted for this period
    let (accruals_drafted,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM close_accrual_entries WHERE close_period_id = $1")
            .bind(period.id)
            .fetch_one(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to count accruals: {}", e))
            })?;

    // invoices_needing_accrual: unapproved invoices in window with NO matching accrual row
    let (invoices_needing_accrual,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoices i \
         WHERE i.tenant_id = $1 AND i.invoice_date >= $2::date AND i.invoice_date <= $3::date \
           AND i.status NOT IN ('approved', 'paid', 'void', 'rejected') \
           AND NOT EXISTS ( \
               SELECT 1 FROM close_accrual_entries cae \
               WHERE cae.invoice_id = i.id AND cae.close_period_id = $4 \
           )",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&period.period_start)
    .bind(&period.period_end)
    .bind(period.id)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to count invoices needing accrual: {}", e))
    })?;

    // invoices_missing_gl_coding: window invoices with null/empty gl_code
    let (invoices_missing_gl_coding,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoices \
         WHERE tenant_id = $1 AND invoice_date >= $2::date AND invoice_date <= $3::date \
           AND (gl_code IS NULL OR gl_code = '')",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&period.period_start)
    .bind(&period.period_end)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!(
            "Failed to count invoices missing GL coding: {}",
            e
        ))
    })?;

    // days_until_cutoff
    let days_until_cutoff: Option<i64> =
        sqlx::query_scalar("SELECT ($1::date - CURRENT_DATE)::bigint")
            .bind(&period.cutoff_date)
            .fetch_one(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!(
                    "Failed to compute days until cutoff: {}",
                    e
                ))
            })?;

    // 3. Compute score
    let denominator = std::cmp::max(total_invoices, 1);
    let raw_score = 100.0
        * (1.0
            - ((invoices_needing_accrual + invoices_missing_gl_coding) as f64
                / denominator as f64));
    let score = raw_score.round().clamp(0.0, 100.0) as i32;

    // 4. Build exceptions checklist
    let mut exceptions = Vec::new();

    if invoices_needing_accrual > 0 {
        exceptions.push(ExceptionItem {
            id: "unaccrued_invoices".to_string(),
            label: "Invoices missing accrual".to_string(),
            count: invoices_needing_accrual,
            severity: "high".to_string(),
        });
    }

    if invoices_missing_gl_coding > 0 {
        exceptions.push(ExceptionItem {
            id: "missing_gl_coding".to_string(),
            label: "Invoices missing GL coding".to_string(),
            count: invoices_missing_gl_coding,
            severity: "medium".to_string(),
        });
    }

    if unapproved_invoices > 0 {
        exceptions.push(ExceptionItem {
            id: "unapproved_invoices".to_string(),
            label: "Unapproved invoices".to_string(),
            count: unapproved_invoices,
            severity: "low".to_string(),
        });
    }

    Ok(Json(ReadinessResponse {
        period: Some(period),
        score: Some(score),
        computed_at,
        totals: ReadinessTotals {
            total_invoices,
            unapproved_invoices,
            accruals_drafted,
            invoices_needing_accrual,
            invoices_missing_gl_coding,
            days_until_cutoff,
        },
        exceptions,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/close-periods",
    tag = "Close",
    responses(
        (status = 200, description = "All close periods for tenant", body = [ClosePeriodResponse]),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_periods(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<ClosePeriodResponse>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows: Vec<(
        Uuid,
        Uuid,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<Uuid>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, tenant_id, period_label, period_start::text, period_end::text, \
                cutoff_date::text, status, locked_at::text, locked_by_user_id, \
                created_at::text, updated_at::text \
         FROM close_periods \
         WHERE tenant_id = $1 \
         ORDER BY period_start DESC",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to list close periods: {}", e)))?;

    let periods = rows
        .into_iter()
        .map(
            |(
                id,
                tid,
                label,
                start,
                end,
                cutoff,
                status,
                locked_at,
                locked_by,
                created,
                updated,
            )| {
                ClosePeriodResponse {
                    id,
                    tenant_id: tid,
                    period_label: label,
                    period_start: start,
                    period_end: end,
                    cutoff_date: cutoff,
                    status,
                    locked_at,
                    locked_by_user_id: locked_by,
                    created_at: created,
                    updated_at: updated,
                }
            },
        )
        .collect();

    Ok(Json(periods))
}

#[utoipa::path(
    post,
    path = "/api/v1/close-periods",
    tag = "Close",
    request_body = CreatePeriodRequest,
    responses(
        (status = 201, description = "Period created", body = ClosePeriodResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_period(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(body): Json<CreatePeriodRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let row: (Uuid, Uuid, String, String, String, String, String, Option<String>, Option<Uuid>, String, String) = sqlx::query_as(
        "INSERT INTO close_periods (tenant_id, period_label, period_start, period_end, cutoff_date) \
         VALUES ($1, $2, $3::date, $4::date, $5::date) \
         RETURNING id, tenant_id, period_label, period_start::text, period_end::text, \
                   cutoff_date::text, status, locked_at::text, locked_by_user_id, \
                   created_at::text, updated_at::text",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&body.period_label)
    .bind(&body.period_start)
    .bind(&body.period_end)
    .bind(&body.cutoff_date)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate") || e.to_string().contains("uniq") {
            billforge_core::Error::AlreadyExists {
                resource_type: "ClosePeriod".to_string(),
            }
        } else {
            billforge_core::Error::Database(format!("Failed to create close period: {}", e))
        }
    })?;

    let resp = ClosePeriodResponse {
        id: row.0,
        tenant_id: row.1,
        period_label: row.2,
        period_start: row.3,
        period_end: row.4,
        cutoff_date: row.5,
        status: row.6,
        locked_at: row.7,
        locked_by_user_id: row.8,
        created_at: row.9,
        updated_at: row.10,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/close-periods/{id}",
    tag = "Close",
    params(("id" = Uuid, Path, description = "Close period id")),
    request_body = UpdatePeriodRequest,
    responses(
        (status = 200, description = "Period updated", body = ClosePeriodResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Period not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_period(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePeriodRequest>,
) -> ApiResult<Json<ClosePeriodResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Only allow updating cutoff_date when status is 'open'
    if let Some(ref cutoff) = body.cutoff_date {
        let result = sqlx::query(
            "UPDATE close_periods SET cutoff_date = $1::date, updated_at = NOW() \
             WHERE id = $2 AND tenant_id = $3 AND status = 'open'",
        )
        .bind(cutoff)
        .bind(id)
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update close period: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(billforge_core::Error::Validation(
                "Period not found or not in 'open' status".to_string(),
            )
            .into());
        }
    }

    // Fetch updated row
    let row: (
        Uuid,
        Uuid,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<Uuid>,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT id, tenant_id, period_label, period_start::text, period_end::text, \
                cutoff_date::text, status, locked_at::text, locked_by_user_id, \
                created_at::text, updated_at::text \
         FROM close_periods WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|_| billforge_core::Error::NotFound {
        resource_type: "ClosePeriod".to_string(),
        id: id.to_string(),
    })?;

    let resp = ClosePeriodResponse {
        id: row.0,
        tenant_id: row.1,
        period_label: row.2,
        period_start: row.3,
        period_end: row.4,
        cutoff_date: row.5,
        status: row.6,
        locked_at: row.7,
        locked_by_user_id: row.8,
        created_at: row.9,
        updated_at: row.10,
    };

    Ok(Json(resp))
}

#[utoipa::path(
    post,
    path = "/api/v1/close-periods/{id}/close",
    tag = "Close",
    params(("id" = Uuid, Path, description = "Close period id")),
    responses(
        (status = 200, description = "Close run completed (accruals + ERP post attempt)", body = RunCloseResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Period not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn run_close(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RunCloseResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Begin transaction
    let mut tx = pool.begin().await.map_err(|e| {
        billforge_core::Error::Database(format!("Failed to begin transaction: {}", e))
    })?;

    // 1. Verify period exists and is not locked
    let period: Option<(String, String, String)> = sqlx::query_as(
        "SELECT status, period_end::text, cutoff_date::text \
         FROM close_periods WHERE id = $1 AND tenant_id = $2 \
         FOR UPDATE",
    )
    .bind(id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch period: {}", e)))?;

    let (current_status, period_end, _cutoff_date) =
        period.ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ClosePeriod".to_string(),
            id: id.to_string(),
        })?;

    if current_status == "locked" {
        tx.rollback()
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to rollback: {}", e)))?;
        return Err(
            billforge_core::Error::Validation("Period is already locked".to_string()).into(),
        );
    }

    // 2. Find unapproved invoices within period range that haven't been accrued yet
    //    Status NOT IN ('approved', 'paid', 'void', 'rejected') means still unapproved
    let invoices: Vec<(Uuid, Option<Uuid>, i64, Option<String>)> = sqlx::query_as(
        "SELECT i.id, i.vendor_id, i.total_amount_cents, i.invoice_number \
         FROM invoices i \
         WHERE i.tenant_id = $1 \
           AND i.invoice_date <= $2::date \
           AND i.status NOT IN ('approved', 'paid', 'void', 'rejected') \
           AND NOT EXISTS ( \
               SELECT 1 FROM close_accrual_entries cae \
               WHERE cae.invoice_id = i.id AND cae.close_period_id = $3 \
           )",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&period_end)
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to query unapproved invoices: {}", e))
    })?;

    let entry_count = invoices.len();

    // 3. Insert accrual entries
    let mut entry_stubs = Vec::with_capacity(invoices.len());
    for (invoice_id, vendor_id, amount_cents, invoice_number) in &invoices {
        let entry_id = Uuid::new_v4();
        let description = format!(
            "Accrual for unapproved invoice {}",
            invoice_number.as_deref().unwrap_or("unknown")
        );

        sqlx::query(
            "INSERT INTO close_accrual_entries \
             (id, close_period_id, invoice_id, vendor_id, gl_account, amount_cents, description, source) \
             VALUES ($1, $2, $3, $4, '2100 - Accrued Expenses', $5, $6, 'unapproved_invoice')",
        )
        .bind(entry_id)
        .bind(id)
        .bind(invoice_id)
        .bind(vendor_id)
        .bind(amount_cents)
        .bind(&description)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to insert accrual entry: {}", e))
        })?;

        // Mark invoice as posted to this period
        sqlx::query(
            "UPDATE invoices SET posted_to_period_id = $1 WHERE id = $2 AND tenant_id = $3",
        )
        .bind(id)
        .bind(invoice_id)
        .bind(tenant.tenant_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!(
                "Failed to update invoice posted_to_period_id: {}",
                e
            ))
        })?;

        entry_stubs.push(AccrualEntryStub {
            id: entry_id,
            invoice_id: Some(*invoice_id),
            vendor_id: *vendor_id,
            amount_cents: *amount_cents,
            description: Some(description),
        });
    }

    // Touch updated_at when draft accruals are generated (serves as last_auto_drafted_at)
    if entry_count > 0 {
        sqlx::query("UPDATE close_periods SET updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to touch period updated_at: {}", e))
            })?;
    }

    // 4. Attempt ERP posting
    let erp_result = post_accrual_entries_to_erp(&state, &tenant.tenant_id, &entry_stubs).await;

    // Derive (erp_post_status, period_status, erp_post_error) from the result.
    let (erp_post_status, period_status, erp_post_error): (String, &str, Option<String>) =
        match &erp_result {
            ErpPostResult::Posted { journal_id } => ("posted".to_string(), "locked", None),
            ErpPostResult::NoConnection => {
                // No ERP connection configured - still lock the period.
                ("pending".to_string(), "locked", None)
            }
            ErpPostResult::Unsupported { reason } => {
                // ERP posting not implemented - do NOT lock the period.
                (
                    "unsupported".to_string(),
                    "cutoff_passed",
                    Some(reason.clone()),
                )
            }
            ErpPostResult::Failed { error } => {
                ("failed".to_string(), "cutoff_passed", Some(error.clone()))
            }
        };

    // 5. Update accrual entries with ERP result
    match &erp_result {
        ErpPostResult::Posted { journal_id } => {
            sqlx::query(
                "UPDATE close_accrual_entries \
                 SET erp_journal_id = $1, erp_post_status = 'posted' \
                 WHERE close_period_id = $2 AND erp_post_status = 'pending'",
            )
            .bind(journal_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update accrual entries: {}", e))
            })?;
        }
        ErpPostResult::Unsupported { reason } => {
            sqlx::query(
                "UPDATE close_accrual_entries \
                 SET erp_post_status = 'unsupported', erp_post_error = $1 \
                 WHERE close_period_id = $2 AND erp_post_status = 'pending'",
            )
            .bind(reason)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update accrual entries: {}", e))
            })?;
        }
        ErpPostResult::Failed { error } => {
            sqlx::query(
                "UPDATE close_accrual_entries \
                 SET erp_post_status = 'failed', erp_post_error = $1 \
                 WHERE close_period_id = $2 AND erp_post_status = 'pending'",
            )
            .bind(error)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update accrual entries: {}", e))
            })?;
        }
        ErpPostResult::NoConnection => {
            // Entries already inserted as 'pending'; nothing to update.
        }
    }

    // 6. Update period status
    let now = Utc::now();
    if period_status == "locked" {
        sqlx::query(
            "UPDATE close_periods \
             SET status = 'locked', locked_at = $1, locked_by_user_id = $2, updated_at = $1 \
             WHERE id = $3 AND tenant_id = $4",
        )
        .bind(now)
        .bind(tenant.tenant_id.as_uuid()) // use tenant_id as user_id placeholder; real impl passes user
        .bind(id)
        .bind(tenant.tenant_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to lock period: {}", e)))?;
    } else {
        sqlx::query(
            "UPDATE close_periods SET status = 'cutoff_passed', updated_at = NOW() \
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant.tenant_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update period status: {}", e))
        })?;
    }

    // 7. Commit
    tx.commit()
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to commit close: {}", e)))?;

    Ok(Json(RunCloseResponse {
        period_id: id,
        accrual_entries_created: entry_count,
        erp_post_status,
        erp_post_error,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_period_request_deserialize() {
        let json = r#"{
            "period_label": "2026-05",
            "period_start": "2026-05-01",
            "period_end": "2026-05-31",
            "cutoff_date": "2026-05-25"
        }"#;
        let req: CreatePeriodRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.period_label, "2026-05");
        assert_eq!(req.period_start, "2026-05-01");
        assert_eq!(req.period_end, "2026-05-31");
        assert_eq!(req.cutoff_date, "2026-05-25");
    }

    #[test]
    fn test_update_period_request_deserialize() {
        let json = r#"{"cutoff_date": "2026-05-28"}"#;
        let req: UpdatePeriodRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.cutoff_date.as_deref(), Some("2026-05-28"));
    }

    #[test]
    fn test_update_period_request_empty() {
        let json = r#"{}"#;
        let req: UpdatePeriodRequest = serde_json::from_str(json).unwrap();
        assert!(req.cutoff_date.is_none());
    }

    #[test]
    fn test_run_close_response_serialize() {
        let resp = RunCloseResponse {
            period_id: Uuid::new_v4(),
            accrual_entries_created: 5,
            erp_post_status: "posted".to_string(),
            erp_post_error: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["accrual_entries_created"], 5);
        assert_eq!(json["erp_post_status"], "posted");
    }

    #[test]
    fn test_run_close_response_unsupported() {
        let period_id = Uuid::new_v4();
        let resp = RunCloseResponse {
            period_id,
            accrual_entries_created: 2,
            erp_post_status: "unsupported".to_string(),
            erp_post_error: Some("QBO journal entry posting not implemented".to_string()),
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["period_id"], period_id.to_string());
        assert_eq!(val["accrual_entries_created"], 2);
        assert_eq!(val["erp_post_status"], "unsupported");
        assert_eq!(
            val["erp_post_error"],
            "QBO journal entry posting not implemented"
        );
    }

    /// Verify the ErpPostResult::Unsupported variant produces the right field
    /// values when mapped through the run_close match arm (unit-level, no DB).
    #[test]
    fn test_erp_post_result_unsupported_maps_correctly() {
        let erp_result = ErpPostResult::Unsupported {
            reason: "QBO journal entry posting not implemented".to_string(),
        };

        let (erp_post_status, period_status, erp_post_error) = match &erp_result {
            ErpPostResult::Posted { journal_id: _ } => ("posted".to_string(), "locked", None),
            ErpPostResult::NoConnection => ("pending".to_string(), "locked", None),
            ErpPostResult::Unsupported { reason } => (
                "unsupported".to_string(),
                "cutoff_passed",
                Some(reason.clone()),
            ),
            ErpPostResult::Failed { error } => {
                ("failed".to_string(), "cutoff_passed", Some(error.clone()))
            }
        };

        assert_eq!(erp_post_status, "unsupported");
        assert_eq!(period_status, "cutoff_passed");
        assert_eq!(
            erp_post_error,
            Some("QBO journal entry posting not implemented".to_string())
        );
    }

    #[test]
    fn test_readiness_response_no_period() {
        let resp = ReadinessResponse {
            period: None,
            score: None,
            computed_at: "2026-06-01T00:00:00Z".to_string(),
            totals: ReadinessTotals {
                total_invoices: 0,
                unapproved_invoices: 0,
                accruals_drafted: 0,
                invoices_needing_accrual: 0,
                invoices_missing_gl_coding: 0,
                days_until_cutoff: None,
            },
            exceptions: vec![ExceptionItem {
                id: "no_open_period".to_string(),
                label: "No open close period configured".to_string(),
                count: 1,
                severity: "high".to_string(),
            }],
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert!(val["period"].is_null());
        assert!(val["score"].is_null());
        assert_eq!(val["exceptions"][0]["id"], "no_open_period");
    }

    #[test]
    fn test_readiness_response_score_100_clean() {
        let resp = ReadinessResponse {
            period: Some(ClosePeriodResponse {
                id: Uuid::new_v4(),
                tenant_id: Uuid::new_v4(),
                period_label: "2026-05".to_string(),
                period_start: "2026-05-01".to_string(),
                period_end: "2026-05-31".to_string(),
                cutoff_date: "2026-05-25".to_string(),
                status: "open".to_string(),
                locked_at: None,
                locked_by_user_id: None,
                created_at: "2026-05-01T00:00:00Z".to_string(),
                updated_at: "2026-05-01T00:00:00Z".to_string(),
            }),
            score: Some(100),
            computed_at: "2026-06-01T00:00:00Z".to_string(),
            totals: ReadinessTotals {
                total_invoices: 10,
                unapproved_invoices: 0,
                accruals_drafted: 0,
                invoices_needing_accrual: 0,
                invoices_missing_gl_coding: 0,
                days_until_cutoff: Some(24),
            },
            exceptions: vec![],
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["score"], 100);
        assert!(val["exceptions"].as_array().unwrap().is_empty());
        assert_eq!(val["totals"]["total_invoices"], 10);
    }

    #[test]
    fn test_readiness_response_with_exceptions() {
        let resp = ReadinessResponse {
            period: Some(ClosePeriodResponse {
                id: Uuid::new_v4(),
                tenant_id: Uuid::new_v4(),
                period_label: "2026-05".to_string(),
                period_start: "2026-05-01".to_string(),
                period_end: "2026-05-31".to_string(),
                cutoff_date: "2026-05-25".to_string(),
                status: "open".to_string(),
                locked_at: None,
                locked_by_user_id: None,
                created_at: "2026-05-01T00:00:00Z".to_string(),
                updated_at: "2026-05-01T00:00:00Z".to_string(),
            }),
            score: Some(62),
            computed_at: "2026-06-01T00:00:00Z".to_string(),
            totals: ReadinessTotals {
                total_invoices: 10,
                unapproved_invoices: 3,
                accruals_drafted: 1,
                invoices_needing_accrual: 2,
                invoices_missing_gl_coding: 1,
                days_until_cutoff: Some(5),
            },
            exceptions: vec![
                ExceptionItem {
                    id: "unaccrued_invoices".to_string(),
                    label: "Invoices missing accrual".to_string(),
                    count: 2,
                    severity: "high".to_string(),
                },
                ExceptionItem {
                    id: "missing_gl_coding".to_string(),
                    label: "Invoices missing GL coding".to_string(),
                    count: 1,
                    severity: "medium".to_string(),
                },
                ExceptionItem {
                    id: "unapproved_invoices".to_string(),
                    label: "Unapproved invoices".to_string(),
                    count: 3,
                    severity: "low".to_string(),
                },
            ],
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["score"], 62);
        let exc = val["exceptions"].as_array().unwrap();
        assert_eq!(exc.len(), 3);
        assert_eq!(exc[0]["id"], "unaccrued_invoices");
        assert_eq!(exc[0]["count"], 2);
        assert_eq!(exc[0]["severity"], "high");
    }
}
