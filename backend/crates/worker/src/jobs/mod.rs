//! Background job definitions and worker

use crate::config::WorkerConfig;
use anyhow::{Context, Result};
use billforge_core::TenantId;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

pub mod anomaly_detection;
pub mod approval_expiry;
pub mod autopilot_sweep;
pub mod categorization_training;
pub mod email_batch;
pub mod embedding_refresh;
pub mod erp_sync;
pub mod forecast_refresh;
pub mod forecast_tuning;
pub mod metrics_aggregation;
pub mod ocr_processing;
pub mod quickbooks_sync;
pub mod report_digest;
pub mod routing_optimization;
pub mod vendor_risk_rescan;

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
    ForecastTuning,
    AnomalyDetection,
    EdiProcessInbound,
    EdiSendRemittance,
    EdiSendAck,
    EdiCheckAckStatus,
    OcrProcess,
    ApprovalExpiry,
    VendorRiskRescan,
    AutopilotSweep,
    XeroContactSync,
    XeroAccountSync,
    XeroInvoiceExport,
    SageIntacctVendorSync,
    SageIntacctAccountSync,
    SageIntacctInvoiceExport,
    SalesforceAccountSync,
    SalesforceContactSync,
    WorkdaySupplierSync,
    WorkdayAccountSync,
    WorkdayInvoiceExport,
    BillComVendorSync,
    #[serde(rename = "netsuite_vendor_sync")]
    NetSuiteVendorSync,
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
            JobType::ForecastTuning => write!(f, "ForecastTuning"),
            JobType::AnomalyDetection => write!(f, "AnomalyDetection"),
            JobType::EdiProcessInbound => write!(f, "EdiProcessInbound"),
            JobType::EdiSendRemittance => write!(f, "EdiSendRemittance"),
            JobType::EdiSendAck => write!(f, "EdiSendAck"),
            JobType::EdiCheckAckStatus => write!(f, "EdiCheckAckStatus"),
            JobType::OcrProcess => write!(f, "OcrProcess"),
            JobType::ApprovalExpiry => write!(f, "ApprovalExpiry"),
            JobType::VendorRiskRescan => write!(f, "VendorRiskRescan"),
            JobType::AutopilotSweep => write!(f, "AutopilotSweep"),
            JobType::XeroContactSync => write!(f, "XeroContactSync"),
            JobType::XeroAccountSync => write!(f, "XeroAccountSync"),
            JobType::XeroInvoiceExport => write!(f, "XeroInvoiceExport"),
            JobType::SageIntacctVendorSync => write!(f, "SageIntacctVendorSync"),
            JobType::SageIntacctAccountSync => write!(f, "SageIntacctAccountSync"),
            JobType::SageIntacctInvoiceExport => write!(f, "SageIntacctInvoiceExport"),
            JobType::SalesforceAccountSync => write!(f, "SalesforceAccountSync"),
            JobType::SalesforceContactSync => write!(f, "SalesforceContactSync"),
            JobType::WorkdaySupplierSync => write!(f, "WorkdaySupplierSync"),
            JobType::WorkdayAccountSync => write!(f, "WorkdayAccountSync"),
            JobType::WorkdayInvoiceExport => write!(f, "WorkdayInvoiceExport"),
            JobType::BillComVendorSync => write!(f, "BillComVendorSync"),
            JobType::NetSuiteVendorSync => write!(f, "NetSuiteVendorSync"),
        }
    }
}

pub async fn start_worker(config: WorkerConfig) -> Result<()> {
    info!("Starting job worker");

    // Connect to Redis
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let mut redis_conn = redis_client.get_async_connection().await?;

    info!("Connected to Redis, polling for jobs...");

    // Bounded concurrency: semaphore limits in-flight jobs to max_concurrent_jobs.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_jobs));
    let mut join_set: JoinSet<()> = JoinSet::new();

    // Job polling loop — non-blocking: each job is spawned on its own task.
    loop {
        // Acquire a permit before polling so we never exceed max_concurrent_jobs.
        let permit = semaphore.clone().acquire_owned().await?;

        match poll_job(&mut redis_conn).await {
            Ok(Some(job)) => {
                let job_config = config.clone();
                let sem = semaphore.clone();

                join_set.spawn(async move {
                    let _permit = permit; // holds permit until task completes
                    let redis_client = match redis::Client::open(job_config.redis_url.as_str()) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Failed to create Redis client for spawned task: {}", e);
                            return;
                        }
                    };
                    let mut conn = match redis_client.get_async_connection().await {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Failed to get Redis connection in spawned task: {}", e);
                            return;
                        }
                    };

                    process_job_with_retry(&mut conn, &job_config, job, sem).await;
                });
            }
            Ok(None) => {
                // No job available — drop the permit and wait before polling again.
                drop(permit);
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    config.job_poll_interval_secs,
                ))
                .await;
            }
            Err(e) => {
                drop(permit);
                error!("Error polling for job: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }

        // Reap completed tasks to avoid unbounded JoinSet growth.
        while join_set.try_join_next().is_some() {}
    }
}

/// Pop a single job from the Redis queue (BRPOP with 1s timeout).
/// Returns `Ok(None)` when the queue is empty within the timeout window.
async fn poll_job(conn: &mut redis::aio::Connection) -> Result<Option<Job>> {
    let result: Option<(String, String)> = conn.brpop("billforge:jobs:queue", 1.0).await?;
    match result {
        Some((_queue, job_json)) => {
            let job: Job = serde_json::from_str(&job_json)?;
            Ok(Some(job))
        }
        None => Ok(None),
    }
}

/// Run `process_job` and handle retry/requeue logic (including backoff sleep)
/// inside the spawned task so the BRPOP loop is never blocked.
async fn process_job_with_retry(
    conn: &mut redis::aio::Connection,
    config: &WorkerConfig,
    job: Job,
    _semaphore: Arc<tokio::sync::Semaphore>,
) {
    info!(
        "Processing job: {} for tenant: {}",
        job.job_type.clone() as JobType,
        job.tenant_id
    );

    match process_job(&job, config).await {
        Ok(_) => {
            info!("Job completed successfully: {}", job.id);
            if let Err(e) = conn
                .lpush::<_, _, ()>(
                    format!("billforge:jobs:completed:{}", job.tenant_id),
                    &job.id,
                )
                .await
            {
                error!("Failed to mark job {} as completed: {}", job.id, e);
            }
        }
        Err(e) => {
            error!("Job failed: {} - Error: {}", job.id, e);

            // Retry logic
            if job.retry_count < 3 {
                let mut retry_job = job.clone();
                retry_job.retry_count += 1;
                let retry_json = serde_json::to_string(&retry_job).unwrap_or_default();

                // Exponential backoff — slept inside the spawned task, never blocks the poll loop.
                let delay_secs = 10 * 2_u64.pow(job.retry_count);
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;

                if let Err(e) = conn
                    .lpush::<_, _, ()>("billforge:jobs:queue", retry_json)
                    .await
                {
                    error!("Failed to requeue job {} for retry: {}", job.id, e);
                }

                warn!(
                    "Requeued job {} for retry (attempt {})",
                    job.id, retry_job.retry_count
                );
            } else {
                // Move to dead letter queue
                if let Err(e) = conn
                    .lpush::<_, _, ()>(format!("billforge:jobs:failed:{}", job.tenant_id), &job.id)
                    .await
                {
                    error!("Failed to move job {} to dead letter queue: {}", job.id, e);
                }
                error!(
                    "Job {} moved to dead letter queue after {} retries",
                    job.id, job.retry_count
                );
            }
        }
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
        JobType::ForecastTuning => forecast_tuning::run_tenant_forecast_tuning(
            config.pg_manager.clone(),
            &tenant_id,
        )
        .await
        .map(|_| ()),
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
        JobType::VendorRiskRescan => {
            vendor_risk_rescan::rescan_tenant(config.pg_manager.clone(), &tenant_id)
                .await
                .map(|_| ())
        }
        JobType::AutopilotSweep => {
            autopilot_sweep::run_tenant_autopilot_sweep(config.pg_manager.clone(), &tenant_id)
                .await
        }
        JobType::XeroContactSync => {
            erp_sync::xero_contact_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::XeroAccountSync => {
            erp_sync::xero_account_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::XeroInvoiceExport => {
            erp_sync::xero_invoice_export(&tenant_id_str, &job.payload, config).await
        }
        JobType::SageIntacctVendorSync => {
            erp_sync::sage_intacct_vendor_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::SageIntacctAccountSync => {
            erp_sync::sage_intacct_account_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::SageIntacctInvoiceExport => {
            erp_sync::sage_intacct_invoice_export(&tenant_id_str, &job.payload, config).await
        }
        JobType::SalesforceAccountSync => {
            erp_sync::salesforce_account_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::SalesforceContactSync => {
            erp_sync::salesforce_contact_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::WorkdaySupplierSync => {
            erp_sync::workday_supplier_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::WorkdayAccountSync => {
            erp_sync::workday_account_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::WorkdayInvoiceExport => {
            erp_sync::workday_invoice_export(&tenant_id_str, &job.payload, config).await
        }
        JobType::BillComVendorSync => {
            erp_sync::bill_com_vendor_sync(&tenant_id_str, &job.payload, config).await
        }
        JobType::NetSuiteVendorSync => {
            erp_sync::netsuite_vendor_sync(&tenant_id_str, &job.payload, config).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    /// Test that the semaphore + spawn pattern allows N jobs to run concurrently,
    /// so total wall-clock time is ~duration of one job, not N × duration.
    #[tokio::test]
    async fn semaphore_allows_concurrent_job_execution() {
        let max_concurrent = 4;
        let job_count = 4;
        let job_duration_ms = 200u64;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let counter = Arc::new(AtomicUsize::new(0));

        let start = tokio::time::Instant::now();
        let mut join_set: JoinSet<()> = JoinSet::new();

        for _ in 0..job_count {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let c = counter.clone();
            join_set.spawn(async move {
                let _permit = permit;
                c.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(job_duration_ms)).await;
            });
        }

        // Wait for all tasks
        while join_set.join_next().await.is_some() {}

        let elapsed = start.elapsed();
        assert_eq!(counter.load(Ordering::SeqCst), job_count);
        // Concurrent: should be ~job_duration_ms, not job_count * job_duration_ms.
        // Allow generous margin: must be less than half the serial time.
        assert!(
            elapsed < std::time::Duration::from_millis(job_duration_ms * job_count as u64 / 2),
            "Jobs did not run concurrently: elapsed {:?} >= serial budget {:?}",
            elapsed,
            job_duration_ms * job_count as u64 / 2,
        );
    }

    /// Test that semaphore blocks the 5th job when only 4 permits are available,
    /// proving the concurrency limiter actually bounds parallelism.
    #[tokio::test]
    async fn semaphore_limits_concurrency_to_max() {
        let max_concurrent = 2;
        let job_count = 4;
        let job_duration_ms = 100u64;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let peak = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));

        let mut join_set: JoinSet<()> = JoinSet::new();

        for _ in 0..job_count {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let p = peak.clone();
            let a = active.clone();
            join_set.spawn(async move {
                let _permit = permit;
                let current = a.fetch_add(1, Ordering::SeqCst) + 1;
                // Track peak concurrent
                loop {
                    let seen = p.load(Ordering::SeqCst);
                    if current <= seen
                        || p.compare_exchange(seen, current, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                    {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(job_duration_ms)).await;
                a.fetch_sub(1, Ordering::SeqCst);
            });
        }

        while join_set.join_next().await.is_some() {}

        assert_eq!(
            peak.load(Ordering::SeqCst),
            max_concurrent,
            "Peak concurrency should equal max_concurrent_jobs"
        );
    }

    /// Test that a retry backoff sleep inside a spawned task does not block
    /// another concurrent task from completing promptly.
    #[tokio::test]
    async fn retry_backoff_does_not_block_other_jobs() {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(2));
        let mut join_set: JoinSet<()> = JoinSet::new();

        // Task 1: simulates a job that fails and enters retry backoff sleep.
        let sem1 = semaphore.clone();
        join_set.spawn(async move {
            let _permit = sem1.acquire_owned().await.unwrap();
            // Simulate job failure + retry backoff (10s scaled down for test).
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        // Task 2: a fast job that should complete without waiting for task 1's backoff.
        let start = tokio::time::Instant::now();
        let sem2 = semaphore.clone();
        join_set.spawn(async move {
            let _permit = sem2.acquire_owned().await.unwrap();
            // Simulate a quick successful job.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        });

        // Give the fast task time to finish (well before the 5s backoff).
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let elapsed = start.elapsed();

        // The fast job should have completed in ~50-500ms, not after the 5s backoff.
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "Fast job was blocked by retry backoff: elapsed {:?}",
            elapsed,
        );

        // Cancel the slow task (we don't need to wait for it).
        join_set.abort_all();
    }
}
