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
    tokio::spawn(async move {
        if let Err(e) = schedule_categorization_training_task(redis_client2).await {
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
async fn schedule_categorization_training_task(redis_client: redis::Client) -> Result<()> {
    // Run every 24 hours (daily)
    let mut interval = interval(Duration::from_secs(24 * 60 * 60));

    info!("Categorization training scheduler started (daily)");

    loop {
        interval.tick().await;

        let mut conn = redis_client.get_async_connection().await?;
        match enqueue_training_job(&mut conn).await {
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
    // Get all active tenants
    let metadata_pool = pg_manager.metadata();
    let tenants: Vec<String> = sqlx::query_scalar(
        "SELECT id::text FROM tenants WHERE active = true",
    )
    .fetch_all(metadata_pool)
    .await
    .context("Failed to fetch active tenants")?;

    // Enqueue job for each tenant
    for tenant_id in tenants {
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            job_type: JobType::EmbeddingRefresh,
            tenant_id: tenant_id.clone(),
            payload: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            retry_count: 0,
        };

        let job_json = serde_json::to_string(&job)?;
        conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
            .await
            .context("Failed to enqueue embedding refresh job")?;

        info!(tenant_id = %tenant_id, "Enqueued embedding refresh job");
    }

    Ok(())
}

/// Enqueue a categorization training job
async fn enqueue_training_job(conn: &mut redis::aio::Connection) -> Result<()> {
    // System-wide job that processes all tenants
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        job_type: JobType::CategorizationTraining,
        tenant_id: "system".to_string(), // System-wide job
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        retry_count: 0,
    };

    let job_json = serde_json::to_string(&job)?;
    conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
        .await
        .context("Failed to enqueue categorization training job")?;

    Ok(())
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
        match enqueue_forecast_refresh_job(&mut conn).await {
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
        match enqueue_anomaly_detection_job(&mut conn).await {
            Ok(_) => info!("Enqueued daily anomaly detection job"),
            Err(e) => error!("Failed to enqueue anomaly detection job: {}", e),
        }
    }
}

/// Enqueue a forecast refresh job
async fn enqueue_forecast_refresh_job(conn: &mut redis::aio::Connection) -> Result<()> {
    // System-wide job that processes all tenants
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        job_type: JobType::ForecastRefresh,
        tenant_id: "system".to_string(),
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        retry_count: 0,
    };

    let job_json = serde_json::to_string(&job)?;
    conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
        .await
        .context("Failed to enqueue forecast refresh job")?;

    Ok(())
}

/// Enqueue an anomaly detection job
async fn enqueue_anomaly_detection_job(conn: &mut redis::aio::Connection) -> Result<()> {
    // System-wide job that processes all tenants
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        job_type: JobType::AnomalyDetection,
        tenant_id: "system".to_string(),
        payload: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        retry_count: 0,
    };

    let job_json = serde_json::to_string(&job)?;
    conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
        .await
        .context("Failed to enqueue anomaly detection job")?;

    Ok(())
}
