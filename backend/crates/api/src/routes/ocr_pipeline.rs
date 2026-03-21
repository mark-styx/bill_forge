//! OCR pipeline routes — batch upload, job management, processing, corrections

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use billforge_ocr_pipeline::{
    BatchProcessor, CreateOcrJobInput, OcrFieldCorrection, OcrJob, OcrJobStatus,
    OcrPipeline, OcrProcessingStats, VendorAlias,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Batch upload
        .route("/upload", post(batch_upload))
        // Job management
        .route("/jobs", get(list_jobs))
        .route("/jobs/:id", get(get_job))
        .route("/jobs/:id/retry", post(retry_job))
        .route("/jobs/:id/cancel", post(cancel_job))
        // Processing (trigger processing of next queued job)
        .route("/process", post(process_next))
        // Stats
        .route("/stats", get(get_stats))
        // Corrections
        .route("/jobs/:id/corrections", post(record_correction))
        // Vendor aliases
        .route("/vendor-aliases", get(list_vendor_aliases))
}

// ──────────────────────────── Query / Body Types ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CorrectionInput {
    pub field_name: String,
    pub original_value: Option<String>,
    pub corrected_value: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessResult {
    pub job: Option<OcrJob>,
}

// ──────────────────────────── Handlers ────────────────────────────

/// Upload multiple files for OCR processing.
/// Each multipart field is treated as a separate document.
async fn batch_upload(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    mut multipart: Multipart,
) -> ApiResult<Json<billforge_ocr_pipeline::BatchUploadResult>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let mut job_ids: Vec<Uuid> = Vec::new();
    let mut errors: Vec<billforge_ocr_pipeline::BatchError> = Vec::new();
    let mut total_files: usize = 0;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Failed to read upload: {}", e)))?
    {
        total_files += 1;

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("document_{}", total_files));

        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let data = match field.bytes().await {
            Ok(d) => d,
            Err(e) => {
                errors.push(billforge_ocr_pipeline::BatchError {
                    file_name: filename,
                    error: format!("Failed to read file: {}", e),
                });
                continue;
            }
        };

        // Validate file size (max 50MB)
        const MAX_SIZE: usize = 50 * 1024 * 1024;
        if data.len() > MAX_SIZE {
            errors.push(billforge_ocr_pipeline::BatchError {
                file_name: filename,
                error: "File too large. Maximum size is 50MB".to_string(),
            });
            continue;
        }

        // Upload to storage
        let document_id = match state
            .storage
            .upload(&tenant.tenant_id, &filename, &data, &content_type)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                errors.push(billforge_ocr_pipeline::BatchError {
                    file_name: filename,
                    error: format!("Storage upload failed: {}", e),
                });
                continue;
            }
        };

        // Create OCR job
        let input = CreateOcrJobInput {
            document_id,
            file_name: filename.clone(),
            mime_type: content_type,
            provider: None,
            priority: None,
            max_attempts: None,
        };

        match pipeline.create_job(&tenant.tenant_id, input).await {
            Ok(job) => job_ids.push(job.id),
            Err(e) => {
                errors.push(billforge_ocr_pipeline::BatchError {
                    file_name: filename,
                    error: e.to_string(),
                });
            }
        }
    }

    let jobs_created = job_ids.len();

    Ok(Json(billforge_ocr_pipeline::BatchUploadResult {
        total_files,
        jobs_created,
        job_ids,
        errors,
    }))
}

/// List OCR jobs with optional status filter and pagination
async fn list_jobs(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<ListJobsQuery>,
) -> ApiResult<Json<Vec<OcrJob>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let status_filter = query
        .status
        .as_deref()
        .map(|s| {
            s.parse::<OcrJobStatus>()
                .map_err(|_| billforge_core::Error::Validation(format!("Invalid status: {}", s)))
        })
        .transpose()?;

    let limit = query.limit.unwrap_or(25);
    let offset = query.offset.unwrap_or(0);

    let jobs = pipeline
        .list_jobs(&tenant.tenant_id, status_filter, limit, offset)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(jobs))
}

/// Get a single OCR job by ID
async fn get_job(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<OcrJob>> {
    let job_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid job ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let job = pipeline
        .get_job(&tenant.tenant_id, job_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(job))
}

/// Retry a failed OCR job
async fn retry_job(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<OcrJob>> {
    let job_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid job ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let job = pipeline
        .retry_job(&tenant.tenant_id, job_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(job))
}

/// Cancel a pending or processing OCR job
async fn cancel_job(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<OcrJob>> {
    let job_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid job ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let job = pipeline
        .cancel_job(&tenant.tenant_id, job_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(job))
}

/// Trigger processing of the next queued OCR job
async fn process_next(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<ProcessResult>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let job = pipeline
        .process_next_job(&tenant.tenant_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(ProcessResult { job }))
}

/// Get OCR processing statistics for the tenant
async fn get_stats(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<OcrProcessingStats>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let stats = pipeline
        .get_stats(&tenant.tenant_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(stats))
}

/// Record a user correction to an OCR-extracted field
async fn record_correction(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
    Json(input): Json<CorrectionInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let job_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid job ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pipeline = OcrPipeline::new((*pool).clone(), state.storage.clone());

    let correction = OcrFieldCorrection {
        job_id,
        field_name: input.field_name,
        original_value: input.original_value,
        corrected_value: input.corrected_value,
        corrected_by: *user.user_id.as_uuid(),
        corrected_at: Utc::now(),
    };

    pipeline
        .record_correction(&tenant.tenant_id, &correction)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// List vendor aliases for the tenant
async fn list_vendor_aliases(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<VendorAlias>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, vendor_id, alias, is_learned, created_at
        FROM vendor_aliases
        WHERE tenant_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(tenant_uuid)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    use sqlx::Row;
    let aliases: Vec<VendorAlias> = rows
        .iter()
        .map(|row| VendorAlias {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            vendor_id: row.get("vendor_id"),
            alias: row.get("alias"),
            is_learned: row.get("is_learned"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(Json(aliases))
}
