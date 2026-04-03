//! Background Job Worker and Scheduler for BillForge
//!
//! Handles:
//! - QuickBooks data synchronization
//! - Scheduled vendor updates
//! - Invoice processing queue
//! - Metrics aggregation
//! - Email notification batching
//! - ML embedding refresh (weekly)
//! - Categorization training (daily)

use anyhow::Result;
use tracing::info;

mod config;
mod jobs;
mod scheduler;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Determine mode from environment
    let mode = std::env::var("WORKER_MODE").unwrap_or_else(|_| "worker".to_string());

    info!("Starting BillForge Background Service (mode: {})", mode);

    // Load configuration (async to initialize database connections)
    let config = config::WorkerConfig::from_env().await?;

    info!("Configuration loaded successfully");

    match mode.as_str() {
        "worker" => {
            // Start job worker only
            jobs::start_worker(config).await?;
        }
        "scheduler" => {
            // Start scheduler only
            scheduler::start_scheduler(config).await?;
        }
        "both" => {
            // Start both worker and scheduler
            let worker_config = config.clone();
            let scheduler_config = config;

            let worker_handle = tokio::spawn(async move {
                if let Err(e) = jobs::start_worker(worker_config).await {
                    tracing::error!("Worker error: {}", e);
                }
            });

            let scheduler_handle = tokio::spawn(async move {
                if let Err(e) = scheduler::start_scheduler(scheduler_config).await {
                    tracing::error!("Scheduler error: {}", e);
                }
            });

            // Wait for either to finish (they should run forever)
            tokio::select! {
                _ = worker_handle => info!("Worker stopped"),
                _ = scheduler_handle => info!("Scheduler stopped"),
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid WORKER_MODE: {}. Must be 'worker', 'scheduler', or 'both'",
                mode
            ));
        }
    }

    Ok(())
}
