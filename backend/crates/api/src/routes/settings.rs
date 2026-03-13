//! Settings routes for updating tenant configuration

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, put},
    Json, Router,
};
use billforge_core::{
    domain::{InvoiceStatusConfig, InvoiceStatusConfigInput},
    traits::InvoiceStatusConfigRepository,
};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_settings).put(update_settings))
        .route("/invoice-statuses", get(list_invoice_statuses).put(update_invoice_statuses))
        .route("/invoice-statuses/seed-defaults", axum::routing::post(seed_default_statuses))
        .route("/invoice-statuses/:status_key", delete(delete_invoice_status))
}

#[derive(Debug, Serialize)]
struct SettingsResponse {
    company_name: String,
    timezone: String,
    default_currency: String,
    logo_url: Option<String>,
    primary_color: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateSettingsInput {
    company_name: Option<String>,
    timezone: Option<String>,
    default_currency: Option<String>,
}

async fn get_settings(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Json<SettingsResponse> {
    Json(SettingsResponse {
        company_name: tenant.settings.company_name,
        timezone: tenant.settings.timezone,
        default_currency: tenant.settings.default_currency,
        logo_url: tenant.settings.logo_url,
        primary_color: tenant.settings.primary_color,
    })
}

async fn update_settings(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<UpdateSettingsInput>,
) -> Result<Json<SettingsResponse>, axum::http::StatusCode> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let metadata_db = billforge_db::MetadataDatabase::new(&database_url).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut settings = tenant.settings.clone();
    if let Some(name) = input.company_name {
        settings.company_name = name;
    }
    if let Some(tz) = input.timezone {
        settings.timezone = tz;
    }
    if let Some(cur) = input.default_currency {
        settings.default_currency = cur;
    }

    metadata_db.update_tenant_settings(&tenant.tenant_id, &settings).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SettingsResponse {
        company_name: settings.company_name,
        timezone: settings.timezone,
        default_currency: settings.default_currency,
        logo_url: settings.logo_url,
        primary_color: settings.primary_color,
    }))
}

// -- Invoice Status Config endpoints --

#[derive(Debug, Deserialize)]
struct ListStatusesQuery {
    category: Option<String>,
}

async fn list_invoice_statuses(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<ListStatusesQuery>,
) -> Result<Json<Vec<InvoiceStatusConfig>>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = billforge_db::repositories::InvoiceStatusConfigRepositoryImpl::new(pool);
    let statuses = repo.list(&tenant.tenant_id, query.category.as_deref()).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(statuses))
}

async fn update_invoice_statuses(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(inputs): Json<Vec<InvoiceStatusConfigInput>>,
) -> Result<Json<Vec<InvoiceStatusConfig>>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = billforge_db::repositories::InvoiceStatusConfigRepositoryImpl::new(pool);
    let statuses = repo.upsert_batch(&tenant.tenant_id, inputs).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(statuses))
}

async fn seed_default_statuses(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = billforge_db::repositories::InvoiceStatusConfigRepositoryImpl::new(pool);
    repo.seed_defaults(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "message": "Default statuses seeded" })))
}

async fn delete_invoice_status(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(status_key): Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = state.db.tenant(&tenant.tenant_id).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let repo = billforge_db::repositories::InvoiceStatusConfigRepositoryImpl::new(pool);
    repo.delete(&tenant.tenant_id, &status_key).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "success": true })))
}
