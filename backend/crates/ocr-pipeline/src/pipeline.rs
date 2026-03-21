//! OCR processing pipeline
//!
//! Manages the lifecycle of OCR jobs: creation, processing, retry, and cancellation.
//! Uses runtime sqlx queries (not compile-time macros) for all database operations.

use crate::error::PipelineError;
use crate::types::*;
use crate::vendor_matcher::VendorMatcher;
use billforge_core::traits::{OcrService, StorageService};
use billforge_core::types::TenantId;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

/// OCR processing pipeline
pub struct OcrPipeline {
    pool: PgPool,
    storage: Arc<dyn StorageService>,
    vendor_matcher: VendorMatcher,
}

impl OcrPipeline {
    /// Create a new OCR pipeline
    pub fn new(pool: PgPool, storage: Arc<dyn StorageService>) -> Self {
        let vendor_matcher = VendorMatcher::new(pool.clone());
        Self {
            pool,
            storage,
            vendor_matcher,
        }
    }

    // ──────────────────────────── Job Management ────────────────────────────

    /// Create a new OCR job
    pub async fn create_job(
        &self,
        tenant_id: &TenantId,
        input: CreateOcrJobInput,
    ) -> Result<OcrJob, PipelineError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let provider = input.provider.unwrap_or_else(|| "tesseract".to_string());
        let priority = input.priority.unwrap_or(100);
        let max_attempts = input.max_attempts.unwrap_or(3);
        let tenant_uuid = *tenant_id.as_uuid();

        let row = sqlx::query(
            r#"
            INSERT INTO ocr_jobs (
                id, tenant_id, document_id, file_name, mime_type,
                provider, status, attempt_count, max_attempts, priority,
                created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING id, tenant_id, document_id, file_name, mime_type,
                      provider, status, attempt_count, max_attempts,
                      result, matched_vendor_id, vendor_match_confidence,
                      error_message, processing_time_ms, priority,
                      created_at, updated_at, started_at, completed_at
            "#,
        )
        .bind(id)
        .bind(tenant_uuid)
        .bind(input.document_id)
        .bind(&input.file_name)
        .bind(&input.mime_type)
        .bind(&provider)
        .bind(OcrJobStatus::Pending.as_str())
        .bind(0i32)
        .bind(max_attempts)
        .bind(priority)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(row_to_ocr_job(&row))
    }

    /// Process the next pending job from the queue
    pub async fn process_next_job(
        &self,
        tenant_id: &TenantId,
    ) -> Result<Option<OcrJob>, PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();

        // Atomically claim the next pending job (ordered by priority then created_at)
        let maybe_row = sqlx::query(
            r#"
            UPDATE ocr_jobs
            SET status = $1, started_at = $2, updated_at = $2, attempt_count = attempt_count + 1
            WHERE id = (
                SELECT id FROM ocr_jobs
                WHERE tenant_id = $3 AND status = $4
                ORDER BY priority ASC, created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, tenant_id, document_id, file_name, mime_type,
                      provider, status, attempt_count, max_attempts,
                      result, matched_vendor_id, vendor_match_confidence,
                      error_message, processing_time_ms, priority,
                      created_at, updated_at, started_at, completed_at
            "#,
        )
        .bind(OcrJobStatus::Processing.as_str())
        .bind(Utc::now())
        .bind(tenant_uuid)
        .bind(OcrJobStatus::Pending.as_str())
        .fetch_optional(&self.pool)
        .await?;

        let row = match maybe_row {
            Some(r) => r,
            None => return Ok(None),
        };

        let job = row_to_ocr_job(&row);
        let processed = self.process_job(tenant_id, &job).await;

        match processed {
            Ok(completed_job) => Ok(Some(completed_job)),
            Err(e) => {
                // Mark job as failed
                let _ = self.mark_job_failed(&job.id, &e.to_string()).await;
                Err(e)
            }
        }
    }

    /// Process a specific job (run OCR extraction + vendor matching)
    pub async fn process_job(
        &self,
        tenant_id: &TenantId,
        job: &OcrJob,
    ) -> Result<OcrJob, PipelineError> {
        let start = std::time::Instant::now();

        // Download document from storage
        let doc_bytes = self
            .storage
            .download(tenant_id, job.document_id)
            .await
            .map_err(|e| PipelineError::Storage(e.to_string()))?;

        // Create OCR provider and extract
        let ocr_result = self
            .execute_ocr(&job.provider, &doc_bytes, &job.mime_type)
            .await?;

        let processing_time_ms = start.elapsed().as_millis() as i64;

        // Serialize extraction result
        let result_json = serde_json::to_value(&ocr_result)
            .map_err(|e| PipelineError::Internal(format!("Failed to serialize OCR result: {}", e)))?;

        // Vendor matching
        let vendor_match = if let Some(ref vendor_name) = ocr_result.vendor_name.value {
            self.vendor_matcher
                .match_vendor(tenant_id, vendor_name)
                .await
                .unwrap_or(VendorMatchResult {
                    vendor_id: None,
                    vendor_name: None,
                    confidence: 0.0,
                    match_method: VendorMatchMethod::None,
                })
        } else {
            VendorMatchResult {
                vendor_id: None,
                vendor_name: None,
                confidence: 0.0,
                match_method: VendorMatchMethod::None,
            }
        };

        // Update job with results
        let row = sqlx::query(
            r#"
            UPDATE ocr_jobs
            SET status = $1, result = $2, matched_vendor_id = $3,
                vendor_match_confidence = $4, processing_time_ms = $5,
                completed_at = $6, updated_at = $6
            WHERE id = $7
            RETURNING id, tenant_id, document_id, file_name, mime_type,
                      provider, status, attempt_count, max_attempts,
                      result, matched_vendor_id, vendor_match_confidence,
                      error_message, processing_time_ms, priority,
                      created_at, updated_at, started_at, completed_at
            "#,
        )
        .bind(OcrJobStatus::Completed.as_str())
        .bind(&result_json)
        .bind(vendor_match.vendor_id)
        .bind(vendor_match.confidence)
        .bind(processing_time_ms)
        .bind(Utc::now())
        .bind(job.id)
        .fetch_one(&self.pool)
        .await?;

        tracing::info!(
            job_id = %job.id,
            provider = %job.provider,
            processing_time_ms = processing_time_ms,
            vendor_matched = vendor_match.vendor_id.is_some(),
            "OCR job completed"
        );

        Ok(row_to_ocr_job(&row))
    }

    /// Execute OCR extraction using the specified provider
    pub async fn execute_ocr(
        &self,
        provider_name: &str,
        document_bytes: &[u8],
        mime_type: &str,
    ) -> Result<billforge_core::OcrExtractionResult, PipelineError> {
        let provider = billforge_invoice_capture::ocr::create_provider(provider_name);

        let supported = provider.supported_formats();
        if !supported.iter().any(|f| *f == mime_type) {
            return Err(PipelineError::UnsupportedFormat(format!(
                "Provider '{}' does not support MIME type '{}'",
                provider_name, mime_type
            )));
        }

        provider
            .extract(document_bytes, mime_type)
            .await
            .map_err(|e| PipelineError::OcrFailed(e.to_string()))
    }

    /// Get a job by ID
    pub async fn get_job(
        &self,
        tenant_id: &TenantId,
        job_id: Uuid,
    ) -> Result<OcrJob, PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();

        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, document_id, file_name, mime_type,
                   provider, status, attempt_count, max_attempts,
                   result, matched_vendor_id, vendor_match_confidence,
                   error_message, processing_time_ms, priority,
                   created_at, updated_at, started_at, completed_at
            FROM ocr_jobs
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(job_id)
        .bind(tenant_uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PipelineError::JobNotFound(job_id.to_string()))?;

        Ok(row_to_ocr_job(&row))
    }

    /// List jobs for a tenant with pagination
    pub async fn list_jobs(
        &self,
        tenant_id: &TenantId,
        status_filter: Option<OcrJobStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OcrJob>, PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();

        let rows = if let Some(status) = status_filter {
            sqlx::query(
                r#"
                SELECT id, tenant_id, document_id, file_name, mime_type,
                       provider, status, attempt_count, max_attempts,
                       result, matched_vendor_id, vendor_match_confidence,
                       error_message, processing_time_ms, priority,
                       created_at, updated_at, started_at, completed_at
                FROM ocr_jobs
                WHERE tenant_id = $1 AND status = $2
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(tenant_uuid)
            .bind(status.as_str())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, tenant_id, document_id, file_name, mime_type,
                       provider, status, attempt_count, max_attempts,
                       result, matched_vendor_id, vendor_match_confidence,
                       error_message, processing_time_ms, priority,
                       created_at, updated_at, started_at, completed_at
                FROM ocr_jobs
                WHERE tenant_id = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(tenant_uuid)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows.iter().map(row_to_ocr_job).collect())
    }

    /// Cancel a pending or processing job
    pub async fn cancel_job(
        &self,
        tenant_id: &TenantId,
        job_id: Uuid,
    ) -> Result<OcrJob, PipelineError> {
        let job = self.get_job(tenant_id, job_id).await?;

        if job.status.is_terminal() {
            return Err(PipelineError::JobAlreadyCompleted(job_id.to_string()));
        }

        let row = sqlx::query(
            r#"
            UPDATE ocr_jobs
            SET status = $1, updated_at = $2, completed_at = $2
            WHERE id = $3 AND tenant_id = $4
            RETURNING id, tenant_id, document_id, file_name, mime_type,
                      provider, status, attempt_count, max_attempts,
                      result, matched_vendor_id, vendor_match_confidence,
                      error_message, processing_time_ms, priority,
                      created_at, updated_at, started_at, completed_at
            "#,
        )
        .bind(OcrJobStatus::Cancelled.as_str())
        .bind(Utc::now())
        .bind(job_id)
        .bind(*tenant_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;

        tracing::info!(job_id = %job_id, "OCR job cancelled");

        Ok(row_to_ocr_job(&row))
    }

    /// Retry a failed job
    pub async fn retry_job(
        &self,
        tenant_id: &TenantId,
        job_id: Uuid,
    ) -> Result<OcrJob, PipelineError> {
        let job = self.get_job(tenant_id, job_id).await?;

        if job.status != OcrJobStatus::Failed {
            return Err(PipelineError::InvalidStateTransition {
                from: job.status.to_string(),
                to: OcrJobStatus::Pending.to_string(),
            });
        }

        if job.attempt_count >= job.max_attempts {
            return Err(PipelineError::MaxRetriesExceeded(job_id.to_string()));
        }

        let row = sqlx::query(
            r#"
            UPDATE ocr_jobs
            SET status = $1, error_message = NULL, updated_at = $2
            WHERE id = $3 AND tenant_id = $4
            RETURNING id, tenant_id, document_id, file_name, mime_type,
                      provider, status, attempt_count, max_attempts,
                      result, matched_vendor_id, vendor_match_confidence,
                      error_message, processing_time_ms, priority,
                      created_at, updated_at, started_at, completed_at
            "#,
        )
        .bind(OcrJobStatus::Pending.as_str())
        .bind(Utc::now())
        .bind(job_id)
        .bind(*tenant_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;

        tracing::info!(job_id = %job_id, attempt = job.attempt_count, "OCR job queued for retry");

        Ok(row_to_ocr_job(&row))
    }

    /// Record a user correction to an OCR-extracted field
    pub async fn record_correction(
        &self,
        tenant_id: &TenantId,
        correction: &OcrFieldCorrection,
    ) -> Result<(), PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();

        // Verify the job exists and belongs to this tenant
        let _job = self.get_job(tenant_id, correction.job_id).await?;

        sqlx::query(
            r#"
            INSERT INTO ocr_corrections (
                id, tenant_id, job_id, field_name,
                original_value, corrected_value, corrected_by, corrected_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_uuid)
        .bind(correction.job_id)
        .bind(&correction.field_name)
        .bind(&correction.original_value)
        .bind(&correction.corrected_value)
        .bind(correction.corrected_by)
        .bind(correction.corrected_at)
        .execute(&self.pool)
        .await?;

        // If the correction is for vendor_name, teach the vendor matcher
        if correction.field_name == "vendor_name" {
            if let Some(ref original) = correction.original_value {
                // Look up the corrected vendor to learn the alias
                let maybe_vendor = sqlx::query(
                    "SELECT id FROM vendors WHERE tenant_id = $1 AND name = $2 LIMIT 1",
                )
                .bind(tenant_uuid)
                .bind(&correction.corrected_value)
                .fetch_optional(&self.pool)
                .await?;

                if let Some(vendor_row) = maybe_vendor {
                    let vendor_id = vendor_row.get::<Uuid, _>("id");
                    let _ = self
                        .vendor_matcher
                        .learn_alias(tenant_id, vendor_id, original)
                        .await;
                }
            }
        }

        tracing::info!(
            job_id = %correction.job_id,
            field = %correction.field_name,
            "OCR correction recorded"
        );

        Ok(())
    }

    /// Get processing statistics for a tenant
    pub async fn get_stats(
        &self,
        tenant_id: &TenantId,
    ) -> Result<OcrProcessingStats, PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();

        // Job counts by status
        let count_row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_jobs,
                COUNT(*) FILTER (WHERE status = 'pending') as pending_jobs,
                COUNT(*) FILTER (WHERE status = 'processing') as processing_jobs,
                COUNT(*) FILTER (WHERE status = 'completed') as completed_jobs,
                COUNT(*) FILTER (WHERE status = 'failed') as failed_jobs,
                COUNT(*) FILTER (WHERE status = 'cancelled') as cancelled_jobs,
                AVG(processing_time_ms) FILTER (WHERE status = 'completed') as avg_processing_time_ms
            FROM ocr_jobs
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_uuid)
        .fetch_one(&self.pool)
        .await?;

        let total_jobs = count_row.get::<i64, _>("total_jobs");
        let completed_jobs = count_row.get::<i64, _>("completed_jobs");

        // Correction count
        let correction_row = sqlx::query(
            "SELECT COUNT(*) as total_corrections FROM ocr_corrections WHERE tenant_id = $1",
        )
        .bind(tenant_uuid)
        .fetch_one(&self.pool)
        .await?;

        // Vendor match rate
        let vendor_match_rate = if completed_jobs > 0 {
            let match_row = sqlx::query(
                r#"
                SELECT COUNT(*) as matched
                FROM ocr_jobs
                WHERE tenant_id = $1 AND status = 'completed' AND matched_vendor_id IS NOT NULL
                "#,
            )
            .bind(tenant_uuid)
            .fetch_one(&self.pool)
            .await?;

            let matched = match_row.get::<i64, _>("matched");
            Some(matched as f64 / completed_jobs as f64)
        } else {
            None
        };

        Ok(OcrProcessingStats {
            total_jobs,
            pending_jobs: count_row.get::<i64, _>("pending_jobs"),
            processing_jobs: count_row.get::<i64, _>("processing_jobs"),
            completed_jobs,
            failed_jobs: count_row.get::<i64, _>("failed_jobs"),
            cancelled_jobs: count_row.get::<i64, _>("cancelled_jobs"),
            avg_processing_time_ms: count_row.get::<Option<f64>, _>("avg_processing_time_ms"),
            total_corrections: correction_row.get::<i64, _>("total_corrections"),
            vendor_match_rate,
        })
    }

    // ──────────────────────────── Internal Helpers ────────────────────────────

    /// Mark a job as failed with an error message
    async fn mark_job_failed(&self, job_id: &Uuid, error: &str) -> Result<(), PipelineError> {
        sqlx::query(
            r#"
            UPDATE ocr_jobs
            SET status = $1, error_message = $2, updated_at = $3
            WHERE id = $4
            "#,
        )
        .bind(OcrJobStatus::Failed.as_str())
        .bind(error)
        .bind(Utc::now())
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// ──────────────────────────── Row Mapping ────────────────────────────

/// Map a database row to an OcrJob struct
fn row_to_ocr_job(row: &PgRow) -> OcrJob {
    let status_str: String = row.get("status");
    let status = status_str
        .parse::<OcrJobStatus>()
        .unwrap_or(OcrJobStatus::Pending);

    OcrJob {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        document_id: row.get("document_id"),
        file_name: row.get("file_name"),
        mime_type: row.get("mime_type"),
        provider: row.get("provider"),
        status,
        attempt_count: row.get("attempt_count"),
        max_attempts: row.get("max_attempts"),
        result: row.get("result"),
        matched_vendor_id: row.get("matched_vendor_id"),
        vendor_match_confidence: row.get("vendor_match_confidence"),
        error_message: row.get("error_message"),
        processing_time_ms: row.get("processing_time_ms"),
        priority: row.get("priority"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
    }
}
