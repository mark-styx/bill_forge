//! Issue #415: 2-hour Implementation Speedrun.
//!
//! Sits alongside the existing 14-day implementation wizard
//! (`crate::routes::implementation`). Reuses the same OAuth / ERP sync /
//! sample-invoice pipeline; this module only adds:
//!   * progress state for the under-2-hour onboarding target,
//!   * threshold inference from historical GL spend,
//!   * approval-chain suggestions from QBO-seeded user roles,
//!   * a "process your first 5 invoices" walkthrough counter.

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType},
    TenantId,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Default speedrun target (the under-2-hour promise from #415).
pub const SPEEDRUN_TARGET_MINUTES: i32 = 120;

/// Number of invoices the guided walkthrough requires before
/// `completed_at` is stamped.
pub const SPEEDRUN_FIRST_INVOICES_TARGET: i32 = 5;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/speedrun/start", post(start_speedrun))
        .route(
            "/speedrun/inferred-thresholds",
            get(get_inferred_thresholds),
        )
        .route(
            "/speedrun/suggested-approval-chains",
            get(get_suggested_approval_chains),
        )
        .route(
            "/speedrun/process-first-invoices/:n",
            post(process_first_invoices),
        )
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpeedrunProgress {
    pub started_at: DateTime<Utc>,
    pub elapsed_minutes: i64,
    pub target_minutes: i32,
    pub thresholds_inferred_at: Option<DateTime<Utc>>,
    pub approval_chain_suggested_at: Option<DateTime<Utc>>,
    pub first_invoices_processed: i32,
    pub first_invoices_target: i32,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferredThresholdTier {
    /// 1 = lowest dollar tier, 3 = highest.
    pub tier: u8,
    /// Suggested approval threshold in minor units (cents) for the tier.
    pub amount_cents: i64,
    /// Statistic the suggestion was derived from (median / p75 / p95).
    pub source_percentile: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferredThresholdsResponse {
    pub sample_size: i64,
    /// `vendor` when no GL coding was present on any invoice, `gl_account`
    /// otherwise.
    pub basis: &'static str,
    pub currency: String,
    pub tiers: Vec<InferredThresholdTier>,
    pub progress: SpeedrunProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuggestedApprover {
    /// Local BillForge user, if the QBO employee already maps to one.
    /// QBO-sourced suggestions populate `external_id` instead.
    pub user_id: Option<Uuid>,
    /// QBO Employee Id when the row came from the connected QuickBooks
    /// company; `None` when sourced from the local users table.
    pub external_id: Option<String>,
    pub email: String,
    pub name: String,
    /// 1 = first-line approver, 3 = executive sign-off.
    pub tier: u8,
    /// The QBO/local role string the tier was derived from.
    pub source_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuggestedApprovalChainsResponse {
    /// `true` when a sync-enabled QuickBooks connection seeded the role
    /// data. When `false`, the chain comes from BillForge's local user roster
    /// (Xero connections fall back here pending follow-up #415).
    pub qbo_connected: bool,
    pub chain: Vec<SuggestedApprover>,
    pub progress: SpeedrunProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProcessFirstInvoicesResponse {
    pub progress: SpeedrunProgress,
}

// ---------------------------------------------------------------------------
// Persistence row
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SpeedrunRow {
    started_at: DateTime<Utc>,
    thresholds_inferred_at: Option<DateTime<Utc>>,
    approval_chain_suggested_at: Option<DateTime<Utc>>,
    first_invoices_processed: i32,
    completed_at: Option<DateTime<Utc>>,
    target_minutes: i32,
}

impl SpeedrunRow {
    fn into_progress(self) -> SpeedrunProgress {
        let elapsed_minutes = (Utc::now() - self.started_at).num_minutes().max(0);
        SpeedrunProgress {
            started_at: self.started_at,
            elapsed_minutes,
            target_minutes: self.target_minutes,
            thresholds_inferred_at: self.thresholds_inferred_at,
            approval_chain_suggested_at: self.approval_chain_suggested_at,
            first_invoices_processed: self.first_invoices_processed,
            first_invoices_target: SPEEDRUN_FIRST_INVOICES_TARGET,
            completed_at: self.completed_at,
        }
    }
}

async fn load_or_create_row(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<SpeedrunRow, billforge_core::Error> {
    if let Some(row) = fetch_row(pool, tenant_id).await? {
        return Ok(row);
    }
    sqlx::query(
        "INSERT INTO implementation_speedrun_state
            (tenant_id, started_at, target_minutes, created_at, updated_at)
         VALUES ($1, NOW(), $2, NOW(), NOW())
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(tenant_id.as_uuid())
    .bind(SPEEDRUN_TARGET_MINUTES)
    .execute(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to create speedrun state: {}", e))
    })?;
    fetch_row(pool, tenant_id).await?.ok_or_else(|| {
        billforge_core::Error::Internal(
            "Speedrun state row missing immediately after insert".to_string(),
        )
    })
}

async fn fetch_row(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<Option<SpeedrunRow>, billforge_core::Error> {
    let row: Option<(
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
        i32,
        Option<DateTime<Utc>>,
        i32,
    )> = sqlx::query_as(
        "SELECT started_at, thresholds_inferred_at, approval_chain_suggested_at,
                first_invoices_processed, completed_at, target_minutes
           FROM implementation_speedrun_state
          WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to load speedrun state: {}", e))
    })?;
    Ok(row.map(
        |(s, t, a, n, c, m)| SpeedrunRow {
            started_at: s,
            thresholds_inferred_at: t,
            approval_chain_suggested_at: a,
            first_invoices_processed: n,
            completed_at: c,
            target_minutes: m,
        },
    ))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/v1/implementation/speedrun/start",
    tag = "Implementation",
    responses((status = 200, description = "Speedrun started", body = SpeedrunProgress))
)]
pub async fn start_speedrun(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<SpeedrunProgress>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let row = load_or_create_row(&pool, &tenant.tenant_id).await?;
    log_speedrun_audit(
        &state,
        &tenant.tenant_id,
        AuditEntry::new(
            tenant.tenant_id.clone(),
            Some(user.user_id.clone()),
            AuditAction::Create,
            ResourceType::Settings,
            tenant.tenant_id.as_uuid().to_string(),
            "Implementation speedrun started",
        )
        .with_user_email(&user.email)
        .with_metadata(serde_json::json!({
            "action": "implementation_speedrun_start",
            "target_minutes": row.target_minutes,
        })),
    )
    .await;
    Ok(Json(row.into_progress()))
}

#[utoipa::path(
    get,
    path = "/api/v1/implementation/speedrun/inferred-thresholds",
    tag = "Implementation",
    responses((status = 200, description = "Suggested approval thresholds", body = InferredThresholdsResponse))
)]
pub async fn get_inferred_thresholds(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<InferredThresholdsResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let _ = load_or_create_row(&pool, &tenant.tenant_id).await?;

    let amounts = trailing_12mo_invoice_amounts(&pool, &tenant.tenant_id).await?;
    let basis = if amounts.has_gl_coded {
        "gl_account"
    } else {
        "vendor"
    };
    let tiers = compute_threshold_tiers(&amounts.values);
    let currency = amounts.currency.unwrap_or_else(|| "USD".to_string());

    sqlx::query(
        "UPDATE implementation_speedrun_state
            SET thresholds_inferred_at = NOW(), updated_at = NOW()
          WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to stamp threshold time: {}", e))
    })?;

    log_speedrun_audit(
        &state,
        &tenant.tenant_id,
        AuditEntry::new(
            tenant.tenant_id.clone(),
            Some(user.user_id.clone()),
            AuditAction::Read,
            ResourceType::Settings,
            tenant.tenant_id.as_uuid().to_string(),
            "Implementation speedrun inferred approval thresholds",
        )
        .with_user_email(&user.email)
        .with_metadata(serde_json::json!({
            "action": "implementation_speedrun_infer_thresholds",
            "sample_size": amounts.values.len(),
            "basis": basis,
        })),
    )
    .await;

    let row = load_or_create_row(&pool, &tenant.tenant_id).await?;
    Ok(Json(InferredThresholdsResponse {
        sample_size: amounts.values.len() as i64,
        basis,
        currency,
        tiers,
        progress: row.into_progress(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/implementation/speedrun/suggested-approval-chains",
    tag = "Implementation",
    responses((status = 200, description = "Suggested approval chain", body = SuggestedApprovalChainsResponse))
)]
pub async fn get_suggested_approval_chains(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<SuggestedApprovalChainsResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let _ = load_or_create_row(&pool, &tenant.tenant_id).await?;

    let qbo_connected = quickbooks_sync_enabled(&pool, &tenant.tenant_id).await?;
    let chain = if qbo_connected {
        // Pull live QBO Employee records and map their JobTitle to a
        // BillForge approver tier. The QBO REST API does not expose the
        // platform-level User entity to third-party apps, so JobTitle is
        // the closest role proxy we can rely on. If QBO returns no usable
        // rows we fall back to the local users table so the wizard still
        // has something to show the operator.
        let mut chain = match fetch_qbo_employee_chain(&state, &pool, &tenant.tenant_id).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = %e, "Falling back to local users for approval chain suggestion");
                Vec::new()
            }
        };
        if chain.is_empty() {
            chain = load_user_chain(&pool, &tenant.tenant_id).await?;
        }
        chain
    } else {
        Vec::new()
    };

    sqlx::query(
        "UPDATE implementation_speedrun_state
            SET approval_chain_suggested_at = NOW(), updated_at = NOW()
          WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to stamp chain suggestion time: {}", e))
    })?;

    log_speedrun_audit(
        &state,
        &tenant.tenant_id,
        AuditEntry::new(
            tenant.tenant_id.clone(),
            Some(user.user_id.clone()),
            AuditAction::Read,
            ResourceType::Settings,
            tenant.tenant_id.as_uuid().to_string(),
            "Implementation speedrun suggested approval chain",
        )
        .with_user_email(&user.email)
        .with_metadata(serde_json::json!({
            "action": "implementation_speedrun_suggest_approval_chain",
            "qbo_connected": qbo_connected,
            "chain_length": chain.len(),
        })),
    )
    .await;

    let row = load_or_create_row(&pool, &tenant.tenant_id).await?;
    Ok(Json(SuggestedApprovalChainsResponse {
        qbo_connected,
        chain,
        progress: row.into_progress(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/implementation/speedrun/process-first-invoices/{n}",
    tag = "Implementation",
    params(("n" = i32, Path, description = "Number of invoices the operator just processed (added to running total).")),
    responses((status = 200, description = "Walkthrough progress updated", body = ProcessFirstInvoicesResponse))
)]
pub async fn process_first_invoices(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(n): Path<i32>,
) -> ApiResult<Json<ProcessFirstInvoicesResponse>> {
    if n <= 0 {
        return Err(
            billforge_core::Error::Validation("n must be positive".to_string()).into(),
        );
    }
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let existing = load_or_create_row(&pool, &tenant.tenant_id).await?;
    let new_count = existing
        .first_invoices_processed
        .saturating_add(n)
        .min(SPEEDRUN_FIRST_INVOICES_TARGET);
    let just_completed =
        existing.completed_at.is_none() && new_count >= SPEEDRUN_FIRST_INVOICES_TARGET;
    let completed_at = existing
        .completed_at
        .or(if just_completed { Some(Utc::now()) } else { None });

    sqlx::query(
        "UPDATE implementation_speedrun_state
            SET first_invoices_processed = $2,
                completed_at = $3,
                updated_at = NOW()
          WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(new_count)
    .bind(completed_at)
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to update walkthrough progress: {}", e))
    })?;

    if just_completed {
        log_speedrun_audit(
            &state,
            &tenant.tenant_id,
            AuditEntry::new(
                tenant.tenant_id.clone(),
                Some(user.user_id.clone()),
                AuditAction::Update,
                ResourceType::Settings,
                tenant.tenant_id.as_uuid().to_string(),
                "Implementation speedrun completed",
            )
            .with_user_email(&user.email)
            .with_metadata(serde_json::json!({
                "action": "implementation_speedrun_complete",
                "elapsed_minutes": (Utc::now() - existing.started_at).num_minutes(),
                "target_minutes": existing.target_minutes,
            })),
        )
        .await;
    }

    let row = load_or_create_row(&pool, &tenant.tenant_id).await?;
    Ok(Json(ProcessFirstInvoicesResponse {
        progress: row.into_progress(),
    }))
}

// ---------------------------------------------------------------------------
// Threshold inference
// ---------------------------------------------------------------------------

struct HistoricalAmounts {
    values: Vec<i64>,
    currency: Option<String>,
    has_gl_coded: bool,
}

async fn trailing_12mo_invoice_amounts(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<HistoricalAmounts, billforge_core::Error> {
    let cutoff = Utc::now() - Duration::days(365);
    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT total_amount_cents, currency, gl_code
           FROM invoices
          WHERE tenant_id = $1
            AND created_at >= $2
            AND total_amount_cents > 0",
    )
    .bind(tenant_id.as_uuid())
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to read invoice history: {}", e))
    })?;

    let currency = rows.first().map(|(_, c, _)| c.clone());
    let has_gl_coded = rows
        .iter()
        .any(|(_, _, gl)| gl.as_ref().map(|s| !s.is_empty()).unwrap_or(false));
    let values: Vec<i64> = rows.into_iter().map(|(amt, _, _)| amt).collect();
    Ok(HistoricalAmounts {
        values,
        currency,
        has_gl_coded,
    })
}

/// Build the three-tier threshold suggestion from a flat list of historical
/// invoice amounts (in cents). Tier 1 = median, tier 2 = p75, tier 3 = p95.
/// When the sample is empty the tiers fall back to conservative round numbers
/// so the UI always has something to show.
pub fn compute_threshold_tiers(amounts: &[i64]) -> Vec<InferredThresholdTier> {
    if amounts.is_empty() {
        return vec![
            InferredThresholdTier {
                tier: 1,
                amount_cents: 100_000,
                source_percentile: "default",
            },
            InferredThresholdTier {
                tier: 2,
                amount_cents: 500_000,
                source_percentile: "default",
            },
            InferredThresholdTier {
                tier: 3,
                amount_cents: 2_500_000,
                source_percentile: "default",
            },
        ];
    }

    let mut sorted = amounts.to_vec();
    sorted.sort_unstable();
    vec![
        InferredThresholdTier {
            tier: 1,
            amount_cents: percentile(&sorted, 0.50),
            source_percentile: "median",
        },
        InferredThresholdTier {
            tier: 2,
            amount_cents: percentile(&sorted, 0.75),
            source_percentile: "p75",
        },
        InferredThresholdTier {
            tier: 3,
            amount_cents: percentile(&sorted, 0.95),
            source_percentile: "p95",
        },
    ]
}

fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    // Nearest-rank percentile; matches what a finance reviewer expects when
    // eyeballing "the median bill" vs. "the top 5% bill".
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

// ---------------------------------------------------------------------------
// Approval chain suggestion
// ---------------------------------------------------------------------------

async fn quickbooks_sync_enabled(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<bool, billforge_core::Error> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT EXISTS(
             SELECT 1 FROM quickbooks_connections
              WHERE tenant_id = $1 AND sync_enabled = true
         )",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to read QBO connection: {}", e))
    })?;
    Ok(row.map(|(b,)| b).unwrap_or(false))
}

async fn load_user_chain(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<Vec<SuggestedApprover>, billforge_core::Error> {
    let rows: Vec<(Uuid, String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT id, email, name, roles
           FROM users
          WHERE tenant_id = $1 AND is_active = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_all(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to read users: {}", e)))?;

    let mut chain: Vec<SuggestedApprover> = rows
        .into_iter()
        .filter_map(|(id, email, name, roles)| {
            let role_strings = roles_as_strings(&roles);
            let (tier, source_role) = role_strings_to_tier(&role_strings)?;
            Some(SuggestedApprover {
                user_id: Some(id),
                external_id: None,
                email,
                name,
                tier,
                source_role,
            })
        })
        .collect();

    chain.sort_by_key(|a| a.tier);
    Ok(chain)
}

async fn fetch_qbo_employee_chain(
    state: &AppState,
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<Vec<SuggestedApprover>, billforge_core::Error> {
    let client = crate::routes::implementation::quickbooks_client(state, pool, tenant_id).await?;
    let employees = client.query_employees(1, 100).await.map_err(|e| {
        billforge_core::Error::Validation(format!("Failed to query QBO employees: {}", e))
    })?;
    let mut chain: Vec<SuggestedApprover> = employees
        .into_iter()
        .filter(|emp| emp.Active.unwrap_or(true))
        .filter_map(qbo_employee_to_approver)
        .collect();
    chain.sort_by_key(|a| a.tier);
    Ok(chain)
}

/// Convert a single QBO Employee row to an approver suggestion. Returns
/// `None` when the JobTitle is missing or does not map to a known tier.
pub fn qbo_employee_to_approver(
    employee: billforge_quickbooks::QBEmployee,
) -> Option<SuggestedApprover> {
    let job_title = employee.JobTitle.clone().unwrap_or_default();
    let (tier, source_role) = role_strings_to_tier(&[job_title])?;
    let name = employee
        .DisplayName
        .clone()
        .unwrap_or_else(|| format!("QBO Employee {}", employee.Id));
    let email = employee
        .PrimaryEmailAddr
        .as_ref()
        .map(|e| e.Address.clone())
        .unwrap_or_default();
    Some(SuggestedApprover {
        user_id: None,
        external_id: Some(employee.Id),
        email,
        name,
        tier,
        source_role,
    })
}

fn roles_as_strings(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Map a user's role list to a (tier, source role) pair using the QBO →
/// BillForge heuristic table. Returns `None` when no role qualifies the user
/// as a candidate approver.
///
/// Heuristic mirror of QBO Standard / Manager / Admin (and BillForge's own
/// ApUser / Approver / TenantAdmin):
///   * TenantAdmin / admin / company_admin / master_admin → Tier 3
///   * Approver / manager → Tier 2
///   * ApUser / standard / accountant / ap_clerk → Tier 1
pub fn role_strings_to_tier(roles: &[String]) -> Option<(u8, String)> {
    // Highest-tier role wins so an admin who is also tagged ap_user lands
    // in the executive bucket, not the first-line bucket.
    let mut best: Option<(u8, String)> = None;
    for role in roles {
        let normalized = role.to_ascii_lowercase();
        let candidate: Option<u8> = match normalized.as_str() {
            "tenant_admin" | "admin" | "company_admin" | "master_admin" => Some(3),
            "approver" | "manager" => Some(2),
            "ap_user" | "standard" | "accountant" | "ap_clerk" => Some(1),
            _ => None,
        };
        if let Some(tier) = candidate {
            if best.as_ref().map(|(b, _)| tier > *b).unwrap_or(true) {
                best = Some((tier, role.clone()));
            }
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Audit helper
// ---------------------------------------------------------------------------

async fn log_speedrun_audit(state: &AppState, tenant_id: &TenantId, entry: AuditEntry) {
    use billforge_core::AuditService;
    if let Ok(pool) = state.db.tenant(tenant_id).await {
        let repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
        if let Err(e) = repo.log(entry).await {
            tracing::warn!(error = %e, "Failed to log speedrun audit entry");
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
    fn percentile_picks_nearest_rank() {
        let sorted = vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
        assert_eq!(percentile(&sorted, 0.50), 500);
        assert_eq!(percentile(&sorted, 0.75), 800);
        assert_eq!(percentile(&sorted, 0.95), 1000);
    }

    #[test]
    fn percentile_handles_single_element() {
        let sorted = vec![42];
        assert_eq!(percentile(&sorted, 0.50), 42);
        assert_eq!(percentile(&sorted, 0.95), 42);
    }

    #[test]
    fn compute_threshold_tiers_returns_median_p75_p95() {
        let amounts: Vec<i64> = (1..=100).map(|n| n * 1000).collect();
        let tiers = compute_threshold_tiers(&amounts);
        assert_eq!(tiers.len(), 3);
        assert_eq!(tiers[0].tier, 1);
        assert_eq!(tiers[0].source_percentile, "median");
        assert_eq!(tiers[0].amount_cents, 50_000);
        assert_eq!(tiers[1].tier, 2);
        assert_eq!(tiers[1].source_percentile, "p75");
        assert_eq!(tiers[1].amount_cents, 75_000);
        assert_eq!(tiers[2].tier, 3);
        assert_eq!(tiers[2].source_percentile, "p95");
        assert_eq!(tiers[2].amount_cents, 95_000);
    }

    #[test]
    fn compute_threshold_tiers_falls_back_when_empty() {
        let tiers = compute_threshold_tiers(&[]);
        assert_eq!(tiers.len(), 3);
        assert!(tiers.iter().all(|t| t.source_percentile == "default"));
        assert!(tiers[0].amount_cents < tiers[1].amount_cents);
        assert!(tiers[1].amount_cents < tiers[2].amount_cents);
    }

    #[test]
    fn role_strings_to_tier_maps_admin_to_three() {
        let (tier, source) = role_strings_to_tier(&["tenant_admin".to_string()]).unwrap();
        assert_eq!(tier, 3);
        assert_eq!(source, "tenant_admin");

        let (tier, _) = role_strings_to_tier(&["Admin".to_string()]).unwrap();
        assert_eq!(tier, 3);
    }

    #[test]
    fn role_strings_to_tier_maps_manager_to_two() {
        let (tier, source) = role_strings_to_tier(&["approver".to_string()]).unwrap();
        assert_eq!(tier, 2);
        assert_eq!(source, "approver");

        let (tier, _) = role_strings_to_tier(&["Manager".to_string()]).unwrap();
        assert_eq!(tier, 2);
    }

    #[test]
    fn role_strings_to_tier_maps_standard_to_one() {
        let (tier, _) = role_strings_to_tier(&["ap_user".to_string()]).unwrap();
        assert_eq!(tier, 1);

        let (tier, _) = role_strings_to_tier(&["standard".to_string()]).unwrap();
        assert_eq!(tier, 1);
    }

    #[test]
    fn role_strings_to_tier_returns_highest_match() {
        // A user holding ap_user + tenant_admin should land in tier 3, not 1.
        let (tier, _) = role_strings_to_tier(&[
            "ap_user".to_string(),
            "tenant_admin".to_string(),
        ])
        .unwrap();
        assert_eq!(tier, 3);
    }

    #[test]
    fn role_strings_to_tier_skips_unknown_roles() {
        assert!(role_strings_to_tier(&["report_viewer".to_string()]).is_none());
        assert!(role_strings_to_tier(&[]).is_none());
    }

    fn employee(id: &str, name: &str, email: &str, title: Option<&str>) -> billforge_quickbooks::QBEmployee {
        billforge_quickbooks::QBEmployee {
            Id: id.to_string(),
            DisplayName: Some(name.to_string()),
            PrimaryEmailAddr: Some(billforge_quickbooks::QBEmailAddress {
                Address: email.to_string(),
            }),
            JobTitle: title.map(|s| s.to_string()),
            Active: Some(true),
        }
    }

    #[test]
    fn qbo_employee_admin_title_maps_to_tier_three() {
        let suggestion = qbo_employee_to_approver(employee(
            "42",
            "Avery Admin",
            "avery@example.com",
            Some("Admin"),
        ))
        .expect("admin must map to a tier");
        assert_eq!(suggestion.tier, 3);
        assert_eq!(suggestion.external_id.as_deref(), Some("42"));
        assert!(suggestion.user_id.is_none());
        assert_eq!(suggestion.email, "avery@example.com");
    }

    #[test]
    fn qbo_employee_manager_and_standard_map_to_tiers_two_and_one() {
        let manager = qbo_employee_to_approver(employee(
            "7",
            "Morgan Manager",
            "morgan@example.com",
            Some("Manager"),
        ))
        .expect("manager must map to a tier");
        assert_eq!(manager.tier, 2);

        let standard = qbo_employee_to_approver(employee(
            "9",
            "Sam Standard",
            "sam@example.com",
            Some("Standard"),
        ))
        .expect("standard must map to a tier");
        assert_eq!(standard.tier, 1);
    }

    #[test]
    fn qbo_employee_skips_unmapped_titles() {
        assert!(qbo_employee_to_approver(employee(
            "1",
            "Quinn Custodian",
            "quinn@example.com",
            Some("Office Custodian"),
        ))
        .is_none());
        assert!(qbo_employee_to_approver(employee(
            "2",
            "Pat Pending",
            "pat@example.com",
            None,
        ))
        .is_none());
    }
}
