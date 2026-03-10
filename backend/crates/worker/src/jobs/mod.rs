//! Background job definitions and worker

use crate::config::WorkerConfig;
use anyhow::Result;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::{info, error, warn};

pub mod quickbooks_sync;
pub mod metrics_aggregation;
pub mod email_batch;
pub mod report_digest;
pub mod embedding_refresh;
pub mod categorization_training;
pub mod routing_optimization;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Job {
    pub id: String,
    pub job_type: JobType,
    pub tenant_id: String,
    pub payload: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub retry_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    QuickBooksVendorSync,
    QuickBooksAccountSync,
    QuickBooksInvoiceExport,
    MetricsAggregation,
    EmailBatch,
    ReportDigest,
    EmbeddingRefresh,
    CategorizationTraining,
    RoutingOptimization,
}

impl std::fmt::Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::QuickBooksVendorSync => write!(f, "QuickBooksVendorSync"),
            JobType::QuickBooksAccountSync => write!(f, "QuickBooksAccountSync"),
            JobType::QuickBooksInvoiceExport => write!(f, "QuickBooksInvoiceExport"),
            JobType::MetricsAggregation => write!(f, "MetricsAggregation"),
            JobType::EmailBatch => write!(f, "EmailBatch"),
            JobType::ReportDigest => write!(f, "ReportDigest"),
            JobType::EmbeddingRefresh => write!(f, "EmbeddingRefresh"),
            JobType::CategorizationTraining => write!(f, "CategorizationTraining"),
            JobType::RoutingOptimization => write!(f, "RoutingOptimization"),
        }
    }
}

pub async fn start_worker(config: WorkerConfig) -> Result<()> {
    info!("Starting job worker");

    // Connect to Redis
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let mut redis_conn = redis_client.get_async_connection().await?;

    info!("Connected to Redis, polling for jobs...");

    // Job polling loop
    loop {
        match poll_and_process_job(&mut redis_conn, &config).await {
            Ok(processed) => {
                if !processed {
                    // No job available, wait before polling again
                    tokio::time::sleep(tokio::time::Duration::from_secs(config.job_poll_interval_secs)).await;
                }
            }
            Err(e) => {
                error!("Error processing job: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

async fn poll_and_process_job(conn: &mut redis::aio::Connection, config: &WorkerConfig) -> Result<bool> {
    // Try to pop a job from the queue (BRPOP with timeout)
    let result: Option<(String, String)> = conn
        .brpop("billforge:jobs:queue", 1.0)
        .await?;

    if let Some((_queue, job_json)) = result {
        let job: Job = serde_json::from_str(&job_json)?;

        info!("Processing job: {} for tenant: {}", job.job_type.clone() as JobType, job.tenant_id);

        match process_job(&job, config).await {
            Ok(_) => {
                info!("Job completed successfully: {}", job.id);
                // Mark job as completed
                conn.lpush::<_, _, ()>(format!("billforge:jobs:completed:{}", job.tenant_id), &job.id).await?;
            }
            Err(e) => {
                error!("Job failed: {} - Error: {}", job.id, e);

                // Retry logic
                if job.retry_count < 3 {
                    let mut retry_job = job.clone();
                    retry_job.retry_count += 1;
                    let retry_json = serde_json::to_string(&retry_job)?;

                    // Exponential backoff: requeue with delay
                    let delay_secs = 10 * 2_u64.pow(job.retry_count);
                    tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                    conn.lpush::<_, _, ()>("billforge:jobs:queue", retry_json).await?;

                    warn!("Requeued job {} for retry (attempt {})", job.id, retry_job.retry_count);
                } else {
                    // Move to dead letter queue
                    conn.lpush::<_, _, ()>(format!("billforge:jobs:failed:{}", job.tenant_id), &job.id).await?;
                    error!("Job {} moved to dead letter queue after {} retries", job.id, job.retry_count);
                }
            }
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

async fn process_job(job: &Job, config: &WorkerConfig) -> Result<()> {
    match job.job_type {
        JobType::QuickBooksVendorSync => {
            quickbooks_sync::sync_vendors(&job.tenant_id, &job.payload, config).await
        }
        JobType::QuickBooksAccountSync => {
            quickbooks_sync::sync_accounts(&job.tenant_id, &job.payload, config).await
        }
        JobType::QuickBooksInvoiceExport => {
            quickbooks_sync::export_invoice(&job.tenant_id, &job.payload, config).await
        }
        JobType::MetricsAggregation => {
            metrics_aggregation::aggregate_metrics(&job.tenant_id, config).await
        }
        JobType::EmailBatch => {
            email_batch::send_batch(&job.tenant_id, &job.payload, config).await
        }
        JobType::ReportDigest => {
            report_digest::send_digests(&job.tenant_id, config).await
        }
        JobType::EmbeddingRefresh => {
            embedding_refresh::refresh_all_embeddings(config.pg_manager.clone()).await
        }
        JobType::CategorizationTraining => {
            categorization_training::learn_from_feedback(config.pg_manager.clone()).await
        }
        JobType::RoutingOptimization => {
            // Get connection pool
            let pool = config.pg_manager.clone();
            routing_optimization::run_routing_optimization(&pool).await
        }
    }
}
