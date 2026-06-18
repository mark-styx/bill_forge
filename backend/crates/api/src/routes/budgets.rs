//! Budget guardrails: CRUD endpoints for per-department, cost-center, GL-account,
//! and project budgets, plus a live remaining-balance check at approval time.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extractors::{AuthUser, ReportingAccess};
use crate::state::AppState;
use billforge_core::AuditService;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single budget row.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Budget {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub scope_type: String,
    pub scope_value: String,
    pub period_type: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub amount_cents: i64,
    pub enforcement: String,
    pub created_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create-budget request body.
#[derive(Debug, Deserialize)]
pub struct CreateBudgetRequest {
    pub scope_type: String,
    pub scope_value: String,
    pub period_type: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub amount_cents: i64,
    #[serde(default)]
    pub enforcement: Option<String>,
}

/// Patch-budget request body (partial update).
#[derive(Debug, Deserialize)]
pub struct PatchBudgetRequest {
    pub amount_cents: Option<i64>,
    pub enforcement: Option<String>,
}

/// Budget-check query parameters.
#[derive(Debug, Deserialize)]
pub struct BudgetCheckQuery {
    pub scope_type: String,
    pub scope_value: String,
    pub date: NaiveDate,
    pub amount_cents: i64,
}

/// Result of a single-dimension budget check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetCheckResult {
    pub scope_type: String,
    pub scope_value: String,
    pub budget_amount_cents: i64,
    pub committed_cents: i64,
    pub remaining_after_cents: i64,
    pub enforcement: String,
    /// `ok` | `warn` | `block`
    pub status: String,
}

/// Invoice-level budget check result (multi-dimension).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceBudgetCheckResult {
    pub results: Vec<BudgetCheckResult>,
    /// True if any dimension has status=block
    pub blocked: bool,
    /// Dimensions with status=warn
    pub warnings: Vec<BudgetCheckResult>,
    /// Dimensions with status=block
    pub violations: Vec<BudgetCheckResult>,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_budgets).post(create_budget))
        .route("/check", get(check_budget))
        .route("/check-invoice/:invoice_id", get(check_invoice_budget))
        .route("/:id", patch(update_budget).delete(delete_budget))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/budgets
async fn list_budgets(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
) -> ApiResult<Json<Vec<Budget>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let rows = sqlx::query_as::<_, Budget>(
        "SELECT id, tenant_id, scope_type, scope_value, period_type, \
         period_start, period_end, amount_cents, enforcement, created_by, \
         created_at, updated_at \
         FROM budgets WHERE tenant_id = $1 ORDER BY period_start DESC",
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to list budgets: {}", e)))?;

    Ok(Json(rows))
}

/// POST /api/v1/budgets
async fn create_budget(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Json(body): Json<CreateBudgetRequest>,
) -> ApiResult<impl IntoResponse> {
    validate_scope_type(&body.scope_type)?;
    validate_period_type(&body.period_type)?;

    if body.period_start >= body.period_end {
        return Err(billforge_core::Error::Validation(
            "period_start must be before period_end".to_string(),
        )
        .into());
    }
    if body.amount_cents < 0 {
        return Err(billforge_core::Error::Validation(
            "amount_cents must be non-negative".to_string(),
        )
        .into());
    }

    let enforcement = body.enforcement.unwrap_or_else(|| "warn".to_string());
    validate_enforcement(&enforcement)?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO budgets (id, tenant_id, scope_type, scope_value, period_type, \
         period_start, period_end, amount_cents, enforcement, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&body.scope_type)
    .bind(&body.scope_value)
    .bind(&body.period_type)
    .bind(body.period_start)
    .bind(body.period_end)
    .bind(body.amount_cents)
    .bind(&enforcement)
    .bind(*user.user_id.as_uuid())
    .execute(&*state.db.tenant(&tenant.tenant_id).await?)
    .await
    .map_err(|e| {
        if e.to_string().contains("violates unique constraint") {
            billforge_core::Error::AlreadyExists {
                resource_type: "Budget".to_string(),
            }
        } else {
            billforge_core::Error::Database(format!("Failed to create budget: {}", e))
        }
    })?;

    // Audit log
    let audit_entry = billforge_core::domain::AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        billforge_core::domain::AuditAction::Create,
        billforge_core::domain::ResourceType::Invoice,
        id.to_string(),
        format!(
            "Budget created: {} {} for {} ({})",
            body.scope_type, body.scope_value, body.amount_cents, enforcement
        ),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "budget_create",
        "scope_type": body.scope_type,
        "scope_value": body.scope_value,
        "amount_cents": body.amount_cents,
        "enforcement": enforcement,
    }));
    log_budget_audit(&state, &tenant.tenant_id, audit_entry).await;

    let budget = Budget {
        id,
        tenant_id: *tenant.tenant_id.as_uuid(),
        scope_type: body.scope_type,
        scope_value: body.scope_value,
        period_type: body.period_type,
        period_start: body.period_start,
        period_end: body.period_end,
        amount_cents: body.amount_cents,
        enforcement,
        created_by: Some(*user.user_id.as_uuid()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    Ok((StatusCode::CREATED, Json(budget)))
}

/// PATCH /api/v1/budgets/:id
async fn update_budget(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchBudgetRequest>,
) -> ApiResult<Json<Budget>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify ownership
    let existing = sqlx::query_as::<_, Budget>(
        "SELECT id, tenant_id, scope_type, scope_value, period_type, \
         period_start, period_end, amount_cents, enforcement, created_by, \
         created_at, updated_at \
         FROM budgets WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query budget: {}", e)))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Budget".to_string(),
        id: id.to_string(),
    })?;

    let new_amount = body.amount_cents.unwrap_or(existing.amount_cents);
    let new_enforcement = body.enforcement.unwrap_or(existing.enforcement.clone());

    if new_amount < 0 {
        return Err(billforge_core::Error::Validation(
            "amount_cents must be non-negative".to_string(),
        )
        .into());
    }
    validate_enforcement(&new_enforcement)?;

    sqlx::query(
        "UPDATE budgets SET amount_cents = $1, enforcement = $2, updated_at = NOW() \
         WHERE id = $3 AND tenant_id = $4",
    )
    .bind(new_amount)
    .bind(&new_enforcement)
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to update budget: {}", e)))?;

    // Audit log
    let audit_entry = billforge_core::domain::AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        billforge_core::domain::AuditAction::Update,
        billforge_core::domain::ResourceType::Invoice,
        id.to_string(),
        format!(
            "Budget updated: amount={}, enforcement={}",
            new_amount, new_enforcement
        ),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "budget_update",
        "budget_id": id.to_string(),
        "new_amount_cents": new_amount,
        "new_enforcement": new_enforcement,
    }));
    log_budget_audit(&state, &tenant.tenant_id, audit_entry).await;

    let mut updated = existing;
    updated.amount_cents = new_amount;
    updated.enforcement = new_enforcement;
    updated.updated_at = chrono::Utc::now();

    Ok(Json(updated))
}

/// DELETE /api/v1/budgets/:id
async fn delete_budget(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query("DELETE FROM budgets WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to delete budget: {}", e)))?;

    if rows.rows_affected() == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "Budget".to_string(),
            id: id.to_string(),
        }
        .into());
    }

    // Audit log
    let audit_entry = billforge_core::domain::AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        billforge_core::domain::AuditAction::Delete,
        billforge_core::domain::ResourceType::Invoice,
        id.to_string(),
        "Budget deleted".to_string(),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "action": "budget_delete",
        "budget_id": id.to_string(),
    }));
    log_budget_audit(&state, &tenant.tenant_id, audit_entry).await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/budgets/check?scope_type=&scope_value=&date=&amount_cents=
async fn check_budget(
    State(state): State<AppState>,
    ReportingAccess(_user, tenant): ReportingAccess,
    Query(query): Query<BudgetCheckQuery>,
) -> ApiResult<Json<BudgetCheckResult>> {
    validate_scope_type(&query.scope_type)?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let result = check_single_dimension(
        &pool,
        *tenant.tenant_id.as_uuid(),
        &query.scope_type,
        &query.scope_value,
        query.date,
        query.amount_cents,
    )
    .await?;

    Ok(Json(result))
}

/// GET /api/v1/budgets/check-invoice/:invoice_id
async fn check_invoice_budget(
    State(state): State<AppState>,
    ReportingAccess(_user, tenant): ReportingAccess,
    Path(invoice_id): Path<Uuid>,
) -> ApiResult<Json<InvoiceBudgetCheckResult>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let result =
        check_invoice_against_budgets(&pool, *tenant.tenant_id.as_uuid(), invoice_id).await?;
    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// Public helper: check invoice against all relevant budgets
// ---------------------------------------------------------------------------

/// Check an invoice against all matching budgets before approval.
///
/// Reads the invoice's `department`, `cost_center`, `gl_code`, and `project` fields,
/// finds active budget rows for the period containing the invoice date, sums already-
/// approved invoices in that period for each scope, and returns per-dimension results.
pub async fn check_invoice_against_budgets(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    invoice_id: Uuid,
) -> billforge_core::Result<InvoiceBudgetCheckResult> {
    // 1. Read invoice dimensions + amount + date
    // NOTE: project is stored inside custom_fields JSONB (no dedicated column).
    let row = sqlx::query(
        "SELECT department, cost_center, gl_code, total_amount_cents, \
         COALESCE(invoice_date, created_at::date) as invoice_date, \
         custom_fields \
         FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query invoice: {}", e)))?;

    let row = row.ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Invoice".to_string(),
        id: invoice_id.to_string(),
    })?;

    let department: Option<String> = row.try_get("department").ok();
    let cost_center: Option<String> = row.try_get("cost_center").ok();
    let gl_code: Option<String> = row.try_get("gl_code").ok();
    // Project is stored in custom_fields JSONB (invoices table has no project column)
    let project: Option<String> = {
        let custom_fields: Option<serde_json::Value> = row.try_get("custom_fields").ok();
        custom_fields
            .as_ref()
            .and_then(|cf| cf.get("project"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let amount_cents: i64 = row.try_get("total_amount_cents").unwrap_or(0);
    let invoice_date: NaiveDate = row
        .try_get("invoice_date")
        .unwrap_or_else(|_| chrono::Utc::now().date_naive());

    // 2. Build dimension list
    let mut dimensions: Vec<(&str, &str)> = Vec::new();
    if let Some(ref v) = department {
        dimensions.push(("department", v.as_str()));
    }
    if let Some(ref v) = cost_center {
        dimensions.push(("cost_center", v.as_str()));
    }
    if let Some(ref v) = gl_code {
        dimensions.push(("gl_account", v.as_str()));
    }
    if let Some(ref v) = project {
        dimensions.push(("project", v.as_str()));
    }

    // 3. If no dimensions, no budget checks apply
    if dimensions.is_empty() {
        return Ok(InvoiceBudgetCheckResult {
            results: vec![],
            blocked: false,
            warnings: vec![],
            violations: vec![],
        });
    }

    // 4. Check each dimension
    let mut results = Vec::with_capacity(dimensions.len());
    for (scope_type, scope_value) in &dimensions {
        let check = check_single_dimension(
            pool,
            tenant_id,
            scope_type,
            scope_value,
            invoice_date,
            amount_cents,
        )
        .await?;
        results.push(check);
    }

    let violations: Vec<BudgetCheckResult> = results
        .iter()
        .filter(|r| r.status == "block")
        .cloned()
        .collect();
    let warnings: Vec<BudgetCheckResult> = results
        .iter()
        .filter(|r| r.status == "warn")
        .cloned()
        .collect();

    Ok(InvoiceBudgetCheckResult {
        blocked: !violations.is_empty(),
        warnings,
        violations,
        results,
    })
}

/// Check a single scope dimension against its active budget.
async fn check_single_dimension(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    scope_type: &str,
    scope_value: &str,
    date: NaiveDate,
    amount_cents: i64,
) -> billforge_core::Result<BudgetCheckResult> {
    // Find the active budget for this scope + period containing the date
    let budget_row = sqlx::query(
        "SELECT id, amount_cents, enforcement FROM budgets \
         WHERE tenant_id = $1 AND scope_type = $2 AND scope_value = $3 \
         AND period_start <= $4 AND period_end >= $4 \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(scope_type)
    .bind(scope_value)
    .bind(date)
    .fetch_optional(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query budget: {}", e)))?;

    let (budget_id, budget_amount, enforcement) = match budget_row {
        Some(row) => {
            let id: Uuid = row.try_get("id").unwrap_or_default();
            let amt: i64 = row.try_get("amount_cents").unwrap_or(0);
            let enf: String = row
                .try_get("enforcement")
                .unwrap_or_else(|_| "warn".to_string());
            (Some(id), amt, enf)
        }
        None => {
            // No budget configured for this dimension => ok
            return Ok(BudgetCheckResult {
                scope_type: scope_type.to_string(),
                scope_value: scope_value.to_string(),
                budget_amount_cents: 0,
                committed_cents: 0,
                remaining_after_cents: 0,
                enforcement: "none".to_string(),
                status: "ok".to_string(),
            });
        }
    };

    // Sum already-approved invoices in this period matching the same scope
    let committed: i64 = sum_committed_for_scope(
        pool,
        tenant_id,
        scope_type,
        scope_value,
        date,
        budget_id.unwrap(),
    )
    .await?;

    let remaining_after = budget_amount - committed - amount_cents;

    let status = if remaining_after < 0 {
        if enforcement == "block" {
            "block".to_string()
        } else {
            "warn".to_string()
        }
    } else {
        "ok".to_string()
    };

    Ok(BudgetCheckResult {
        scope_type: scope_type.to_string(),
        scope_value: scope_value.to_string(),
        budget_amount_cents: budget_amount,
        committed_cents: committed + amount_cents,
        remaining_after_cents: remaining_after,
        enforcement,
        status,
    })
}

/// Sum total_amount_cents of approved invoices matching a scope dimension
/// within the budget period.
async fn sum_committed_for_scope(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    scope_type: &str,
    scope_value: &str,
    date: NaiveDate,
    _budget_id: Uuid,
) -> billforge_core::Result<i64> {
    // Look up the budget period to bound the sum
    let period: Option<(NaiveDate, NaiveDate)> = sqlx::query_as(
        "SELECT period_start, period_end FROM budgets \
         WHERE tenant_id = $1 AND scope_type = $2 AND scope_value = $3 \
         AND period_start <= $4 AND period_end >= $4 \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(scope_type)
    .bind(scope_value)
    .bind(date)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to query budget period: {}", e))
    })?;

    let Some((period_start, period_end)) = period else {
        return Ok(0);
    };

    // Build the WHERE clause for the scope column.
    // "project" is stored inside custom_fields JSONB, not a dedicated column.
    let query = if scope_type == "project" {
        format!(
            "SELECT COALESCE(SUM(total_amount_cents), 0) as total \
             FROM invoices \
             WHERE tenant_id = $1 \
             AND status IN ('approved', 'paid') \
             AND custom_fields->>'project' = $2 \
             AND COALESCE(invoice_date, created_at::date) >= $3 \
             AND COALESCE(invoice_date, created_at::date) <= $4"
        )
    } else {
        let scope_column = match scope_type {
            "gl_account" => "gl_code",
            other => other,
        };
        format!(
            "SELECT COALESCE(SUM(total_amount_cents), 0) as total \
             FROM invoices \
             WHERE tenant_id = $1 \
             AND status IN ('approved', 'paid') \
             AND {} = $2 \
             AND COALESCE(invoice_date, created_at::date) >= $3 \
             AND COALESCE(invoice_date, created_at::date) <= $4",
            scope_column
        )
    };

    let total: i64 = sqlx::query_scalar(&query)
        .bind(tenant_id)
        .bind(scope_value)
        .bind(period_start)
        .bind(period_end)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to sum committed amounts: {}", e))
        })?;

    Ok(total)
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

pub fn validate_scope_type(scope_type: &str) -> billforge_core::Result<()> {
    if !["department", "cost_center", "gl_account", "project"].contains(&scope_type) {
        return Err(billforge_core::Error::Validation(format!(
            "Invalid scope_type '{}'. Must be department, cost_center, gl_account, or project",
            scope_type
        )));
    }
    Ok(())
}

pub fn validate_period_type(period_type: &str) -> billforge_core::Result<()> {
    if !["monthly", "quarterly", "annual"].contains(&period_type) {
        return Err(billforge_core::Error::Validation(format!(
            "Invalid period_type '{}'. Must be monthly, quarterly, or annual",
            period_type
        )));
    }
    Ok(())
}

pub fn validate_enforcement(enforcement: &str) -> billforge_core::Result<()> {
    if !["warn", "block"].contains(&enforcement) {
        return Err(billforge_core::Error::Validation(format!(
            "Invalid enforcement '{}'. Must be warn or block",
            enforcement
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Audit helper
// ---------------------------------------------------------------------------

async fn log_budget_audit(
    state: &AppState,
    tenant_id: &billforge_core::TenantId,
    entry: billforge_core::domain::AuditEntry,
) {
    if let Ok(pool) = state.db.tenant(tenant_id).await {
        let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
        if let Err(e) = audit_repo.log(entry).await {
            tracing::warn!(error = %e, "Failed to log budget audit entry");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_scope_type_valid() {
        assert!(validate_scope_type("department").is_ok());
        assert!(validate_scope_type("cost_center").is_ok());
        assert!(validate_scope_type("gl_account").is_ok());
        assert!(validate_scope_type("project").is_ok());
    }

    #[test]
    fn test_validate_scope_type_invalid() {
        assert!(validate_scope_type("vendor").is_err());
        assert!(validate_scope_type("").is_err());
    }

    #[test]
    fn test_validate_period_type_valid() {
        assert!(validate_period_type("monthly").is_ok());
        assert!(validate_period_type("quarterly").is_ok());
        assert!(validate_period_type("annual").is_ok());
    }

    #[test]
    fn test_validate_period_type_invalid() {
        assert!(validate_period_type("weekly").is_err());
    }

    #[test]
    fn test_validate_enforcement_valid() {
        assert!(validate_enforcement("warn").is_ok());
        assert!(validate_enforcement("block").is_ok());
    }

    #[test]
    fn test_validate_enforcement_invalid() {
        assert!(validate_enforcement("strict").is_err());
    }

    #[test]
    fn test_budget_check_result_status_block_when_over_and_block_enforcement() {
        let result = BudgetCheckResult {
            scope_type: "department".to_string(),
            scope_value: "eng".to_string(),
            budget_amount_cents: 10000,
            committed_cents: 12000,
            remaining_after_cents: -2000,
            enforcement: "block".to_string(),
            status: "block".to_string(),
        };
        assert_eq!(result.status, "block");
    }

    #[test]
    fn test_budget_check_result_status_warn_when_over_and_warn_enforcement() {
        let result = BudgetCheckResult {
            scope_type: "department".to_string(),
            scope_value: "eng".to_string(),
            budget_amount_cents: 10000,
            committed_cents: 12000,
            remaining_after_cents: -2000,
            enforcement: "warn".to_string(),
            status: "warn".to_string(),
        };
        assert_eq!(result.status, "warn");
    }

    #[test]
    fn test_invoice_budget_check_empty_dimensions() {
        let check = InvoiceBudgetCheckResult {
            results: vec![],
            blocked: false,
            warnings: vec![],
            violations: vec![],
        };
        assert!(!check.blocked);
        assert!(check.results.is_empty());
    }
}
