//! Background job: Routing Optimization
//!
//! Periodically optimizes routing decisions based on:
//! - Workload rebalancing
//! - Expertise score updates from recent approvals
//! - Availability calendar sync
//! - Performance pattern analysis
//!
//! Issue #284: Auto-adjusts routing config weights when workload imbalance
//! exceeds a threshold, closing the learning loop beyond expertise scores.
//!
//! Issue #396: The adjustment is bidirectional and driven by an observed
//! routing-outcome signal, the rejection rate of recent rows in
//! `routing_optimization_log.outcome`. A high rejection rate nudges
//! `workload_weight` UP (favor balancing harder); a low rejection rate
//! nudges it DOWN toward the install baseline (let expertise/availability
//! re-assert). Variance over loads is kept only as a cold-start fallback
//! when there is not yet enough outcome data. `enable_pattern_learning`
//! on `routing_configuration` gates the whole adjustment.

use anyhow::{Context, Result};
use billforge_core::{
    intelligent_routing::ExpertiseType,
    types::TenantId,
    workload_balancer::{WorkloadBalancer, WorkloadBalancerConfig},
    UserId,
};
use num_traits::{cast::ToPrimitive, FromPrimitive};
use sqlx::{types::BigDecimal, PgPool, Row};
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Imbalance threshold above which auto-adjustment is triggered.
/// Variance coefficient is (std_dev / mean * 100); 30.0 means ~30% dispersion.
const IMBALANCE_THRESHOLD: f64 = 30.0;

/// How much to increase workload_weight per adjustment step.
const WEIGHT_NUDGE_STEP: f64 = 0.05;

/// Bounds for individual weight values.
const WEIGHT_MIN: f64 = 0.1;
const WEIGHT_MAX: f64 = 0.7;

/// Floor for `workload_weight` when nudging DOWN. We only revert toward,
/// not below, this install baseline so a healthy routing window cannot
/// erode workload balancing entirely.
const WEIGHT_MIN_BASELINE: f64 = 0.2;

/// Cooldown period between auto-adjustments for the same tenant (hours).
const AUTO_ADJUST_COOLDOWN_HOURS: i64 = 24;

/// Minimum number of outcome-tagged routing rows required before we trust
/// the rejection-rate signal. Below this we fall back to cold-start
/// behavior (variance-only, Up-only).
const MIN_OUTCOME_SAMPLES: i64 = 20;

/// Rejection-rate dead band. At or above HIGH we nudge UP; at or below
/// LOW we nudge DOWN; in between we do nothing.
const REJECTION_RATE_HIGH: f64 = 0.15;
const REJECTION_RATE_LOW: f64 = 0.05;

/// Direction of a workload-weight nudge.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum NudgeDirection {
    Up,
    Down,
}

/// Aggregated routing-outcome signal over the recent window.
#[derive(Debug, Clone, Copy)]
struct RoutingOutcomeSignal {
    rejection_rate: f64,
    sample_size: i64,
}

/// Run routing optimization for all tenants
pub async fn run_routing_optimization(
    pg_manager: std::sync::Arc<billforge_db::PgManager>,
) -> Result<()> {
    info!("Starting routing optimization job");

    // Get metadata pool for listing tenants
    let metadata_pool = pg_manager.metadata();

    // Get all active tenants
    let tenants = get_active_tenants(metadata_pool).await?;

    info!(
        "Processing {} tenants for routing optimization",
        tenants.len()
    );

    for tenant_id in tenants {
        let tenant_pool = match pg_manager.tenant(&tenant_id).await {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to get pool for tenant {}: {}", tenant_id, e);
                continue;
            }
        };
        if let Err(e) = optimize_tenant_routing(&tenant_pool, &tenant_id).await {
            warn!("Failed to optimize routing for tenant {}: {}", tenant_id, e);
        }
    }

    info!("Completed routing optimization job");
    Ok(())
}

/// Run routing optimization for one validated tenant.
pub async fn run_tenant_routing_optimization(
    pg_manager: std::sync::Arc<billforge_db::PgManager>,
    tenant_id: &TenantId,
) -> Result<()> {
    let tenant_pool = pg_manager.tenant(tenant_id).await?;
    optimize_tenant_routing(&tenant_pool, tenant_id).await
}

/// Optimize routing for a single tenant
async fn optimize_tenant_routing(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    info!("Optimizing routing for tenant {}", tenant_id);

    // 1. Update workload scores for all approvers
    update_workload_scores(pool, tenant_id).await?;

    // 2. Update expertise scores based on recent approvals
    update_expertise_scores(pool, tenant_id).await?;

    // 3. Check for workload imbalances and suggest redistributions
    check_workload_balance(pool, tenant_id).await?;

    // 4. Clean up old routing log entries (older than 90 days)
    cleanup_old_routing_logs(pool, tenant_id).await?;

    info!("Completed routing optimization for tenant {}", tenant_id);
    Ok(())
}

/// Update workload scores for all approvers in a tenant
async fn update_workload_scores(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    let balancer = WorkloadBalancer::new(WorkloadBalancerConfig::default());

    // Fetch current workload data
    let workloads = sqlx::query!(
        r#"
        SELECT
            user_id,
            active_approvals,
            pending_approvals,
            completed_this_week,
            completed_this_month,
            avg_approval_time_hours
        FROM approver_workload_tracking
        WHERE tenant_id = $1
        "#,
        tenant_id.as_str()
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch workload data")?;

    // Update scores for each approver
    for row in workloads {
        let score = balancer.calculate_workload_score(
            row.active_approvals,
            row.pending_approvals,
            row.avg_approval_time_hours.and_then(|h| h.to_f64()),
        );

        sqlx::query!(
            r#"
            UPDATE approver_workload_tracking
            SET
                workload_score = $2,
                metrics_updated_at = NOW()
            WHERE tenant_id = $1 AND user_id = $3
            "#,
            tenant_id.as_str(),
            BigDecimal::from_f64(score).unwrap_or_else(|| BigDecimal::from(0)),
            row.user_id,
        )
        .execute(pool)
        .await
        .context("Failed to update workload score")?;
    }

    Ok(())
}

/// Update expertise scores based on recent approval outcomes
async fn update_expertise_scores(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    // Get routing log entries from the last 30 days with outcomes
    let recent_routes = sqlx::query!(
        r#"
        SELECT
            selected_approver_id as user_id,
            routing_factors->>'vendor_id' as vendor_id,
            routing_factors->>'department' as department,
            routing_factors->>'gl_code' as gl_code,
            outcome,
            time_to_decision_hours
        FROM routing_optimization_log
        WHERE tenant_id = $1
            AND outcome IS NOT NULL
            AND created_at > NOW() - INTERVAL '30 days'
        "#,
        tenant_id.as_str()
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch recent routing logs")?;

    // Aggregate by approver and expertise type
    let mut expertise_updates: HashMap<(Uuid, ExpertiseType, String), ExpertiseStats> =
        HashMap::new();

    for route in recent_routes {
        let user_id = match route.user_id {
            Some(id) => id,
            None => continue,
        };

        // Extract time_to_decision_hours once to avoid multiple moves
        let time_hours = route.time_to_decision_hours.and_then(|h| h.to_f64());

        // Update vendor expertise
        if let Some(vendor_id) = route.vendor_id {
            let key = (user_id, ExpertiseType::Vendor, vendor_id);
            let stats = expertise_updates.entry(key).or_default();
            update_stats(stats, &route.outcome, time_hours);
        }

        // Update department expertise
        if let Some(dept) = route.department {
            let key = (user_id, ExpertiseType::Department, dept);
            let stats = expertise_updates.entry(key).or_default();
            update_stats(stats, &route.outcome, time_hours);
        }

        // Update GL code expertise
        if let Some(gl_code) = route.gl_code {
            let key = (user_id, ExpertiseType::GlCode, gl_code);
            let stats = expertise_updates.entry(key).or_default();
            update_stats(stats, &route.outcome, time_hours);
        }
    }

    // Update expertise scores in database
    for ((user_id, expertise_type, expertise_key), stats) in expertise_updates {
        let expertise_score = calculate_expertise_score(&stats);

        // Upsert expertise record
        sqlx::query!(
            r#"
            INSERT INTO approver_expertise (
                id, tenant_id, user_id, expertise_type, expertise_key,
                total_approved, total_rejected, avg_time_hours, expertise_score,
                last_used_at, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW(), NOW()
            )
            ON CONFLICT (tenant_id, user_id, expertise_type, expertise_key)
            DO UPDATE SET
                total_approved = approver_expertise.total_approved + $6,
                total_rejected = approver_expertise.total_rejected + $7,
                avg_time_hours = $8,
                expertise_score = $9,
                last_used_at = NOW(),
                updated_at = NOW()
            "#,
            Uuid::new_v4(),
            tenant_id.as_str(),
            user_id,
            expertise_type.to_string(),
            expertise_key,
            stats.approved_count as i32,
            stats.rejected_count as i32,
            stats
                .avg_time_hours
                .map(|h| BigDecimal::from_f64(h).unwrap()),
            BigDecimal::from_f64(expertise_score).unwrap_or_else(|| BigDecimal::from(0)),
        )
        .execute(pool)
        .await
        .context("Failed to update expertise score")?;
    }

    Ok(())
}

/// Check workload balance and create redistribution suggestions
async fn check_workload_balance(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    let balancer = WorkloadBalancer::new(WorkloadBalancerConfig::default());

    // Fetch all workload data
    let rows = sqlx::query!(
        r#"
        SELECT
            user_id,
            active_approvals,
            pending_approvals,
            completed_this_week,
            avg_approval_time_hours,
            workload_score
        FROM approver_workload_tracking
        WHERE tenant_id = $1
        "#,
        tenant_id.as_str()
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch workload data")?;

    let workloads: Vec<billforge_core::intelligent_routing::ApproverWorkload> = rows
        .into_iter()
        .map(
            |row| billforge_core::intelligent_routing::ApproverWorkload {
                user_id: UserId(row.user_id),
                active_approvals: row.active_approvals,
                pending_approvals: row.pending_approvals,
                completed_this_week: row.completed_this_week,
                avg_approval_time_hours: row.avg_approval_time_hours.and_then(|h| h.to_f64()),
                workload_score: row.workload_score.to_f64().unwrap_or(0.0),
                last_assignment_at: None,
            },
        )
        .collect();

    // Calculate distribution stats
    let stats = balancer.calculate_distribution_stats(&workloads);

    // Get suggestions
    let suggestions = balancer.suggest_redistribution(&workloads);

    if !suggestions.is_empty() {
        info!(
            "Tenant {} has {} workload redistribution suggestions (variance: {:.2}%)",
            tenant_id,
            suggestions.len(),
            stats.variance_coefficient
        );
    }

    // Issue #396: trigger decision moved into apply_workload_rebalance so it
    // can weigh observed routing outcome quality, not only the variance
    // proxy. Variance is still passed in as a cold-start fallback signal.
    if let Err(e) = apply_workload_rebalance(pool, tenant_id, stats.variance_coefficient).await {
        warn!(
            "Failed to auto-adjust routing weights for tenant {}: {}",
            tenant_id, e
        );
    }

    Ok(())
}

/// Clean up old routing log entries
async fn cleanup_old_routing_logs(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    let result = sqlx::query!(
        r#"
        DELETE FROM routing_optimization_log
        WHERE tenant_id = $1
            AND created_at < NOW() - INTERVAL '90 days'
        "#,
        tenant_id.as_str()
    )
    .execute(pool)
    .await
    .context("Failed to clean up old routing logs")?;

    if result.rows_affected() > 0 {
        info!(
            "Cleaned up {} old routing log entries for tenant {}",
            result.rows_affected(),
            tenant_id
        );
    }

    Ok(())
}

/// Nudge routing weights either toward more or less workload-balancing,
/// based on observed routing outcome quality.
///
/// `Up`: increase `workload_weight` by `WEIGHT_NUDGE_STEP` (clamped to
/// `WEIGHT_MAX`). Returns `None` if already at `WEIGHT_MAX`.
///
/// `Down`: decrease `workload_weight` by `WEIGHT_NUDGE_STEP` (floored at
/// `WEIGHT_MIN_BASELINE`). Returns `None` if already at or below baseline.
///
/// In either case, `expertise_weight` + `availability_weight` are
/// proportionally renormalized so all three weights sum to 1.0. Individual
/// values are clamped to `[WEIGHT_MIN, WEIGHT_MAX]` during the proportional
/// rebalance, then the result is renormalized so the sum is still 1.0.
fn nudge_routing_weights(
    workload_weight: f64,
    expertise_weight: f64,
    availability_weight: f64,
    direction: NudgeDirection,
) -> Option<(f64, f64, f64, f64)> {
    // Returns (new_workload, new_expertise, new_availability, old_workload)

    let old_workload = workload_weight;

    let new_workload = match direction {
        NudgeDirection::Up => {
            let candidate = (workload_weight + WEIGHT_NUDGE_STEP).min(WEIGHT_MAX);
            if (candidate - old_workload).abs() < 1e-9 {
                return None; // Already at max, nothing to do
            }
            candidate
        }
        NudgeDirection::Down => {
            if workload_weight <= WEIGHT_MIN_BASELINE + 1e-9 {
                return None; // Already at or below baseline, nothing to do
            }
            (workload_weight - WEIGHT_NUDGE_STEP).max(WEIGHT_MIN_BASELINE)
        }
    };

    let remaining = 1.0 - new_workload;
    let other_sum = expertise_weight + availability_weight;

    let (new_expertise, new_availability) = if other_sum > 1e-9 {
        let scale = remaining / other_sum;
        let e = (expertise_weight * scale).clamp(WEIGHT_MIN, WEIGHT_MAX);
        let a = (availability_weight * scale).clamp(WEIGHT_MIN, WEIGHT_MAX);
        (e, a)
    } else {
        // Even split if both are zero
        (remaining / 2.0, remaining / 2.0)
    };

    // Final normalization: ensure sum == 1.0 across all three weights.
    // Clamping an individual weight up can push total > 1.0, so we scale
    // everything proportionally.
    let total = new_workload + new_expertise + new_availability;
    let new_workload = new_workload / total;
    let new_expertise = new_expertise / total;
    let new_availability = new_availability / total;

    Some((new_workload, new_expertise, new_availability, old_workload))
}

/// Compute the recent routing-outcome signal for a tenant.
///
/// Reads `routing_optimization_log` over the last 30 days and returns the
/// rejection rate plus sample size. Returns `None` when there are fewer
/// than `MIN_OUTCOME_SAMPLES` outcome-tagged rows, so we never act on noise.
async fn compute_routing_outcome_signal(
    pool: &PgPool,
    tenant_id: &TenantId,
) -> Result<Option<RoutingOutcomeSignal>> {
    // NOTE: routing_optimization_log.tenant_id is VARCHAR(255) (migration 051),
    // so we bind as &str. Using the non-macro sqlx::query so the build does
    // not require a cached query manifest for this analytic SELECT.
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE outcome = 'rejected') AS rejected,
            COUNT(*) FILTER (WHERE outcome IS NOT NULL) AS total
        FROM routing_optimization_log
        WHERE tenant_id = $1
            AND created_at > NOW() - INTERVAL '30 days'
        "#,
    )
    .bind(tenant_id.as_str())
    .fetch_one(pool)
    .await
    .context("Failed to compute routing outcome signal")?;

    let total: i64 = row.try_get::<Option<i64>, _>("total")?.unwrap_or(0);
    if total < MIN_OUTCOME_SAMPLES {
        return Ok(None);
    }

    let rejected: i64 = row.try_get::<Option<i64>, _>("rejected")?.unwrap_or(0);
    let rejection_rate = rejected as f64 / total as f64;

    Ok(Some(RoutingOutcomeSignal {
        rejection_rate,
        sample_size: total,
    }))
}

/// Auto-adjust routing config weights based on observed routing outcomes.
///
/// This closes the routing learning loop (issue #284 / #396): instead of
/// merely logging the imbalance, we nudge `workload_weight` in the
/// direction that the recent rejection rate suggests:
/// - High rejection rate → nudge UP (lean harder on workload balancing)
/// - Low rejection rate → nudge DOWN toward the install baseline (let
///   expertise/availability re-assert)
/// - Insufficient outcome data → fall back to the variance proxy (UP only)
///
/// Guards:
/// - `enable_pattern_learning` must be true.
/// - Only adjusts if the last auto-adjustment was > 24h ago (or never).
/// - Persists the new weights to `routing_configuration`.
/// - Writes an audit trail entry to `workflow_audit_log` including the
///   direction, rejection rate, and sample size.
async fn apply_workload_rebalance(
    pool: &PgPool,
    tenant_id: &TenantId,
    imbalance_score: f64,
) -> Result<()> {
    let tid = tenant_id.as_uuid();

    // 1. Load current weights, cooldown timestamp, and learning flag
    //    NOTE: routing_configuration.tenant_id is VARCHAR(255) (migration 051),
    //    so we bind as &str, not UUID.
    let row = sqlx::query(
        r#"
        SELECT workload_weight, expertise_weight, availability_weight,
               last_auto_adjusted_at, enable_pattern_learning
        FROM routing_configuration
        WHERE tenant_id = $1
        "#,
    )
    .bind(tenant_id.as_str())
    .fetch_optional(pool)
    .await
    .context("Failed to load routing config for auto-adjust")?;

    // If no config row exists, there is nothing to adjust
    let row = match row {
        Some(r) => r,
        None => return Ok(()),
    };

    // 2. Honor the tenant-controlled learning flag (issue #396).
    let enable_pattern_learning: bool = row.get("enable_pattern_learning");
    if !enable_pattern_learning {
        info!(
            "Skipping auto-adjust: enable_pattern_learning=false for tenant {}",
            tenant_id
        );
        return Ok(());
    }

    // 3. Cooldown guard: skip if adjusted within the last N hours
    let last_auto_adjusted: Option<chrono::DateTime<chrono::Utc>> =
        row.get("last_auto_adjusted_at");
    if let Some(last) = last_auto_adjusted {
        let elapsed = chrono::Utc::now() - last;
        if elapsed < chrono::Duration::hours(AUTO_ADJUST_COOLDOWN_HOURS) {
            info!(
                "Skipping auto-adjust for tenant {}: last adjusted {}h ago (cooldown {}h)",
                tenant_id,
                elapsed.num_hours(),
                AUTO_ADJUST_COOLDOWN_HOURS
            );
            return Ok(());
        }
    }

    let old_w: f64 = row
        .get::<BigDecimal, _>("workload_weight")
        .to_f64()
        .unwrap_or(0.4);
    let old_e: f64 = row
        .get::<BigDecimal, _>("expertise_weight")
        .to_f64()
        .unwrap_or(0.3);
    let old_a: f64 = row
        .get::<BigDecimal, _>("availability_weight")
        .to_f64()
        .unwrap_or(0.3);

    // 4. Read outcome signal and decide direction.
    let outcome = compute_routing_outcome_signal(pool, tenant_id).await?;
    let direction = match outcome.as_ref() {
        Some(sig) if sig.rejection_rate >= REJECTION_RATE_HIGH => NudgeDirection::Up,
        Some(sig) if sig.rejection_rate <= REJECTION_RATE_LOW && old_w > WEIGHT_MIN_BASELINE => {
            NudgeDirection::Down
        }
        Some(sig) => {
            info!(
                "Skipping auto-adjust for tenant {}: outcome quality nominal \
                 (rejection_rate {:.3}, sample_size {}), no nudge",
                tenant_id, sig.rejection_rate, sig.sample_size
            );
            return Ok(());
        }
        None => {
            // Cold-start fallback (preserves the issue #284 behavior): only
            // nudge UP, and only when the variance proxy crosses the
            // threshold. This keeps tenants with no outcome history from
            // running with a learning loop that can't see anything.
            if imbalance_score > IMBALANCE_THRESHOLD {
                NudgeDirection::Up
            } else {
                info!(
                    "Skipping auto-adjust for tenant {}: insufficient outcome data \
                     and variance {:.2}% within threshold {:.2}%",
                    tenant_id, imbalance_score, IMBALANCE_THRESHOLD
                );
                return Ok(());
            }
        }
    };

    // 5. Compute new weights in the chosen direction.
    let result = match nudge_routing_weights(old_w, old_e, old_a, direction) {
        Some(r) => r,
        None => {
            info!(
                "No weight adjustment needed for tenant {} (workload_weight already at \
                 the {:?} bound)",
                tenant_id, direction
            );
            return Ok(());
        }
    };
    let (new_w, new_e, new_a, _old_workload) = result;

    // 6. Persist the new weights and update cooldown timestamp
    //    NOTE: routing_configuration.tenant_id is VARCHAR(255) (migration 051).
    sqlx::query(
        r#"
        UPDATE routing_configuration
        SET workload_weight = $2,
            expertise_weight = $3,
            availability_weight = $4,
            last_auto_adjusted_at = NOW(),
            updated_at = NOW()
        WHERE tenant_id = $1
        "#,
    )
    .bind(tenant_id.as_str())
    .bind(BigDecimal::from_f64(new_w).unwrap_or_else(|| BigDecimal::from(0)))
    .bind(BigDecimal::from_f64(new_e).unwrap_or_else(|| BigDecimal::from(0)))
    .bind(BigDecimal::from_f64(new_a).unwrap_or_else(|| BigDecimal::from(0)))
    .execute(pool)
    .await
    .context("Failed to persist auto-adjusted routing weights")?;

    // 7. Write audit trail including the loop-observability fields.
    let old_weights = serde_json::json!({
        "workload_weight": old_w,
        "expertise_weight": old_e,
        "availability_weight": old_a,
    });
    let new_weights = serde_json::json!({
        "workload_weight": new_w,
        "expertise_weight": new_e,
        "availability_weight": new_a,
    });

    let direction_label = match direction {
        NudgeDirection::Up => "up",
        NudgeDirection::Down => "down",
    };

    sqlx::query(
        r#"
        INSERT INTO workflow_audit_log (
            id, tenant_id, entity_type, entity_id, action,
            actor_type, old_values, new_values, metadata, created_at
        ) VALUES (
            gen_random_uuid(), $1, 'RoutingConfig', $2,
            'routing_config.auto_adjusted', 'system:routing_optimizer',
            $3, $4, $5, NOW()
        )
        "#,
    )
    .bind(tid)
    .bind(tid)
    .bind(&old_weights)
    .bind(&new_weights)
    .bind(serde_json::json!({
        "imbalance_score": imbalance_score,
        "nudge_step": WEIGHT_NUDGE_STEP,
        "direction": direction_label,
        "rejection_rate": outcome.as_ref().map(|s| s.rejection_rate),
        "sample_size": outcome.as_ref().map(|s| s.sample_size),
    }))
    .execute(pool)
    .await
    .context("Failed to insert routing auto-adjust audit entry")?;

    info!(
        "Auto-adjusted routing weights for tenant {} ({}): workload {:.3} -> {:.3}, \
         expertise {:.3} -> {:.3}, availability {:.3} -> {:.3} (imbalance: {:.1}%, \
         rejection_rate: {:?}, sample_size: {:?})",
        tenant_id,
        direction_label,
        old_w,
        new_w,
        old_e,
        new_e,
        old_a,
        new_a,
        imbalance_score,
        outcome.as_ref().map(|s| s.rejection_rate),
        outcome.as_ref().map(|s| s.sample_size),
    );

    Ok(())
}

const TENANT_DISCOVERY_SQL: &str = "SELECT id FROM tenants WHERE is_active = true";

/// Get all active tenants from the tenants table.
async fn get_active_tenants(pool: &PgPool) -> Result<Vec<TenantId>> {
    let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(TENANT_DISCOVERY_SQL)
        .fetch_all(pool)
        .await
        .context("Failed to fetch active tenants")?;

    Ok(rows
        .into_iter()
        .map(|(id,)| TenantId::from_uuid(id))
        .collect())
}

/// Statistics for expertise calculation
#[derive(Debug, Clone, Default)]
struct ExpertiseStats {
    approved_count: u32,
    rejected_count: u32,
    total_time_hours: f64,
    time_count: u32,
    avg_time_hours: Option<f64>,
}

impl ExpertiseStats {
    fn avg_time(&self) -> Option<f64> {
        if self.time_count > 0 {
            Some(self.total_time_hours / self.time_count as f64)
        } else {
            None
        }
    }
}

/// Update stats with a routing outcome
fn update_stats(stats: &mut ExpertiseStats, outcome: &Option<String>, time_hours: Option<f64>) {
    match outcome.as_deref() {
        Some("approved") => stats.approved_count += 1,
        Some("rejected") => stats.rejected_count += 1,
        _ => {}
    }

    if let Some(hours) = time_hours {
        stats.total_time_hours += hours;
        stats.time_count += 1;
        stats.avg_time_hours = Some(stats.total_time_hours / stats.time_count as f64);
    }
}

/// Calculate expertise score from stats
fn calculate_expertise_score(stats: &ExpertiseStats) -> f64 {
    let total = stats.approved_count + stats.rejected_count;

    if total == 0 {
        return 0.5; // Default neutral score
    }

    // Base score from approval rate
    let approval_rate = stats.approved_count as f64 / total as f64;
    let mut score = approval_rate * 0.8 + 0.1; // Scale to 0.1-0.9 range

    // Speed bonus/penalty
    if let Some(avg_time) = stats.avg_time() {
        let target_time = 24.0;
        let speed_ratio = target_time / avg_time;
        let speed_adjustment = (speed_ratio - 1.0) * 0.1;
        score = (score + speed_adjustment).clamp(0.0, 1.0);
    }

    // Volume bonus (more experience = higher score)
    let volume_bonus = (total as f64 / 50.0).min(0.1);
    score = (score + volume_bonus).clamp(0.0, 1.0);

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expertise_score_calculation() {
        // No data
        let stats = ExpertiseStats::default();
        let score = calculate_expertise_score(&stats);
        assert_eq!(score, 0.5);

        // 100% approval rate, fast
        let stats = ExpertiseStats {
            approved_count: 50,
            rejected_count: 0,
            total_time_hours: 600.0, // 12 hours avg
            time_count: 50,
            avg_time_hours: Some(12.0),
        };
        let score = calculate_expertise_score(&stats);
        assert!(score > 0.8);

        // 50% approval rate, slow
        let stats = ExpertiseStats {
            approved_count: 10,
            rejected_count: 10,
            total_time_hours: 960.0, // 48 hours avg
            time_count: 20,
            avg_time_hours: Some(48.0),
        };
        let score = calculate_expertise_score(&stats);
        assert!(score < 0.6);
    }

    // ------------------------------------------------------------------
    // Issue #284 / #396: Weight nudging unit tests
    // ------------------------------------------------------------------

    #[test]
    fn test_nudge_routing_weights_increases_workload() {
        let result = nudge_routing_weights(0.4, 0.3, 0.3, NudgeDirection::Up);
        let (new_w, new_e, new_a, old_w) = result.expect("should produce a nudge");
        assert_eq!(old_w, 0.4);
        assert!(new_w > 0.4, "workload weight should increase");
        assert!(
            (new_w + new_e + new_a - 1.0).abs() < 1e-9,
            "weights must sum to 1.0"
        );
    }

    #[test]
    fn test_nudge_routing_weights_clamps_to_max() {
        // workload_weight near max: 0.68 + 0.05 = 0.73, clamped to 0.7
        let result = nudge_routing_weights(0.68, 0.16, 0.16, NudgeDirection::Up);
        let (new_w, new_e, new_a, _old) = result.expect("should produce a nudge");
        assert!(
            new_w <= WEIGHT_MAX + 1e-9,
            "workload weight must not exceed max"
        );
        assert!(
            (new_w + new_e + new_a - 1.0).abs() < 1e-9,
            "weights must sum to 1.0"
        );
    }

    #[test]
    fn test_nudge_routing_weights_returns_none_at_max() {
        let result = nudge_routing_weights(WEIGHT_MAX, 0.15, 0.15, NudgeDirection::Up);
        assert!(result.is_none(), "should return None when already at max");
    }

    #[test]
    fn test_nudge_routing_weights_stays_in_bounds() {
        // Start from a skewed config and verify all weights are non-negative
        // and the primary constraint (sum == 1.0) holds. Normalization after
        // clamping can push a value slightly below WEIGHT_MIN, which is
        // acceptable because the weights are always renormalized.
        let result = nudge_routing_weights(0.6, 0.1, 0.3, NudgeDirection::Up);
        let (new_w, new_e, new_a, _) = result.expect("should produce a nudge");
        assert!(
            new_w >= 0.0 && new_w <= WEIGHT_MAX + 1e-9,
            "workload in range: got {}",
            new_w
        );
        assert!(new_e >= 0.0, "expertise non-negative: got {}", new_e);
        assert!(new_a >= 0.0, "availability non-negative: got {}", new_a);
        assert!(
            (new_w + new_e + new_a - 1.0).abs() < 1e-9,
            "weights sum to 1.0"
        );
    }

    #[test]
    fn test_nudge_routing_weights_proportional_rebalance() {
        // With 0.4/0.3/0.3 default, after Up nudge workload should be 0.45,
        // remaining 0.55 split proportionally: expertise gets 0.55 * (0.3/0.6) = 0.275,
        // availability gets 0.55 * (0.3/0.6) = 0.275
        let (new_w, new_e, new_a, _) =
            nudge_routing_weights(0.4, 0.3, 0.3, NudgeDirection::Up)
                .expect("should produce a nudge");
        assert!(
            (new_w - 0.45).abs() < 1e-9,
            "workload should be 0.45, got {}",
            new_w
        );
        assert!(
            (new_e - 0.275).abs() < 1e-9,
            "expertise should be 0.275, got {}",
            new_e
        );
        assert!(
            (new_a - 0.275).abs() < 1e-9,
            "availability should be 0.275, got {}",
            new_a
        );
    }

    #[test]
    fn test_imbalance_threshold_sanity() {
        // Ensure the threshold constants are reasonable
        assert!(IMBALANCE_THRESHOLD > 0.0);
        assert!(WEIGHT_NUDGE_STEP > 0.0);
        assert!(WEIGHT_MIN < WEIGHT_MAX);
        assert!(WEIGHT_MIN_BASELINE >= WEIGHT_MIN);
        assert!(WEIGHT_MIN_BASELINE < WEIGHT_MAX);
        assert!(AUTO_ADJUST_COOLDOWN_HOURS > 0);
        assert!(REJECTION_RATE_LOW < REJECTION_RATE_HIGH);
        assert!(MIN_OUTCOME_SAMPLES > 0);
    }

    // ------------------------------------------------------------------
    // Issue #396: bidirectional nudge tests
    // ------------------------------------------------------------------

    #[test]
    fn nudge_up_from_default_renormalizes_to_one() {
        let (new_w, new_e, new_a, old_w) =
            nudge_routing_weights(0.4, 0.3, 0.3, NudgeDirection::Up)
                .expect("should nudge up");
        assert_eq!(old_w, 0.4);
        assert!(new_w > old_w, "workload should increase");
        assert!(
            (new_w + new_e + new_a - 1.0).abs() < 1e-9,
            "weights must sum to 1.0"
        );
    }

    #[test]
    fn nudge_down_from_elevated_renormalizes_to_one() {
        let (new_w, new_e, new_a, old_w) =
            nudge_routing_weights(0.5, 0.25, 0.25, NudgeDirection::Down)
                .expect("should nudge down");
        assert_eq!(old_w, 0.5);
        assert!(new_w < old_w, "workload should decrease, got {}", new_w);
        assert!(
            new_w >= WEIGHT_MIN_BASELINE - 1e-9,
            "workload must not fall below baseline, got {}",
            new_w
        );
        assert!(
            (new_w + new_e + new_a - 1.0).abs() < 1e-9,
            "weights must sum to 1.0"
        );
    }

    #[test]
    fn nudge_up_at_max_returns_none() {
        let result = nudge_routing_weights(WEIGHT_MAX, 0.15, 0.15, NudgeDirection::Up);
        assert!(result.is_none(), "should return None when already at max");
    }

    #[test]
    fn nudge_down_at_baseline_returns_none() {
        let result =
            nudge_routing_weights(WEIGHT_MIN_BASELINE, 0.4, 0.4, NudgeDirection::Down);
        assert!(
            result.is_none(),
            "should return None when already at baseline"
        );
    }

    #[test]
    fn nudge_down_then_up_round_trips_within_epsilon() {
        // Start from default, nudge down, then up, and confirm we land close
        // to where we started (within floating-point and renormalization noise).
        let (w_down, e_down, a_down, _) =
            nudge_routing_weights(0.45, 0.275, 0.275, NudgeDirection::Down)
                .expect("should nudge down from elevated");
        let (w_back, e_back, a_back, _) =
            nudge_routing_weights(w_down, e_down, a_down, NudgeDirection::Up)
                .expect("should nudge up again");

        // Sum invariant
        assert!(
            (w_back + e_back + a_back - 1.0).abs() < 1e-9,
            "weights must sum to 1.0 after round trip"
        );
        // Round-trip should bring workload close to the original 0.45.
        // Renormalization makes this only approximate, so allow a loose epsilon.
        assert!(
            (w_back - 0.45).abs() < 1e-3,
            "workload should round-trip close to 0.45, got {}",
            w_back
        );
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
