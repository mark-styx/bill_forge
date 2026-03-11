//! Background job: Routing Optimization
//!
//! Periodically optimizes routing decisions based on:
//! - Workload rebalancing
//! - Expertise score updates from recent approvals
//! - Availability calendar sync
//! - Performance pattern analysis

use anyhow::{Context, Result};
use billforge_core::{
    intelligent_routing::ExpertiseType,
    workload_balancer::{WorkloadBalancer, WorkloadBalancerConfig},
    types::TenantId,
    UserId,
};
use num_traits::{cast::ToPrimitive, FromPrimitive};
use sqlx::{PgPool, types::BigDecimal};
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Run routing optimization for all tenants
pub async fn run_routing_optimization(pg_manager: std::sync::Arc<billforge_db::PgManager>) -> Result<()> {
    info!("Starting routing optimization job");

    // Get connection pool
    let pool = pg_manager.metadata();

    // Get all active tenants
    let tenants = get_active_tenants(&pool).await?;

    info!("Processing {} tenants for routing optimization", tenants.len());

    for tenant_id in tenants {
        if let Err(e) = optimize_tenant_routing(&pool, &tenant_id).await {
            warn!(
                "Failed to optimize routing for tenant {}: {}",
                tenant_id, e
            );
        }
    }

    info!("Completed routing optimization job");
    Ok(())
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
            stats.avg_time_hours.map(|h| BigDecimal::from_f64(h).unwrap()),
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
        .map(|row| billforge_core::intelligent_routing::ApproverWorkload {
            user_id: UserId(row.user_id),
            active_approvals: row.active_approvals,
            pending_approvals: row.pending_approvals,
            completed_this_week: row.completed_this_week,
            avg_approval_time_hours: row.avg_approval_time_hours.and_then(|h| h.to_f64()),
            workload_score: row.workload_score.to_f64().unwrap_or(0.0),
            last_assignment_at: None,
        })
        .collect();

    // Calculate distribution stats
    let stats = balancer.calculate_distribution_stats(&workloads);

    // Get suggestions
    let suggestions = balancer.suggest_redistribution(&workloads);

    // Store stats and suggestions in database (could be used for alerts/dashboards)
    if !suggestions.is_empty() {
        info!(
            "Tenant {} has {} workload redistribution suggestions (variance: {:.2}%)",
            tenant_id,
            suggestions.len(),
            stats.variance_coefficient
        );

        // Could send notifications to admins here
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

/// Get all active tenants
async fn get_active_tenants(pool: &PgPool) -> Result<Vec<TenantId>> {
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT tenant_id
        FROM users
        WHERE is_active = true
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch active tenants")?;

    Ok(rows
        .into_iter()
        .map(|row| TenantId::from_uuid(row.tenant_id))
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
}
