//! Vendor routes (Vendor Management module)

use crate::error::ApiResult;
use crate::extractors::VendorMgmtAccess;
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{CreateVendorInput, UpdateVendorInput, Vendor, VendorContact, VendorFilters},
    traits::{TaxDocumentRepository, VendorRepository},
    types::{PaginatedResponse, Pagination},
};
use serde::Deserialize;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_vendors))
        .route("/", post(create_vendor))
        .route("/:id", get(get_vendor))
        .route("/:id", put(update_vendor))
        .route("/:id", delete(delete_vendor))
        .route("/:id/contacts", post(add_contact))
        .route("/:id/contacts/:contact_id", delete(remove_contact))
        .route("/:id/documents", get(list_tax_documents))
        .route("/:id/documents", post(upload_tax_document))
        .route("/:id/messages", get(list_messages))
        .route("/:id/messages", post(send_message))
}

#[derive(Debug, Deserialize)]
pub struct ListVendorsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,
    pub vendor_type: Option<String>,
    pub search: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/vendors", tag = "Vendors",
    params(("page" = Option<u32>, Query,), ("per_page" = Option<u32>, Query,), ("status" = Option<String>, Query,), ("search" = Option<String>, Query,)),
    responses((status = 200, description = "Paginated vendor list"), (status = 401, description = "Unauthorized")))]
async fn list_vendors(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Query(query): Query<ListVendorsQuery>,
) -> ApiResult<Json<PaginatedResponse<Vendor>>> {
    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(25),
    };

    let filters = VendorFilters {
        search: query.search,
        ..Default::default()
    };

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    let vendors = repo.list(&tenant.tenant_id, &filters, &pagination).await?;

    Ok(Json(vendors))
}

#[utoipa::path(get, path = "/api/v1/vendors/{id}", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor details"), (status = 404, description = "Vendor not found")))]
async fn get_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vendor>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    let vendor = repo.get_by_id(&tenant.tenant_id, &vendor_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(vendor))
}

#[utoipa::path(post, path = "/api/v1/vendors", tag = "Vendors", request_body = serde_json::Value,
    responses((status = 200, description = "Vendor created"), (status = 401, description = "Unauthorized")))]
async fn create_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Json(input): Json<CreateVendorInput>,
) -> ApiResult<Json<Vendor>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    let vendor = repo.create(&tenant.tenant_id, input).await?;

    Ok(Json(vendor))
}

#[utoipa::path(put, path = "/api/v1/vendors/{id}", tag = "Vendors", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor updated"), (status = 404, description = "Vendor not found")))]
async fn update_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(input): Json<UpdateVendorInput>,
) -> ApiResult<Json<Vendor>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    let vendor = repo.update(&tenant.tenant_id, &vendor_id, input).await?;

    Ok(Json(vendor))
}

#[utoipa::path(delete, path = "/api/v1/vendors/{id}", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor deleted"), (status = 404, description = "Vendor not found")))]
async fn delete_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    repo.delete(&tenant.tenant_id, &vendor_id).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/vendors/{id}/contacts", tag = "Vendors", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Contact added"), (status = 404, description = "Vendor not found")))]
async fn add_contact(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(contact): Json<VendorContact>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    repo.add_contact(&tenant.tenant_id, &vendor_id, contact).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(delete, path = "/api/v1/vendors/{id}/contacts/{contact_id}", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID"), ("contact_id" = String, Path, description = "Contact ID")),
    responses((status = 200, description = "Contact removed")))]
async fn remove_contact(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path((id, contact_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    let contact_uuid = Uuid::parse_str(&contact_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid contact ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    repo.remove_contact(&tenant.tenant_id, &vendor_id, contact_uuid).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(get, path = "/api/v1/vendors/{id}/documents", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Tax documents list")))]
async fn list_tax_documents(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::TaxDocumentRepositoryImpl::new(pool);
    let documents = repo.list_for_vendor(&tenant.tenant_id, &vendor_id).await?;

    let result: Vec<serde_json::Value> = documents.into_iter().map(|doc| {
        serde_json::json!({
            "id": doc.id,
            "document_type": doc.document_type,
            "tax_year": doc.tax_year,
            "file_name": doc.file_name,
            "received_date": doc.received_date,
            "expires_date": doc.expires_date,
            "created_at": doc.created_at,
        })
    }).collect();

    Ok(Json(result))
}

#[utoipa::path(post, path = "/api/v1/vendors/{id}/documents", tag = "Vendors",
    request_body(content = inline(String), content_type = "multipart/form-data"),
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Document uploaded")))]
async fn upload_tax_document(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::TaxDocumentRepositoryImpl::new(pool);

    // Extract file from multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        billforge_core::Error::Validation(format!("Failed to read multipart data: {}", e))
    })? {
        let file_name = field.file_name().unwrap_or("document.pdf").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();

        if let Ok(data) = field.bytes().await {
            let file_size = data.len() as i64;

            // Generate file path (in production, this would upload to S3 or similar)
            let file_path = format!("vendor_documents/{}/{}", vendor_id, uuid::Uuid::new_v4());

            // Store metadata in database
            let doc_id = repo.create(
                &tenant.tenant_id,
                &vendor_id,
                "w9".to_string(), // Default to W9, could be configurable
                file_name.clone(),
                file_path,
                file_size,
                content_type,
                Some(user.user_id.0),
            ).await?;

            return Ok(Json(serde_json::json!({
                "id": doc_id,
                "message": "Tax document uploaded successfully",
                "file_name": file_name
            })));
        }
    }

    Err(billforge_core::Error::Validation("No file uploaded".to_string()).into())
}

#[utoipa::path(get, path = "/api/v1/vendors/{id}/messages", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor messages")))]
async fn list_messages(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    let messages = repo.list_messages(&tenant.tenant_id, &vendor_id, 50).await?;

    let result: Vec<serde_json::Value> = messages.into_iter().map(|msg| {
        serde_json::json!({
            "id": msg.id,
            "subject": msg.subject,
            "body": msg.body,
            "sender_type": msg.sender_type,
            "sender_name": msg.sender_name,
            "created_at": msg.created_at,
        })
    }).collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct SendMessageInput {
    pub subject: String,
    pub body: String,
}

#[utoipa::path(post, path = "/api/v1/vendors/{id}/messages", tag = "Vendors", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Message sent")))]
async fn send_message(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(input): Json<SendMessageInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);

    let message = repo.send_message(
        &tenant.tenant_id,
        &vendor_id,
        input.subject,
        input.body,
        Some(user.user_id.0),
    ).await?;

    Ok(Json(serde_json::json!({
        "id": message.id,
        "message": "Message sent successfully",
        "created_at": message.created_at
    })))
}
