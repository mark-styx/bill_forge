//! Worker configuration

use anyhow::{Context, Result};
use billforge_db::PgManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct WorkerConfig {
    pub redis_url: String,
    pub job_poll_interval_secs: u64,
    pub max_concurrent_jobs: usize,
    /// Multi-tenant database manager
    pub pg_manager: Arc<PgManager>,
    /// Base data directory (parent of tenant DBs, contains documents/)
    pub storage_base_path: String,
    /// OCR provider name (tesseract, aws_textract, google_vision)
    pub ocr_provider: String,
}

impl WorkerConfig {
    pub async fn from_env() -> Result<Self> {
        let metadata_db_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must be set")?;
        let database_url_template = std::env::var("DATABASE_URL_TEMPLATE")
            .unwrap_or_else(|_| {
                // Default template: replace database name in connection string
                metadata_db_url.rsplit_once('/').map(|(prefix, _)| {
                    format!("{}/{{database}}", prefix)
                }).unwrap_or_else(|| metadata_db_url.clone())
            });

        let pg_manager = PgManager::new(&metadata_db_url, database_url_template).await?;

        Ok(Self {
            redis_url: std::env::var("REDIS_URL")
                .context("REDIS_URL must be set")?,
            job_poll_interval_secs: std::env::var("JOB_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .context("Invalid JOB_POLL_INTERVAL_SECS")?,
            max_concurrent_jobs: std::env::var("MAX_CONCURRENT_JOBS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .context("Invalid MAX_CONCURRENT_JOBS")?,
            pg_manager: Arc::new(pg_manager),
            storage_base_path: {
                // Use same derivation as the API: parent of TENANT_DB_PATH
                let tenant_db_path = std::env::var("TENANT_DB_PATH")
                    .unwrap_or_else(|_| "./data/tenants".to_string());
                std::path::Path::new(&tenant_db_path)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("./data"))
                    .to_string_lossy()
                    .into_owned()
            },
            ocr_provider: std::env::var("OCR_PROVIDER")
                .unwrap_or_else(|_| "tesseract".to_string()),
        })
    }
}
