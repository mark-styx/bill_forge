//! Theme API routes - stub handlers returning sensible defaults.
//!
//! These handlers eliminate 404s from the frontend theme UI.
//! Logo upload and import endpoints return 501 until a storage layer is added.

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared types matching frontend TypeScript interfaces
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationThemeColors {
    pub primary: String,
    pub accent: String,
    pub capture: String,
    pub processing: String,
    pub vendor: String,
    pub reporting: String,
}

impl Default for OrganizationThemeColors {
    fn default() -> Self {
        Self {
            primary: "#3B82F6".into(),
            accent: "#8B5CF6".into(),
            capture: "#10B981".into(),
            processing: "#F59E0B".into(),
            vendor: "#EC4899".into(),
            reporting: "#6366F1".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationBranding {
    pub logo_url: Option<String>,
    pub logo_mark: Option<String>,
    pub favicon_url: Option<String>,
    pub brand_name: String,
    pub brand_gradient: Option<String>,
    #[serde(rename = "customCSS")]
    pub custom_css: Option<String>,
}

impl Default for OrganizationBranding {
    fn default() -> Self {
        Self {
            logo_url: None,
            logo_mark: None,
            favicon_url: None,
            brand_name: "BillForge".into(),
            brand_gradient: None,
            custom_css: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientConfig {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub gradient_type: String,
    pub angle: Option<f64>,
    pub positions: Option<Vec<GradientStop>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientStop {
    pub color: String,
    pub position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationTheme {
    pub id: String,
    pub tenant_id: String,
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub branding: OrganizationBranding,
    pub enabled_for_all_users: bool,
    pub allow_user_override: bool,
    pub gradient_config: Option<GradientConfig>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOrganizationThemeInput {
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub branding: OrganizationBranding,
    pub enabled_for_all_users: Option<bool>,
    pub allow_user_override: Option<bool>,
    pub gradient_config: Option<GradientConfig>,
}

/// Partial update input for PUT /organization/theme.
/// Frontend sends `Partial<CreateOrganizationThemeInput>` so every field is optional.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateOrganizationThemeInput {
    pub preset_id: Option<String>,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub branding: Option<OrganizationBranding>,
    pub enabled_for_all_users: Option<bool>,
    pub allow_user_override: Option<bool>,
    pub gradient_config: Option<GradientConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserThemePreference {
    pub id: String,
    pub user_id: String,
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub mode: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserThemeInput {
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub mode: String,
}

/// Partial update input for PUT /user/theme.
/// Frontend sends `Partial<CreateUserThemeInput>` so every field is optional.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserThemeInput {
    pub preset_id: Option<String>,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveTheme {
    pub theme: Option<OrganizationTheme>,
    pub user_preference: Option<UserThemePreference>,
    pub effective_colors: OrganizationThemeColors,
    pub effective_mode: String,
    pub can_override: bool,
}

// ---------------------------------------------------------------------------
// Organization theme routes
// ---------------------------------------------------------------------------

pub fn org_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_org_theme).post(create_org_theme).put(update_org_theme).delete(delete_org_theme))
        .route("/logo", post(upload_logo))
        .route("/logo/{type}", delete(delete_logo))
        .route("/preview", post(preview_theme))
        .route("/export", get(export_theme))
        .route("/import", post(import_theme))
}

#[utoipa::path(get, path = "/api/v1/organization/theme", tag = "Theme", responses((status = 200, description = "Organization theme")))]
async fn get_org_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Json<OrganizationTheme> {
    let now = chrono::Utc::now().to_rfc3339();
    Json(OrganizationTheme {
        id: uuid::Uuid::nil().to_string(),
        tenant_id: tenant.tenant_id.to_string(),
        preset_id: "default".into(),
        custom_colors: None,
        branding: OrganizationBranding::default(),
        enabled_for_all_users: false,
        allow_user_override: true,
        gradient_config: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[utoipa::path(post, path = "/api/v1/organization/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme created")))]
async fn create_org_theme(
    State(_state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<CreateOrganizationThemeInput>,
) -> Json<OrganizationTheme> {
    let tenant_id = tenant.tenant_id.to_string();
    let _ = (user, tenant);
    let now = chrono::Utc::now().to_rfc3339();
    Json(OrganizationTheme {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id,
        preset_id: input.preset_id,
        custom_colors: input.custom_colors,
        branding: input.branding,
        enabled_for_all_users: input.enabled_for_all_users.unwrap_or(false),
        allow_user_override: input.allow_user_override.unwrap_or(true),
        gradient_config: input.gradient_config,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[utoipa::path(put, path = "/api/v1/organization/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme updated")))]
async fn update_org_theme(
    State(_state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<UpdateOrganizationThemeInput>,
) -> Json<OrganizationTheme> {
    let tenant_id = tenant.tenant_id.to_string();
    let _ = (user, tenant);
    let now = chrono::Utc::now().to_rfc3339();
    Json(OrganizationTheme {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id,
        preset_id: input.preset_id.unwrap_or_else(|| "default".into()),
        custom_colors: input.custom_colors,
        branding: input.branding.unwrap_or_default(),
        enabled_for_all_users: input.enabled_for_all_users.unwrap_or(false),
        allow_user_override: input.allow_user_override.unwrap_or(true),
        gradient_config: input.gradient_config,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[utoipa::path(delete, path = "/api/v1/organization/theme", tag = "Theme", responses((status = 200, description = "Theme deleted")))]
async fn delete_org_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(_tenant): TenantCtx,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "success": true }))
}

#[utoipa::path(post, path = "/api/v1/organization/theme/logo", tag = "Theme", request_body = serde_json::Value, responses((status = 501, description = "Not implemented")))]
async fn upload_logo(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(_tenant): TenantCtx,
) -> axum::http::StatusCode {
    axum::http::StatusCode::NOT_IMPLEMENTED
}

#[utoipa::path(delete, path = "/api/v1/organization/theme/logo/{logo_type}", tag = "Theme", params(("logo_type" = String, Path,)), responses((status = 200, description = "Logo deleted")))]
async fn delete_logo(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(_tenant): TenantCtx,
    Path(_logo_type): Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "success": true }))
}

#[utoipa::path(post, path = "/api/v1/organization/theme/preview", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme preview CSS")))]
async fn preview_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(_tenant): TenantCtx,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "css_variables": {} }))
}

#[utoipa::path(get, path = "/api/v1/organization/theme/export", tag = "Theme", responses((status = 200, description = "Theme export JSON")))]
async fn export_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(_tenant): TenantCtx,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "config": "{}",
        "version": "1.0",
        "exported_at": chrono::Utc::now().to_rfc3339()
    }))
}

#[utoipa::path(post, path = "/api/v1/organization/theme/import", tag = "Theme", request_body = serde_json::Value, responses((status = 501, description = "Not implemented")))]
async fn import_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(_tenant): TenantCtx,
) -> axum::http::StatusCode {
    axum::http::StatusCode::NOT_IMPLEMENTED
}

// ---------------------------------------------------------------------------
// User theme preference routes
// ---------------------------------------------------------------------------

pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_user_theme).post(create_user_theme).put(update_user_theme).delete(delete_user_theme))
        .route("/effective", get(get_effective_theme))
}

#[utoipa::path(get, path = "/api/v1/user/theme", tag = "Theme", responses((status = 200, description = "User theme preference")))]
async fn get_user_theme(
    State(_state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Json<UserThemePreference> {
    let now = chrono::Utc::now().to_rfc3339();
    Json(UserThemePreference {
        id: uuid::Uuid::nil().to_string(),
        user_id: user.user_id.to_string(),
        preset_id: "default".into(),
        custom_colors: None,
        mode: "system".into(),
        created_at: now.clone(),
        updated_at: now,
    })
}

#[utoipa::path(post, path = "/api/v1/user/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme preference created")))]
async fn create_user_theme(
    State(_state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<CreateUserThemeInput>,
) -> Json<UserThemePreference> {
    let now = chrono::Utc::now().to_rfc3339();
    Json(UserThemePreference {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.user_id.to_string(),
        preset_id: input.preset_id,
        custom_colors: input.custom_colors,
        mode: input.mode,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[utoipa::path(put, path = "/api/v1/user/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme preference updated")))]
async fn update_user_theme(
    State(_state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<UpdateUserThemeInput>,
) -> Json<UserThemePreference> {
    let now = chrono::Utc::now().to_rfc3339();
    Json(UserThemePreference {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.user_id.to_string(),
        preset_id: input.preset_id.unwrap_or_else(|| "default".into()),
        custom_colors: input.custom_colors,
        mode: input.mode.unwrap_or_else(|| "system".into()),
        created_at: now.clone(),
        updated_at: now,
    })
}

#[utoipa::path(delete, path = "/api/v1/user/theme", tag = "Theme", responses((status = 200, description = "Theme preference deleted")))]
async fn delete_user_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "success": true }))
}

#[utoipa::path(get, path = "/api/v1/user/theme/effective", tag = "Theme", responses((status = 200, description = "Effective theme (org + user)")))]
async fn get_effective_theme(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Json<EffectiveTheme> {
    Json(EffectiveTheme {
        theme: None,
        user_preference: None,
        effective_colors: OrganizationThemeColors::default(),
        effective_mode: "system".into(),
        can_override: true,
    })
}
