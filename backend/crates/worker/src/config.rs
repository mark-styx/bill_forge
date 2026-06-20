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
    /// Optional fallback OCR provider — if set and different from `ocr_provider`,
    /// the OCR job will attempt the fallback provider when the primary fails.
    pub ocr_fallback_provider: Option<String>,
    /// When true AND a fallback provider is configured, run both providers
    /// through `OcrComparison` and pick the confidence-weighted winner instead
    /// of the simple `OcrWithFallback` error-only fallback.
    pub ocr_comparison_enabled: bool,
    /// Minimum confidence for the primary provider's result.  When comparison
    /// mode is active and the primary scores below this threshold while the
    /// fallback scores higher, the fallback's extraction is used instead.
    pub ocr_min_confidence: f32,
    /// QuickBooks OAuth client ID (required for token refresh)
    pub qb_client_id: Option<String>,
    /// QuickBooks OAuth client secret (required for token refresh)
    pub qb_client_secret: Option<String>,
    /// QuickBooks environment: "production" or "sandbox"
    pub qb_environment: String,
    /// Xero OAuth client ID (required for token refresh during sync jobs)
    pub xero_client_id: Option<String>,
    /// Xero OAuth client secret
    pub xero_client_secret: Option<String>,
    /// Xero environment: "production" or "sandbox"
    pub xero_environment: String,
    /// Salesforce OAuth client ID (required for token refresh)
    pub salesforce_client_id: Option<String>,
    /// Salesforce OAuth client secret
    pub salesforce_client_secret: Option<String>,
    /// Salesforce environment: "production" or "sandbox"
    pub salesforce_environment: String,
    /// Workday OAuth client ID (required for token refresh)
    pub workday_client_id: Option<String>,
    /// Workday OAuth client secret
    pub workday_client_secret: Option<String>,
}

impl WorkerConfig {
    pub async fn from_env() -> Result<Self> {
        let metadata_db_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let database_url_template = std::env::var("DATABASE_URL_TEMPLATE").unwrap_or_else(|_| {
            // Default template: replace database name in connection string
            metadata_db_url
                .rsplit_once('/')
                .map(|(prefix, _)| format!("{}/{{database}}", prefix))
                .unwrap_or_else(|| metadata_db_url.clone())
        });

        let pg_manager = PgManager::new(&metadata_db_url, database_url_template).await?;

        Ok(Self {
            redis_url: std::env::var("REDIS_URL").context("REDIS_URL must be set")?,
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
            ocr_provider: std::env::var("OCR_PROVIDER").unwrap_or_else(|_| "tesseract".to_string()),
            ocr_fallback_provider: std::env::var("OCR_FALLBACK_PROVIDER").ok(),
            ocr_comparison_enabled: std::env::var("OCR_COMPARISON_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            ocr_min_confidence: std::env::var("OCR_MIN_CONFIDENCE")
                .unwrap_or_else(|_| "0.6".to_string())
                .parse()
                .unwrap_or(0.6),
            qb_client_id: std::env::var("QUICKBOOKS_CLIENT_ID").ok(),
            qb_client_secret: std::env::var("QUICKBOOKS_CLIENT_SECRET").ok(),
            qb_environment: std::env::var("QUICKBOOKS_ENVIRONMENT")
                .unwrap_or_else(|_| "production".to_string()),
            xero_client_id: std::env::var("XERO_CLIENT_ID").ok(),
            xero_client_secret: std::env::var("XERO_CLIENT_SECRET").ok(),
            xero_environment: std::env::var("XERO_ENVIRONMENT")
                .unwrap_or_else(|_| "production".to_string()),
            salesforce_client_id: std::env::var("SALESFORCE_CLIENT_ID").ok(),
            salesforce_client_secret: std::env::var("SALESFORCE_CLIENT_SECRET").ok(),
            salesforce_environment: std::env::var("SALESFORCE_ENVIRONMENT")
                .unwrap_or_else(|_| "production".to_string()),
            workday_client_id: std::env::var("WORKDAY_CLIENT_ID").ok(),
            workday_client_secret: std::env::var("WORKDAY_CLIENT_SECRET").ok(),
        })
    }
}
