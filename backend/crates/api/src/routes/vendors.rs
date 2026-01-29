//! Vendor routes (Vendor Management module)

use crate::error::ApiResult;
use crate::extractors::VendorMgmtAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{CreateVendorInput, UpdateVendorInput, Vendor, VendorContact, VendorFilters},
    traits::VendorRepository,
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

    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    let vendors = repo.list(&tenant.tenant_id, &filters, &pagination).await?;

    Ok(Json(vendors))
}

async fn get_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vendor>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    let vendor = repo.get_by_id(&tenant.tenant_id, &vendor_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(vendor))
}

async fn create_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Json(input): Json<CreateVendorInput>,
) -> ApiResult<Json<Vendor>> {
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    let vendor = repo.create(&tenant.tenant_id, input).await?;

    Ok(Json(vendor))
}

async fn update_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(input): Json<UpdateVendorInput>,
) -> ApiResult<Json<Vendor>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    let vendor = repo.update(&tenant.tenant_id, &vendor_id, input).await?;

    Ok(Json(vendor))
}

async fn delete_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    repo.delete(&tenant.tenant_id, &vendor_id).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn add_contact(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(contact): Json<VendorContact>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    repo.add_contact(&tenant.tenant_id, &vendor_id, contact).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn remove_contact(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path((id, contact_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    let contact_uuid = Uuid::parse_str(&contact_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid contact ID".to_string()))?;
    
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(state.db.clone());
    repo.remove_contact(&tenant.tenant_id, &vendor_id, contact_uuid).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn list_tax_documents(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    // TODO: Implement tax document listing
    Ok(Json(vec![]))
}

async fn upload_tax_document(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    // TODO: Implement tax document upload
    Ok(Json(serde_json::json!({ "message": "Tax document uploaded" })))
}

async fn list_messages(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    // TODO: Implement message listing
    Ok(Json(vec![]))
}

#[derive(Deserialize)]
pub struct SendMessageInput {
    pub subject: String,
    pub body: String,
}

async fn send_message(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(input): Json<SendMessageInput>,
) -> ApiResult<Json<serde_json::Value>> {
    // TODO: Implement message sending
    Ok(Json(serde_json::json!({ "message": "Message sent" })))
}
