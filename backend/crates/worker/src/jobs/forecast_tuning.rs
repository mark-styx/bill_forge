//! Background job: Forecast Model Tuning (issue #367)
//!
//! Closes the 'learns from outcomes' loop for the forecasting surface by
//! reading the last 30 days of realized-vs-predicted rows from
//! `forecast_accuracy_log` (written by `PredictiveService::calculate_forecast_accuracy`),
//! computing per-tenant MAPE and signed bias, and upserting learned
//! `ArimaForecaster` parameter overrides into `forecast_model_tuning`.
//!
//! `PredictiveService` then feeds those overrides into `ArimaForecaster::with_tuning`
//! on the next forecast run, so the model adapts its seasonality threshold and
//! confidence-interval width from observed per-tenant outcomes.
//!
//! Mirrors the structure of `routing_optimization.rs`: tenant scoping via
//! `pg_manager.tenant()`, a cooldown to avoid churn, and an audit trail entry.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, warn};

use billforge_analytics::predictive_repository::ForecastTuningRow;
use billforge_analytics::PredictiveRepository;
use billforge_core::TenantId;
use billforge_db::PgManager;

/// Lookback window (days) for the realized-vs-predicted aggregate.
const LOOKBACK_DAYS: i32 = 30;

/// MAPE above this widens the confidence interval. Units: percent.
const HIGH_MAPE_THRESHOLD: f64 = 25.0;

/// CI half-width multiplier applied when MAPE exceeds the threshold.
const HIGH_MAPE_CI_MULTIPLIER: f64 = 1.25;

/// |signed bias| above this loosens the seasonality autocorrelation threshold.
/// Units: percent.
const HIGH_BIAS_THRESHOLD: f64 = 10.0;

/// Historical default autocorrelation threshold (matches ArimaForecaster).
const DEFAULT_SEASONALITY_THRESHOLD: f64 = 0.5;

/// How much to lower the seasonality threshold when bias is high. A lower
/// threshold makes seasonality detection more permissive.
const BIAS_SEASONALITY_LOOSEN_STEP: f64 = 0.05;

/// Safe bounds for the learned overrides.
const SEASONALITY_THRESHOLD_MIN: f64 = 0.1;
const SEASONALITY_THRESHOLD_MAX: f64 = 0.9;
const CI_MULTIPLIER_MIN: f64 = 1.0;
const CI_MULTIPLIER_MAX: f64 = 2.0;

/// Fraction of observed signed bias folded back into the next forecast's
/// predicted level (issue #398). Damping below 1.0 keeps the closed loop from
/// oscillating when residual noise inflates the signed bias estimate.
const BIAS_CORRECTION_DAMPING: f64 = 0.5;

/// Safe bounds for the learned level-bias correction. Matches
/// `LEVEL_BIAS_CORRECTION_MIN/MAX` in `billforge_analytics::forecasting`.
const LEVEL_BIAS_CORRECTION_MIN: f64 = -0.5;
const LEVEL_BIAS_CORRECTION_MAX: f64 = 0.5;

/// Cooldown between tuning writes for the same tenant (hours).
const TUNING_COOLDOWN_HOURS: i64 = 24;

/// Per-tenant `ArimaForecaster` parameter overrides the worker computed from
/// observed accuracy. Persisted into `forecast_model_tuning`.
#[derive(Debug, Clone, PartialEq)]
pub struct TuningDecision {
    /// Override for the 0.5 autocorrelation seasonality threshold. `None`
    /// keeps the historical default.
    pub seasonality_threshold_override: Option<f64>,
    /// Multiplier applied to the 1.96σ CI half-width. `None` keeps 1.0.
    pub ci_width_multiplier: Option<f64>,
    /// Multiplicative correction folded into `ArimaForecaster::forecast()`'s
    /// predicted_value (issue #398). `None` leaves the projected level alone.
    pub level_bias_correction: Option<f64>,
    /// Most recent 30-day MAPE observed for this tenant (percent).
    pub mape_30d: Option<f64>,
}

/// Pure decision logic that turns observed accuracy metrics into the
/// `ArimaForecaster` overrides to persist.
///
/// Rules (mirroring the plan):
/// - No history (`sample_count <= 0`) -> `None`, so the caller writes nothing.
/// - MAPE > 25% -> widen CI multiplier to 1.25 (clamped to `[1.0, 2.0]`).
/// - |signed bias| > 10% -> loosen seasonality threshold by 0.05 (clamped to
///   `[0.1, 0.9]`) AND fold half the observed bias back into the projected
///   level via `level_bias_correction` (issue #398), clamped to `[-0.5, 0.5]`.
///   Sign is flipped because positive bias = forecast overshoot, so the
///   correction must shrink the next forecast.
/// - Otherwise the row still records the observed `mape_30d` for observability,
///   with `None` overrides so `ArimaForecaster` falls back to defaults.
pub fn compute_tuning_decision(
    mape: f64,
    signed_bias_pct: f64,
    sample_count: i64,
) -> Option<TuningDecision> {
    if sample_count <= 0 {
        return None;
    }

    let ci_width_multiplier = if mape > HIGH_MAPE_THRESHOLD {
        Some(HIGH_MAPE_CI_MULTIPLIER.clamp(CI_MULTIPLIER_MIN, CI_MULTIPLIER_MAX))
    } else {
        None
    };

    let (seasonality_threshold_override, level_bias_correction) =
        if signed_bias_pct.abs() > HIGH_BIAS_THRESHOLD {
            let loosened = DEFAULT_SEASONALITY_THRESHOLD - BIAS_SEASONALITY_LOOSEN_STEP;
            let threshold =
                Some(loosened.clamp(SEASONALITY_THRESHOLD_MIN, SEASONALITY_THRESHOLD_MAX));

            let correction = (-signed_bias_pct / 100.0) * BIAS_CORRECTION_DAMPING;
            let correction =
                Some(correction.clamp(LEVEL_BIAS_CORRECTION_MIN, LEVEL_BIAS_CORRECTION_MAX));

            (threshold, correction)
        } else {
            (None, None)
        };

    Some(TuningDecision {
        seasonality_threshold_override,
        ci_width_multiplier,
        level_bias_correction,
        mape_30d: Some(mape),
    })
}

/// Run forecast tuning for all active tenants.
pub async fn run_forecast_tuning(pg_manager: Arc<PgManager>) -> Result<()> {
    info!("Starting forecast tuning job");

    let metadata_pool = pg_manager.metadata();
    let tenants = get_active_tenants(metadata_pool).await?;
    info!("Processing {} tenants for forecast tuning", tenants.len());

    for tenant_id in tenants {
        if let Err(e) = run_tenant_forecast_tuning(pg_manager.clone(), &tenant_id).await {
            warn!("Failed to tune forecast model for tenant {}: {}", tenant_id, e);
        }
    }

    info!("Completed forecast tuning job");
    Ok(())
}

/// Run forecast tuning for a single validated tenant.
pub async fn run_tenant_forecast_tuning(
    pg_manager: Arc<PgManager>,
    tenant_id: &TenantId,
) -> Result<()> {
    let pool = pg_manager.tenant(tenant_id).await?;
    tune_tenant_forecast(&pool, tenant_id).await
}

/// Tune the forecast model for a single tenant against its tenant pool.
///
/// Tenant isolation: every query is bound to `tenant_id` and runs against the
/// tenant-specific pool returned by `pg_manager.tenant()`. No cross-tenant
/// reads or writes occur.
async fn tune_tenant_forecast(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    let tenant_uuid = *tenant_id.as_uuid();
    let tenant_str = tenant_id.as_str();

    // 1. Cooldown: skip if we already tuned within the cooldown window.
    if within_cooldown(pool, tenant_uuid).await? {
        info!(
            tenant_id = %tenant_str,
            cooldown_hours = TUNING_COOLDOWN_HOURS,
            "Skipping forecast tuning (cooldown)"
        );
        return Ok(());
    }

    // 2. Read the last 30 days of realized-vs-predicted rows.
    let repo = PredictiveRepository::new(pool.clone());
    let agg = repo
        .get_recent_forecast_accuracy(tenant_uuid, LOOKBACK_DAYS)
        .await
        .context("Failed to fetch recent forecast accuracy")?;

    // 3. Decide overrides. Empty history writes nothing and never panics.
    let decision = match compute_tuning_decision(agg.mape, agg.signed_bias_pct, agg.sample_count) {
        Some(d) => d,
        None => {
            info!(
                tenant_id = %tenant_str,
                sample_count = agg.sample_count,
                "No forecast accuracy history; skipping tuning"
            );
            return Ok(());
        }
    };

    // 4. Upsert the learned overrides. The repo returns the previous row so we
    //    can capture old -> new values in the audit trail.
    let previous = repo
        .upsert_forecast_tuning(
            tenant_uuid,
            decision.seasonality_threshold_override,
            decision.ci_width_multiplier,
            decision.level_bias_correction,
            decision.mape_30d,
        )
        .await
        .context("Failed to upsert forecast_model_tuning row")?;

    // 5. Audit trail.
    write_tuning_audit(pool, tenant_uuid, &previous, &decision, agg.sample_count).await?;

    info!(
        tenant_id = %tenant_str,
        mape = agg.mape,
        bias_pct = agg.signed_bias_pct,
        sample_count = agg.sample_count,
        ci_multiplier = ?decision.ci_width_multiplier,
        seasonality_threshold = ?decision.seasonality_threshold_override,
        level_bias_correction = ?decision.level_bias_correction,
        "Updated forecast model tuning"
    );

    Ok(())
}

/// Returns true if the tenant's tuning row was updated within the cooldown
/// window. A missing row (never tuned) returns false.
async fn within_cooldown(pool: &PgPool, tenant_id: uuid::Uuid) -> Result<bool> {
    let last_updated: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT updated_at FROM forecast_model_tuning WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read forecast_model_tuning updated_at")?;

    let Some(last) = last_updated else {
        return Ok(false);
    };

    let elapsed = Utc::now() - last;
    Ok(elapsed.num_hours() < TUNING_COOLDOWN_HOURS)
}

/// Write an audit-log row capturing the previous and new tuning values, plus
/// the sample size that drove the decision. Uses the same `workflow_audit_log`
/// table the routing optimizer writes to.
async fn write_tuning_audit(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    previous: &Option<ForecastTuningRow>,
    decision: &TuningDecision,
    sample_count: i64,
) -> Result<()> {
    let old_values = match previous {
        Some(row) => json!({
            "seasonality_threshold_override": row.seasonality_threshold_override,
            "ci_width_multiplier": row.ci_width_multiplier,
            "level_bias_correction": row.level_bias_correction,
            "mape_30d": row.mape_30d,
        }),
        None => serde_json::Value::Null,
    };
    let new_values = json!({
        "seasonality_threshold_override": decision.seasonality_threshold_override,
        "ci_width_multiplier": decision.ci_width_multiplier,
        "level_bias_correction": decision.level_bias_correction,
        "mape_30d": decision.mape_30d,
    });

    sqlx::query(
        r#"
        INSERT INTO workflow_audit_log (
            id, tenant_id, entity_type, entity_id, action,
            actor_type, old_values, new_values, metadata, created_at
        ) VALUES (
            gen_random_uuid(), $1, 'ForecastModelTuning', $2,
            'forecast_tuning.updated', 'system:forecast_tuner',
            $3, $4, $5, NOW()
        )
        "#,
    )
    .bind(tenant_id)
    .bind(tenant_id)
    .bind(&old_values)
    .bind(&new_values)
    .bind(json!({
        "lookback_days": LOOKBACK_DAYS,
        "sample_count": sample_count,
    }))
    .execute(pool)
    .await
    .context("Failed to insert forecast tuning audit entry")?;

    Ok(())
}

const TENANT_DISCOVERY_SQL: &str = "SELECT id FROM tenants WHERE is_active = true";

/// Fetch all active tenants from the shared metadata pool.
async fn get_active_tenants(pool: &PgPool) -> Result<Vec<TenantId>> {
    let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(TENANT_DISCOVERY_SQL)
        .fetch_all(pool)
        .await
        .context("Failed to fetch active tenants")?;

    Ok(rows.into_iter().map(|(id,)| TenantId::from_uuid(id)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history_writes_no_tuning() {
        // No realized-vs-predicted rows: nothing to learn from.
        assert_eq!(compute_tuning_decision(50.0, 20.0, 0), None);
        assert_eq!(compute_tuning_decision(50.0, 20.0, -1), None);
    }

    #[test]
    fn high_mape_widens_ci_multiplier() {
        let d = compute_tuning_decision(30.0, 0.0, 10).expect("history present");
        assert_eq!(d.ci_width_multiplier, Some(1.25));
        // MAPE under control: no seasonality loosening, no bias correction.
        assert!(d.seasonality_threshold_override.is_none());
        assert!(d.level_bias_correction.is_none());
        // Observed MAPE is recorded for observability.
        assert_eq!(d.mape_30d, Some(30.0));
    }

    #[test]
    fn threshold_mape_does_not_trigger_widening() {
        // MAPE exactly at threshold is not strictly greater, so no widening.
        let d = compute_tuning_decision(25.0, 0.0, 10).expect("history present");
        assert!(d.ci_width_multiplier.is_none());
    }

    #[test]
    fn high_bias_loosens_seasonality_threshold() {
        // Positive bias > 10%: 0.5 - 0.05 = 0.45.
        let d = compute_tuning_decision(10.0, 15.0, 10).expect("history present");
        assert_eq!(d.seasonality_threshold_override, Some(0.45));
        // MAPE under control: no CI widening.
        assert!(d.ci_width_multiplier.is_none());
        // Positive bias = overshoot, so the level correction is negative.
        assert!(d.level_bias_correction.is_some());
    }

    #[test]
    fn negative_bias_also_loosens_threshold() {
        // |bias| is what matters, so undershooting forecasts also loosen the
        // seasonality threshold.
        let d = compute_tuning_decision(10.0, -15.0, 10).expect("history present");
        assert_eq!(d.seasonality_threshold_override, Some(0.45));
        // Negative bias = undershoot, so the level correction is positive.
        assert!(d.level_bias_correction.is_some());
    }

    #[test]
    fn bias_at_threshold_does_not_trigger() {
        // |bias| exactly at threshold is not strictly greater.
        let d = compute_tuning_decision(5.0, 10.0, 10).expect("history present");
        assert!(d.seasonality_threshold_override.is_none());
        assert!(d.level_bias_correction.is_none());
    }

    #[test]
    fn low_mape_low_bias_records_mape_only() {
        // Good accuracy and unbiased: still write a row so mape_30d is tracked,
        // but no overrides (ArimaForecaster uses defaults).
        let d = compute_tuning_decision(5.0, 2.0, 50).expect("history present");
        assert!(d.ci_width_multiplier.is_none());
        assert!(d.seasonality_threshold_override.is_none());
        assert!(d.level_bias_correction.is_none());
        assert_eq!(d.mape_30d, Some(5.0));
    }

    #[test]
    fn overrides_clamp_to_safe_ranges() {
        // Even with extreme inputs the overrides stay in their declared bands.
        let d = compute_tuning_decision(1_000_000.0, 1_000_000.0, 100)
            .expect("history present");
        if let Some(ci) = d.ci_width_multiplier {
            assert!((CI_MULTIPLIER_MIN..=CI_MULTIPLIER_MAX).contains(&ci));
        }
        if let Some(th) = d.seasonality_threshold_override {
            assert!((SEASONALITY_THRESHOLD_MIN..=SEASONALITY_THRESHOLD_MAX).contains(&th));
        }
        if let Some(c) = d.level_bias_correction {
            assert!(
                (LEVEL_BIAS_CORRECTION_MIN..=LEVEL_BIAS_CORRECTION_MAX).contains(&c),
                "level_bias_correction out of band: {}",
                c
            );
        }
    }

    #[test]
    fn tenant_discovery_uses_tenants_table_not_users() {
        // Regression guard: per-tenant learning jobs must discover tenants via
        // the canonical tenants.is_active query so coverage stays symmetric
        // with the other per-tenant learning jobs. See issue #399.
        assert!(
            TENANT_DISCOVERY_SQL.contains("FROM tenants"),
            "tenant discovery must select from tenants table"
        );
        assert!(
            TENANT_DISCOVERY_SQL.contains("is_active = true"),
            "tenant discovery must filter on tenants.is_active"
        );
        assert!(
            !TENANT_DISCOVERY_SQL.contains("FROM users"),
            "tenant discovery must not key off users table"
        );
    }
}
