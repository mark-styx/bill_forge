//! Background Job Worker for BillForge
//!
//! Handles:
//! - QuickBooks data synchronization
//! - Scheduled vendor updates
//! - Invoice processing queue
//! - Metrics aggregation
//! - Email notification batching

// Allow dead code in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

use anyhow::Result;
use tracing::info;

mod jobs;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("Starting BillForge Background Worker");

    // Load configuration
    let config = config::WorkerConfig::from_env()?;

    info!("Configuration loaded successfully");

    // Start job worker
    jobs::start_worker(config).await?;

    Ok(())
}
