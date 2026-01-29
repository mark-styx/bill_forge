//! Document upload, download, and management routes

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use billforge_core::domain::{DocumentRef, DocumentType, InvoiceId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Document operations
        .route("/", post(upload_document))
        .route("/:id", get(download_document))
        .route("/:id/metadata", get(get_document_metadata))
        .route("/:id", delete(delete_document))
        // Invoice documents
        .route("/invoice/:invoice_id", get(list_invoice_documents))
        .route("/invoice/:invoice_id", post(upload_invoice_document))
}

/// Response for document upload
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub url: String,
}

/// Query params for document upload
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    pub invoice_id: Option<String>,
    pub doc_type: Option<String>,
}

/// Upload a document
async fn upload_document(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadResponse>> {
    // Get the file from multipart
    let field = multipart
        .next_field()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Failed to read upload: {}", e)))?
        .ok_or_else(|| billforge_core::Error::Validation("No file provided".to_string()))?;

    let filename = field
        .file_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "document".to_string());
    
    let content_type = field
        .content_type()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    
    let data = field
        .bytes()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Failed to read file: {}", e)))?;

    // Validate file size (max 50MB)
    const MAX_SIZE: usize = 50 * 1024 * 1024;
    if data.len() > MAX_SIZE {
        return Err(billforge_core::Error::Validation(
            "File too large. Maximum size is 50MB".to_string(),
        ).into());
    }

    // Validate mime type
    let allowed_types = vec![
        "application/pdf",
        "image/png",
        "image/jpeg",
        "image/tiff",
        "image/gif",
    ];
    if !allowed_types.contains(&content_type.as_str()) {
        return Err(billforge_core::Error::Validation(format!(
            "File type not allowed. Allowed types: {}",
            allowed_types.join(", ")
        )).into());
    }

    // Upload to storage
    let document_id = state
        .storage
        .upload(&tenant.tenant_id, &filename, &data, &content_type)
        .await?;

    // Determine document type
    let doc_type = match query.doc_type.as_deref() {
        Some("invoice_original") => DocumentType::InvoiceOriginal,
        Some("supporting") => DocumentType::Supporting,
        Some("tax_document") => DocumentType::TaxDocument,
        Some("contract") => DocumentType::Contract,
        _ => DocumentType::InvoiceOriginal,
    };

    // Parse invoice ID if provided
    let invoice_id = query
        .invoice_id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(InvoiceId);

    // Save metadata to database
    let doc_repo = billforge_db::DocumentRepositoryImpl::new(state.db.clone());
    let doc = doc_repo
        .create(
            &tenant.tenant_id,
            document_id,
            filename.clone(),
            content_type.clone(),
            data.len() as u64,
            document_id.to_string(),
            invoice_id,
            doc_type,
        )
        .await?;

    let url = format!("/api/v1/documents/{}", document_id);

    Ok(Json(UploadResponse {
        id: document_id.to_string(),
        filename,
        mime_type: content_type,
        size_bytes: data.len() as u64,
        url,
    }))
}

/// Upload a document for a specific invoice
async fn upload_invoice_document(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(invoice_id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadResponse>> {
    let invoice_uuid = Uuid::parse_str(&invoice_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    // Get the file from multipart
    let field = multipart
        .next_field()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Failed to read upload: {}", e)))?
        .ok_or_else(|| billforge_core::Error::Validation("No file provided".to_string()))?;

    let filename = field
        .file_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "document".to_string());
    
    let content_type = field
        .content_type()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    
    let data = field
        .bytes()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Failed to read file: {}", e)))?;

    // Upload to storage
    let document_id = state
        .storage
        .upload(&tenant.tenant_id, &filename, &data, &content_type)
        .await?;

    // Save metadata to database
    let doc_repo = billforge_db::DocumentRepositoryImpl::new(state.db.clone());
    let doc = doc_repo
        .create(
            &tenant.tenant_id,
            document_id,
            filename.clone(),
            content_type.clone(),
            data.len() as u64,
            document_id.to_string(),
            Some(InvoiceId(invoice_uuid)),
            DocumentType::InvoiceOriginal,
        )
        .await?;

    let url = format!("/api/v1/documents/{}", document_id);

    Ok(Json(UploadResponse {
        id: document_id.to_string(),
        filename,
        mime_type: content_type,
        size_bytes: data.len() as u64,
        url,
    }))
}

/// Download a document
async fn download_document(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let document_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid document ID".to_string()))?;

    // Get document metadata
    let doc_repo = billforge_db::DocumentRepositoryImpl::new(state.db.clone());
    let doc = doc_repo
        .get_by_id(&tenant.tenant_id, document_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Document".to_string(),
            id: id.clone(),
        })?;

    // Download from storage
    let data = state
        .storage
        .download(&tenant.tenant_id, document_id)
        .await?;

    // Build response with proper headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, doc.mime_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", doc.filename),
        )
        .header(header::CONTENT_LENGTH, data.len())
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(data))
        .map_err(|e| billforge_core::Error::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// Get document metadata
async fn get_document_metadata(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<DocumentMetadataResponse>> {
    let document_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid document ID".to_string()))?;

    let doc_repo = billforge_db::DocumentRepositoryImpl::new(state.db.clone());
    let doc = doc_repo
        .get_by_id(&tenant.tenant_id, document_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Document".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(DocumentMetadataResponse {
        id: doc.id.to_string(),
        filename: doc.filename,
        mime_type: doc.mime_type,
        size_bytes: doc.size_bytes,
        invoice_id: doc.invoice_id.map(|id| id.0.to_string()),
        doc_type: format!("{:?}", doc.doc_type).to_lowercase(),
        created_at: doc.created_at.to_rfc3339(),
        url: format!("/api/v1/documents/{}", doc.id),
    }))
}

#[derive(Debug, Serialize)]
pub struct DocumentMetadataResponse {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub invoice_id: Option<String>,
    pub doc_type: String,
    pub created_at: String,
    pub url: String,
}

/// Delete a document
async fn delete_document(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let document_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid document ID".to_string()))?;

    // Delete from storage
    state
        .storage
        .delete(&tenant.tenant_id, document_id)
        .await?;

    // Delete from database
    let doc_repo = billforge_db::DocumentRepositoryImpl::new(state.db.clone());
    doc_repo.delete(&tenant.tenant_id, document_id).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// List documents for an invoice
async fn list_invoice_documents(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(invoice_id): Path<String>,
) -> ApiResult<Json<Vec<DocumentMetadataResponse>>> {
    let invoice_uuid = Uuid::parse_str(&invoice_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let doc_repo = billforge_db::DocumentRepositoryImpl::new(state.db.clone());
    let docs = doc_repo
        .list_for_invoice(&tenant.tenant_id, &InvoiceId(invoice_uuid))
        .await?;

    let responses: Vec<DocumentMetadataResponse> = docs
        .into_iter()
        .map(|doc| DocumentMetadataResponse {
            id: doc.id.to_string(),
            filename: doc.filename,
            mime_type: doc.mime_type,
            size_bytes: doc.size_bytes,
            invoice_id: doc.invoice_id.map(|id| id.0.to_string()),
            doc_type: format!("{:?}", doc.doc_type).to_lowercase(),
            created_at: doc.created_at.to_rfc3339(),
            url: format!("/api/v1/documents/{}", doc.id),
        })
        .collect();

    Ok(Json(responses))
}
