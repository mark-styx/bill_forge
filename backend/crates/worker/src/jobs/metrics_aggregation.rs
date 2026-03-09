//! Metrics aggregation job
//!
//! Aggregates dashboard metrics and caches them in Redis for faster dashboard loads.
//! This reduces database load by computing metrics on a schedule rather than on every request.

use crate::config::WorkerConfig;
use anyhow::{Context, Result};
use billforge_db::repositories::MetricsRepositoryImpl;
use billforge_core::types::TenantId;
use redis::AsyncCommands;
use tracing::{info, warn};

pub async fn aggregate_metrics(tenant_id: &str, config: &WorkerConfig) -> Result<()> {
    info!("Aggregating metrics for tenant: {}", tenant_id);

    // Parse tenant ID
    let tenant_id = tenant_id.parse::<TenantId>()
        .context("Invalid tenant_id format")?;

    // Get tenant-specific database connection
    let pool = config.pg_manager.tenant(&tenant_id).await
        .context("Failed to get tenant database")?;

    let metrics_repo = MetricsRepositoryImpl::new(pool);

    // Connect to Redis for caching
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let mut redis_conn = redis_client.get_async_connection().await?;

    // 1. Query and cache invoice metrics
    match metrics_repo.get_invoice_metrics(&tenant_id).await {
        Ok(metrics) => {
            let cache_key = format!("billforge:metrics:{}:invoices", tenant_id.as_str());
            let metrics_json = serde_json::to_string(&metrics)?;

            // Cache for 5 minutes (300 seconds)
            let _: () = redis_conn
                .set_ex(&cache_key, metrics_json, 300)
                .await
                .context("Failed to cache invoice metrics")?;

            info!(
                tenant_id = %tenant_id.as_str(),
                total = metrics.total_invoices,
                pending_ocr = metrics.pending_ocr,
                "Invoice metrics aggregated and cached"
            );
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant_id.as_str(),
                error = %e,
                "Failed to aggregate invoice metrics"
            );
        }
    }

    // 2. Query and cache approval metrics
    match metrics_repo.get_approval_metrics(&tenant_id).await {
        Ok(metrics) => {
            let cache_key = format!("billforge:metrics:{}:approvals", tenant_id.as_str());
            let metrics_json = serde_json::to_string(&metrics)?;

            let _: () = redis_conn
                .set_ex(&cache_key, metrics_json, 300)
                .await
                .context("Failed to cache approval metrics")?;

            info!(
                tenant_id = %tenant_id.as_str(),
                pending = metrics.pending_approvals,
                avg_time_hours = metrics.avg_approval_time_hours,
                "Approval metrics aggregated and cached"
            );
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant_id.as_str(),
                error = %e,
                "Failed to aggregate approval metrics"
            );
        }
    }

    // 3. Query and cache vendor metrics
    match metrics_repo.get_vendor_metrics(&tenant_id).await {
        Ok(metrics) => {
            let cache_key = format!("billforge:metrics:{}:vendors", tenant_id.as_str());
            let metrics_json = serde_json::to_string(&metrics)?;

            let _: () = redis_conn
                .set_ex(&cache_key, metrics_json, 300)
                .await
                .context("Failed to cache vendor metrics")?;

            info!(
                tenant_id = %tenant_id.as_str(),
                total_vendors = metrics.total_vendors,
                new_this_month = metrics.new_this_month,
                "Vendor metrics aggregated and cached"
            );
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant_id.as_str(),
                error = %e,
                "Failed to aggregate vendor metrics"
            );
        }
    }

    // 4. Query and cache team metrics
    match metrics_repo.get_team_metrics(&tenant_id).await {
        Ok(metrics) => {
            let cache_key = format!("billforge:metrics:{}:team", tenant_id.as_str());
            let metrics_json = serde_json::to_string(&metrics)?;

            let _: () = redis_conn
                .set_ex(&cache_key, metrics_json, 300)
                .await
                .context("Failed to cache team metrics")?;

            info!(
                tenant_id = %tenant_id.as_str(),
                member_count = metrics.members.len(),
                total_pending = metrics.total_pending_actions,
                "Team metrics aggregated and cached"
            );
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant_id.as_str(),
                error = %e,
                "Failed to aggregate team metrics"
            );
        }
    }

    // 5. Update dashboard summary cache
    let dashboard_cache_key = format!("billforge:metrics:{}:dashboard_updated", tenant_id.as_str());
    let timestamp = chrono::Utc::now().to_rfc3339();
    let _: () = redis_conn
        .set_ex(&dashboard_cache_key, timestamp, 300)
        .await
        .context("Failed to update dashboard cache timestamp")?;

    info!("Metrics aggregation completed for tenant: {}", tenant_id.as_str());

    Ok(())
}
