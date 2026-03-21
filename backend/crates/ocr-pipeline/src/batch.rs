//! Batch OCR processing
//!
//! Handles multi-file upload and creates OCR jobs for each file.
//! Validates file types and creates jobs in a single transaction.

use crate::error::PipelineError;
use crate::pipeline::OcrPipeline;
use crate::types::*;
use billforge_core::types::TenantId;
use uuid::Uuid;

/// Supported MIME types for OCR processing
const SUPPORTED_MIME_TYPES: &[&str] = &[
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/tiff",
    "image/bmp",
    "image/webp",
];

/// Batch processor for multi-file OCR uploads
pub struct BatchProcessor<'a> {
    pipeline: &'a OcrPipeline,
}

impl<'a> BatchProcessor<'a> {
    /// Create a new batch processor
    pub fn new(pipeline: &'a OcrPipeline) -> Self {
        Self { pipeline }
    }

    /// Process a batch of files, creating OCR jobs for each.
    ///
    /// Returns a summary of created jobs and any per-file errors.
    /// Individual file failures do not abort the entire batch.
    pub async fn upload_batch(
        &self,
        tenant_id: &TenantId,
        files: Vec<BatchFileInput>,
        provider: Option<String>,
        priority: Option<i32>,
    ) -> Result<BatchUploadResult, PipelineError> {
        let total_files = files.len();
        let mut job_ids: Vec<Uuid> = Vec::with_capacity(total_files);
        let mut errors: Vec<BatchError> = Vec::new();

        for file in files {
            // Validate MIME type
            if !SUPPORTED_MIME_TYPES.contains(&file.mime_type.as_str()) {
                errors.push(BatchError {
                    file_name: file.file_name.clone(),
                    error: format!(
                        "Unsupported file type '{}'. Supported: {}",
                        file.mime_type,
                        SUPPORTED_MIME_TYPES.join(", ")
                    ),
                });
                continue;
            }

            let input = CreateOcrJobInput {
                document_id: file.document_id,
                file_name: file.file_name.clone(),
                mime_type: file.mime_type.clone(),
                provider: provider.clone(),
                priority,
                max_attempts: None,
            };

            match self.pipeline.create_job(tenant_id, input).await {
                Ok(job) => {
                    job_ids.push(job.id);
                }
                Err(e) => {
                    errors.push(BatchError {
                        file_name: file.file_name,
                        error: e.to_string(),
                    });
                }
            }
        }

        let jobs_created = job_ids.len();

        tracing::info!(
            total = total_files,
            created = jobs_created,
            failed = errors.len(),
            "Batch upload completed"
        );

        Ok(BatchUploadResult {
            total_files,
            jobs_created,
            job_ids,
            errors,
        })
    }

    /// Process all pending jobs for a tenant (up to max_jobs).
    ///
    /// Returns the number of jobs processed.
    pub async fn process_pending(
        &self,
        tenant_id: &TenantId,
        max_jobs: usize,
    ) -> Result<usize, PipelineError> {
        let mut processed = 0;

        for _ in 0..max_jobs {
            match self.pipeline.process_next_job(tenant_id).await {
                Ok(Some(_)) => {
                    processed += 1;
                }
                Ok(None) => {
                    // No more pending jobs
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to process job in batch");
                    // Continue processing remaining jobs
                }
            }
        }

        tracing::info!(processed = processed, "Batch processing completed");

        Ok(processed)
    }
}

/// Input for a single file in a batch upload
#[derive(Debug, Clone)]
pub struct BatchFileInput {
    /// Document file ID (already uploaded to storage)
    pub document_id: Uuid,
    /// Original file name
    pub file_name: String,
    /// MIME type
    pub mime_type: String,
}
