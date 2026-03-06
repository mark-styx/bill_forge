//! Worker configuration

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub database_url: String,
    pub redis_url: String,
    pub tenant_db_path: String,
    pub job_poll_interval_secs: u64,
    pub max_concurrent_jobs: usize,
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL must be set")?,
            redis_url: std::env::var("REDIS_URL")
                .context("REDIS_URL must be set")?,
            tenant_db_path: std::env::var("TENANT_DB_PATH")
                .unwrap_or_else(|_| "/var/lib/billforge/tenants".to_string()),
            job_poll_interval_secs: std::env::var("JOB_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .context("Invalid JOB_POLL_INTERVAL_SECS")?,
            max_concurrent_jobs: std::env::var("MAX_CONCURRENT_JOBS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .context("Invalid MAX_CONCURRENT_JOBS")?,
        })
    }
}
