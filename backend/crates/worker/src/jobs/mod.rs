//! Background job definitions and worker

use crate::config::WorkerConfig;
use anyhow::{Context, Result};
use billforge_core::TenantId;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

pub mod anomaly_detection;
pub mod approval_expiry;
pub mod categorization_training;
pub mod email_batch;
pub mod embedding_refresh;
pub mod forecast_refresh;
pub mod metrics_aggregation;
pub mod ocr_processing;
pub mod quickbooks_sync;
pub mod report_digest;
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
    ForecastRefresh,
    AnomalyDetection,
    EdiProcessInbound,
    EdiSendRemittance,
    EdiSendAck,
    EdiCheckAckStatus,
    OcrProcess,
    ApprovalExpiry,
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
            JobType::ForecastRefresh => write!(f, "ForecastRefresh"),
            JobType::AnomalyDetection => write!(f, "AnomalyDetection"),
            JobType::EdiProcessInbound => write!(f, "EdiProcessInbound"),
            JobType::EdiSendRemittance => write!(f, "EdiSendRemittance"),
            JobType::EdiSendAck => write!(f, "EdiSendAck"),
            JobType::EdiCheckAckStatus => write!(f, "EdiCheckAckStatus"),
            JobType::OcrProcess => write!(f, "OcrProcess"),
            JobType::ApprovalExpiry => write!(f, "ApprovalExpiry"),
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
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        config.job_poll_interval_secs,
                    ))
                    .await;
                }
            }
            Err(e) => {
                error!("Error processing job: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

async fn poll_and_process_job(
    conn: &mut redis::aio::Connection,
    config: &WorkerConfig,
) -> Result<bool> {
    // Try to pop a job from the queue (BRPOP with timeout)
    let result: Option<(String, String)> = conn.brpop("billforge:jobs:queue", 1.0).await?;

    if let Some((_queue, job_json)) = result {
        let job: Job = serde_json::from_str(&job_json)?;

        info!(
            "Processing job: {} for tenant: {}",
            job.job_type.clone() as JobType,
            job.tenant_id
        );

        match process_job(&job, config).await {
            Ok(_) => {
                info!("Job completed successfully: {}", job.id);
                // Mark job as completed
                conn.lpush::<_, _, ()>(
                    format!("billforge:jobs:completed:{}", job.tenant_id),
                    &job.id,
                )
                .await?;
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
                    conn.lpush::<_, _, ()>("billforge:jobs:queue", retry_json)
                        .await?;

                    warn!(
                        "Requeued job {} for retry (attempt {})",
                        job.id, retry_job.retry_count
                    );
                } else {
                    // Move to dead letter queue
                    conn.lpush::<_, _, ()>(
                        format!("billforge:jobs:failed:{}", job.tenant_id),
                        &job.id,
                    )
                    .await?;
                    error!(
                        "Job {} moved to dead letter queue after {} retries",
                        job.id, job.retry_count
                    );
                }
            }
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

fn parse_job_tenant_id(job: &Job) -> Result<TenantId> {
    job.tenant_id
        .parse::<TenantId>()
        .with_context(|| format!("Invalid tenant_id for job {}", job.id))
}

async fn process_job(job: &Job, config: &WorkerConfig) -> Result<()> {
    let tenant_id = parse_job_tenant_id(job)?;
    config
        .pg_manager
        .tenant(&tenant_id)
        .await
        .with_context(|| format!("Tenant validation failed for job {}", job.id))?;
    let tenant_id_str = tenant_id.as_str();

    match job.job_type {
        JobType::QuickBooksVendorSync => {
            quickbooks_sync::sync_vendors(&tenant_id_str, &job.payload, config).await
        }
        JobType::QuickBooksAccountSync => {
            quickbooks_sync::sync_accounts(&tenant_id_str, &job.payload, config).await
        }
        JobType::QuickBooksInvoiceExport => {
            quickbooks_sync::export_invoice(&tenant_id_str, &job.payload, config).await
        }
        JobType::MetricsAggregation => {
            metrics_aggregation::aggregate_metrics(&tenant_id_str, config).await
        }
        JobType::EmailBatch => email_batch::send_batch(&tenant_id_str, &job.payload, config).await,
        JobType::ReportDigest => report_digest::send_digests(&tenant_id_str, config).await,
        JobType::EmbeddingRefresh => {
            embedding_refresh::refresh_tenant_embeddings(config.pg_manager.clone(), &tenant_id)
                .await
                .map(|_| ())
        }
        JobType::CategorizationTraining => categorization_training::learn_from_tenant_feedback(
            config.pg_manager.clone(),
            &tenant_id,
        )
        .await
        .map(|_| ()),
        JobType::RoutingOptimization => {
            routing_optimization::run_tenant_routing_optimization(
                config.pg_manager.clone(),
                &tenant_id,
            )
            .await
        }
        JobType::ForecastRefresh => {
            forecast_refresh::refresh_tenant_forecasts(config.pg_manager.clone(), &tenant_id).await
        }
        JobType::AnomalyDetection => {
            anomaly_detection::detect_tenant_anomalies(config.pg_manager.clone(), &tenant_id).await
        }
        JobType::EdiProcessInbound => {
            // EDI inbound processing is handled inline in the webhook handler for now.
            // This job type is reserved for async/retry processing of EDI documents.
            info!("EDI inbound processing job - payload: {}", job.payload);
            Ok(())
        }
        JobType::EdiSendRemittance => {
            // Sends 820 payment remittance advice to trading partner.
            // Payload: { "invoice_id": "...", "tenant_id": "..." }
            // Requires EDI connection config to build the client.
            info!("EDI send remittance job - payload: {}", job.payload);
            Ok(())
        }
        JobType::EdiSendAck => {
            // Sends 997 functional acknowledgment for a received document.
            // Payload: { "document_id": "...", "tenant_id": "..." }
            info!("EDI send ack job - payload: {}", job.payload);
            Ok(())
        }
        JobType::EdiCheckAckStatus => {
            // Scheduled job to check for ack timeouts on outbound documents.
            // Payload: { "tenant_id": "..." }
            info!("EDI check ack status job - payload: {}", job.payload);
            Ok(())
        }
        JobType::OcrProcess => {
            ocr_processing::process_ocr(&tenant_id_str, &job.payload, config, job.retry_count).await
        }
        JobType::ApprovalExpiry => {
            approval_expiry::process_approval_expiry(&tenant_id_str, config).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn job_with_tenant(tenant_id: String) -> Job {
        Job {
            id: "job-tenant-parse".to_string(),
            job_type: JobType::MetricsAggregation,
            tenant_id,
            payload: json!({}),
            created_at: Utc::now(),
            retry_count: 0,
        }
    }

    #[test]
    fn parse_job_tenant_id_accepts_uuid_tenant() {
        let tenant_uuid = Uuid::new_v4();
        let job = job_with_tenant(tenant_uuid.to_string());

        let parsed = parse_job_tenant_id(&job).unwrap();

        assert_eq!(*parsed.as_uuid(), tenant_uuid);
    }

    #[test]
    fn parse_job_tenant_id_rejects_system_sentinel() {
        let job = job_with_tenant("system".to_string());

        assert!(parse_job_tenant_id(&job).is_err());
    }
}
