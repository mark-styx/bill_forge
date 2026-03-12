//! Settings routes for updating tenant configuration

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::State,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_settings).put(update_settings))
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
