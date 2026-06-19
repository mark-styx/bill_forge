//! Job scheduler for recurring background tasks
//!
//! Enqueues jobs on a cron-like schedule.

use anyhow::{Context, Result};
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::config::WorkerConfig;
use crate::jobs::{Job, JobType};

/// Start the job scheduler
pub async fn start_scheduler(config: WorkerConfig) -> Result<()> {
    info!("Starting job scheduler");

    let redis_client = redis::Client::open(config.redis_url.as_str())?;

    // Spawn scheduler tasks with their own connections
    let redis_client1 = redis_client.clone();
    let pg_manager = config.pg_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = schedule_embeddings_refresh_task(redis_client1, pg_manager).await {
            error!("Embedding refresh scheduler failed: {}", e);
        }
    });

    let redis_client2 = redis_client.clone();
    let pg_manager2 = config.pg_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = schedule_categorization_training_task(redis_client2, pg_manager2).await {
            error!("Categorization training scheduler failed: {}", e);
        }
    });

    let redis_client3 = redis_client.clone();
    let pg_manager3 = config.pg_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = schedule_forecast_refresh_task(redis_client3, pg_manager3).await {
            error!("Forecast refresh scheduler failed: {}", e);
        }
    });

    let redis_client4 = redis_client.clone();
    let pg_manager4 = config.pg_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = schedule_anomaly_detection_task(redis_client4, pg_manager4).await {
            error!("Anomaly detection scheduler failed: {}", e);
        }
    });

    let redis_client5 = redis_client.clone();
    let pg_manager5 = config.pg_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = schedule_routing_optimization_task(redis_client5, pg_manager5).await {
            error!("Routing optimization scheduler failed: {}", e);
        }
    });

    let redis_client6 = redis_client.clone();
    let pg_manager6 = config.pg_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = schedule_vendor_risk_rescan_task(redis_client6, pg_manager6).await {
            error!("Vendor risk rescan scheduler failed: {}", e);
        }
    });

    info!("Scheduler started");

    // Keep the scheduler running
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

/// Schedule embedding refresh jobs weekly
async fn schedule_embeddings_refresh_task(
    redis_client: redis::Client,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    // Run every 7 days (weekly)
    let mut interval = interval(Duration::from_secs(7 * 24 * 60 * 60));

    info!("Embedding refresh scheduler started (weekly)");

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_embedding_refresh_job(&mut conn, pg_manager.clone()).await {
            Ok(_) => info!("Enqueued weekly embedding refresh job"),
            Err(e) => error!("Failed to enqueue embedding refresh job: {}", e),
        }
    }
}

/// Schedule categorization training jobs daily
async fn schedule_categorization_training_task(
    redis_client: redis::Client,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    // Run every 24 hours (daily)
    let mut interval = interval(Duration::from_secs(24 * 60 * 60));

    info!("Categorization training scheduler started (daily)");

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_training_jobs(&mut conn, pg_manager.clone()).await {
            Ok(_) => info!("Enqueued daily categorization training job"),
            Err(e) => error!("Failed to enqueue categorization training job: {}", e),
        }
    }
}

/// Enqueue an embedding refresh job
async fn enqueue_embedding_refresh_job(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    enqueue_jobs_for_active_tenants(conn, pg_manager, JobType::EmbeddingRefresh).await
}

/// Enqueue a categorization training job
async fn enqueue_training_jobs(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    enqueue_jobs_for_active_tenants(conn, pg_manager, JobType::CategorizationTraining).await
}

/// Schedule forecast refresh jobs weekly
async fn schedule_forecast_refresh_task(
    redis_client: redis::Client,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    // Run every 7 days (weekly)
    let mut interval = interval(Duration::from_secs(7 * 24 * 60 * 60));

    info!("Forecast refresh scheduler started (weekly)");

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_forecast_refresh_jobs(&mut conn, pg_manager.clone()).await {
            Ok(_) => info!("Enqueued weekly forecast refresh job"),
            Err(e) => error!("Failed to enqueue forecast refresh job: {}", e),
        }
    }
}

/// Schedule anomaly detection jobs daily
async fn schedule_anomaly_detection_task(
    redis_client: redis::Client,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    // Run every 24 hours (daily)
    let mut interval = interval(Duration::from_secs(24 * 60 * 60));

    info!("Anomaly detection scheduler started (daily)");

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_anomaly_detection_jobs(&mut conn, pg_manager.clone()).await {
            Ok(_) => info!("Enqueued daily anomaly detection job"),
            Err(e) => error!("Failed to enqueue anomaly detection job: {}", e),
        }
    }
}

/// Enqueue a forecast refresh job
async fn enqueue_forecast_refresh_jobs(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    enqueue_jobs_for_active_tenants(conn, pg_manager, JobType::ForecastRefresh).await
}

/// Enqueue an anomaly detection job
async fn enqueue_anomaly_detection_jobs(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    enqueue_jobs_for_active_tenants(conn, pg_manager, JobType::AnomalyDetection).await
}

/// Schedule routing optimization jobs every 6 hours
async fn schedule_routing_optimization_task(
    redis_client: redis::Client,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    // Run every 6 hours
    let mut interval = interval(Duration::from_secs(6 * 60 * 60));

    info!("Routing optimization scheduler started (every 6 hours)");

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_routing_optimization_jobs(&mut conn, pg_manager.clone()).await {
            Ok(_) => info!("Enqueued routing optimization job"),
            Err(e) => error!("Failed to enqueue routing optimization job: {}", e),
        }
    }
}

/// Enqueue a routing optimization job
async fn enqueue_routing_optimization_jobs(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    enqueue_jobs_for_active_tenants(conn, pg_manager, JobType::RoutingOptimization).await
}

/// Schedule vendor risk rescan jobs daily (configurable via VENDOR_RISK_RESCAN_CRON_SECS).
///
/// VENDOR_RISK_RESCAN_CRON_SECS overrides the default 24h interval so an
/// operator can run a one-off rescan or tighten the cadence without redeploying.
async fn schedule_vendor_risk_rescan_task(
    redis_client: redis::Client,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    let interval_secs = std::env::var("VENDOR_RISK_RESCAN_CRON_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(24 * 60 * 60);

    let mut interval = interval(Duration::from_secs(interval_secs));

    info!(
        interval_secs,
        "Vendor risk rescan scheduler started (default daily, override via VENDOR_RISK_RESCAN_CRON_SECS)"
    );

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_vendor_risk_rescan_jobs(&mut conn, pg_manager.clone()).await {
            Ok(_) => info!("Enqueued vendor risk rescan job"),
            Err(e) => error!("Failed to enqueue vendor risk rescan job: {}", e),
        }
    }
}

/// Enqueue a vendor risk rescan job per active tenant.
async fn enqueue_vendor_risk_rescan_jobs(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
) -> Result<()> {
    enqueue_jobs_for_active_tenants(conn, pg_manager, JobType::VendorRiskRescan).await
}

async fn enqueue_jobs_for_active_tenants(
    conn: &mut redis::aio::Connection,
    pg_manager: Arc<billforge_db::PgManager>,
    job_type: JobType,
) -> Result<()> {
    let tenants = fetch_active_tenant_ids(pg_manager).await?;

    for job in build_tenant_jobs(&tenants, job_type.clone()) {
        let tenant_id = job.tenant_id.clone();
        let job_json = serde_json::to_string(&job)?;
        conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
            .await
            .context("Failed to enqueue tenant-scoped job")?;

        info!(tenant_id = %tenant_id, job_type = %job.job_type, "Enqueued tenant-scoped job");
    }

    Ok(())
}

async fn fetch_active_tenant_ids(pg_manager: Arc<billforge_db::PgManager>) -> Result<Vec<String>> {
    sqlx::query_scalar("SELECT id::text FROM tenants WHERE active = true")
        .fetch_all(pg_manager.metadata())
        .await
        .context("Failed to fetch active tenants")
}

fn build_tenant_jobs(tenant_ids: &[String], job_type: JobType) -> Vec<Job> {
    tenant_ids
        .iter()
        .map(|tenant_id| Job {
            id: uuid::Uuid::new_v4().to_string(),
            job_type: job_type.clone(),
            tenant_id: tenant_id.clone(),
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            retry_count: 0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tenant_jobs_never_uses_system_tenant() {
        let tenant_ids = vec![
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ];

        let jobs = build_tenant_jobs(&tenant_ids, JobType::ForecastRefresh);

        assert_eq!(jobs.len(), tenant_ids.len());
        assert!(jobs.iter().all(|job| job.tenant_id != "system"));
        assert!(jobs
            .iter()
            .all(|job| matches!(job.job_type, JobType::ForecastRefresh)));
    }
}
