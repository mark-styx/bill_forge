//! Recurring-pattern CRUD and policy endpoints.
//!
//! GET  /api/v1/recurring-patterns         - list patterns for tenant
//! PATCH /api/v1/recurring-patterns/:id    - update auto-approval policy fields

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct RecurringPatternResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vendor_id: Uuid,
    pub vendor_name: Option<String>,
    pub cadence_days: i32,
    pub trailing_median_cents: i64,
    pub sample_count: i32,
    pub last_invoice_date: Option<chrono::NaiveDate>,
    pub last_line_items_hash: Option<String>,
    pub auto_approve_enabled: bool,
    pub amount_tolerance_pct: f64,
    pub window_tolerance_days: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePatternRequest {
    pub auto_approve_enabled: Option<bool>,
    pub amount_tolerance_pct: Option<f64>,
    pub window_tolerance_days: Option<i32>,
}

// ---------------------------------------------------------------------------
// Row types for SQL queries
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct PatternRow {
    id: Uuid,
    tenant_id: Uuid,
    vendor_id: Uuid,
    vendor_name: Option<String>,
    cadence_days: i32,
    trailing_median_cents: i64,
    sample_count: i32,
    last_invoice_date: Option<chrono::NaiveDate>,
    last_line_items_hash: Option<String>,
    auto_approve_enabled: bool,
    amount_tolerance_pct: f64,
    window_tolerance_days: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<PatternRow> for RecurringPatternResponse {
    fn from(r: PatternRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            vendor_id: r.vendor_id,
            vendor_name: r.vendor_name,
            cadence_days: r.cadence_days,
            trailing_median_cents: r.trailing_median_cents,
            sample_count: r.sample_count,
            last_invoice_date: r.last_invoice_date,
            last_line_items_hash: r.last_line_items_hash,
            auto_approve_enabled: r.auto_approve_enabled,
            amount_tolerance_pct: r.amount_tolerance_pct,
            window_tolerance_days: r.window_tolerance_days,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_patterns))
        .route("/:id", patch(update_pattern))
}

/// GET /api/v1/recurring-patterns
async fn list_patterns(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
) -> ApiResult<Json<Vec<RecurringPatternResponse>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query_as::<_, PatternRow>(
        r#"SELECT rp.id, rp.tenant_id, rp.vendor_id,
                  v.name AS vendor_name,
                  rp.cadence_days, rp.trailing_median_cents, rp.sample_count,
                  rp.last_invoice_date, rp.last_line_items_hash,
                  rp.auto_approve_enabled,
                  CAST(rp.amount_tolerance_pct AS DOUBLE PRECISION) AS amount_tolerance_pct,
                  rp.window_tolerance_days,
                  rp.created_at, rp.updated_at
           FROM recurring_patterns rp
           LEFT JOIN vendors v ON v.id = rp.vendor_id
           WHERE rp.tenant_id = $1
           ORDER BY rp.updated_at DESC"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to list recurring patterns: {}", e))
    })?;

    Ok(Json(rows.into_iter().map(RecurringPatternResponse::from).collect()))
}

/// PATCH /api/v1/recurring-patterns/:id
async fn update_pattern(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePatternRequest>,
) -> ApiResult<Json<RecurringPatternResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify tenant ownership.
    let _existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM recurring_patterns WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to find recurring pattern: {}", e))
    })?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "RecurringPattern".to_string(),
        id: id.to_string(),
    })?;

    // Update provided fields.
    if let Some(enabled) = body.auto_approve_enabled {
        sqlx::query(
            "UPDATE recurring_patterns SET auto_approve_enabled = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(enabled)
        .bind(id)
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update recurring pattern: {}", e))
        })?;
    }
    if let Some(pct) = body.amount_tolerance_pct {
        if pct < 0.0 || pct > 100.0 {
            return Err(billforge_core::Error::Validation(
                "amount_tolerance_pct must be between 0 and 100".to_string(),
            )
            .into());
        }
        sqlx::query(
            "UPDATE recurring_patterns SET amount_tolerance_pct = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(pct)
        .bind(id)
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update recurring pattern: {}", e))
        })?;
    }
    if let Some(days) = body.window_tolerance_days {
        if days < 0 {
            return Err(billforge_core::Error::Validation(
                "window_tolerance_days must be non-negative".to_string(),
            )
            .into());
        }
        sqlx::query(
            "UPDATE recurring_patterns SET window_tolerance_days = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(days)
        .bind(id)
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update recurring pattern: {}", e))
        })?;
    }

    // Audit-log the policy change.
    let changes = serde_json::json!({
        "auto_approve_enabled": body.auto_approve_enabled,
        "amount_tolerance_pct": body.amount_tolerance_pct,
        "window_tolerance_days": body.window_tolerance_days,
    });
    if let Err(e) = sqlx::query(
        r#"INSERT INTO invoice_audit_log
           (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(tenant.tenant_id.as_uuid())
    .bind(uuid::Uuid::nil()) // no specific invoice
    .bind(Some(user.0.user_id.as_uuid()))
    .bind("n/a")
    .bind("n/a")
    .bind("recurring_pattern_policy_update")
    .bind(serde_json::json!({
        "pattern_id": id,
        "changes": changes,
    }))
    .execute(&*pool)
    .await
    {
        tracing::warn!(error = %e, "Failed to audit-log recurring pattern policy update");
    }

    // Fetch updated row.
    let row = sqlx::query_as::<_, PatternRow>(
        r#"SELECT rp.id, rp.tenant_id, rp.vendor_id,
                  v.name AS vendor_name,
                  rp.cadence_days, rp.trailing_median_cents, rp.sample_count,
                  rp.last_invoice_date, rp.last_line_items_hash,
                  rp.auto_approve_enabled,
                  CAST(rp.amount_tolerance_pct AS DOUBLE PRECISION) AS amount_tolerance_pct,
                  rp.window_tolerance_days,
                  rp.created_at, rp.updated_at
           FROM recurring_patterns rp
           LEFT JOIN vendors v ON v.id = rp.vendor_id
           WHERE rp.id = $1"#,
    )
    .bind(id)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!(
            "Failed to fetch updated recurring pattern: {}",
            e
        ))
    })?;

    Ok(Json(RecurringPatternResponse::from(row)))
}
