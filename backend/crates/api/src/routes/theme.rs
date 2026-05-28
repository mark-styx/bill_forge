//! Theme API routes - persistent handlers backed by tenant-scoped storage.
//!
//! Organization themes and user preferences are persisted to the tenant database
//! via ThemeRepository. Logo upload and theme import remain 501 (require
//! file/binary storage layer).

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use billforge_core::TenantId;
use billforge_db::repositories::{
    GradientConfig, GradientStop, OrganizationBranding, OrganizationThemeColors, OrganizationThemeRow,
    ThemeRepository, UserThemePreferenceRow,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared API-facing types (kept for request/response compatibility)
// ---------------------------------------------------------------------------

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

impl From<OrganizationThemeRow> for OrganizationTheme {
    fn from(r: OrganizationThemeRow) -> Self {
        Self {
            id: r.id.to_string(),
            tenant_id: r.tenant_id.to_string(),
            preset_id: r.preset_id,
            custom_colors: r.custom_colors,
            branding: r.branding,
            enabled_for_all_users: r.enabled_for_all_users,
            allow_user_override: r.allow_user_override,
            gradient_config: r.gradient_config,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        }
    }
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

impl From<UserThemePreferenceRow> for UserThemePreference {
    fn from(r: UserThemePreferenceRow) -> Self {
        Self {
            id: r.id.to_string(),
            user_id: r.user_id.to_string(),
            preset_id: r.preset_id,
            custom_colors: r.custom_colors,
            mode: r.mode,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserThemeInput {
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub mode: String,
}

/// Partial update input for PUT /user/theme.
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
// Helpers
// ---------------------------------------------------------------------------

/// Acquire a ThemeRepository scoped to the tenant's pool.
async fn theme_repo(state: &AppState, tenant_id: &Uuid) -> Result<ThemeRepository, (axum::http::StatusCode, String)> {
    let tid = TenantId::from_uuid(*tenant_id);
    let pool = state
        .db
        .tenant(&tid)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(ThemeRepository::new(pool))
}

/// Resolve effective colors: user overrides > org theme > defaults.
fn effective_colors(
    org: Option<&OrganizationThemeRow>,
    user: Option<&UserThemePreferenceRow>,
) -> OrganizationThemeColors {
    let mut colors = org
        .and_then(|o| o.custom_colors.clone())
        .unwrap_or_default();
    if let Some(uc) = user.and_then(|u| u.custom_colors.clone()) {
        colors = uc;
    }
    colors
}

fn effective_mode(user: Option<&UserThemePreferenceRow>) -> String {
    user.map(|u| u.mode.clone()).unwrap_or_else(|| "system".into())
}

// ---------------------------------------------------------------------------
// Organization theme routes
// ---------------------------------------------------------------------------

pub fn org_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(get_org_theme)
                .post(create_org_theme)
                .put(update_org_theme)
                .delete(delete_org_theme),
        )
        .route("/logo", post(upload_logo))
        .route("/logo/{type}", delete(delete_logo))
        .route("/preview", post(preview_theme))
        .route("/export", get(export_theme))
        .route("/import", post(import_theme))
}

#[utoipa::path(get, path = "/api/v1/organization/theme", tag = "Theme", responses((status = 200, description = "Organization theme")))]
async fn get_org_theme(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<OrganizationTheme>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;
    match repo.get_org_theme(tenant_uuid).await.map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => Ok(Json(OrganizationTheme::from(row))),
        None => {
            // Return default theme for tenants that haven't saved one yet
            let now = chrono::Utc::now();
            Ok(Json(OrganizationTheme {
                id: Uuid::nil().to_string(),
                tenant_id: tenant_uuid.to_string(),
                preset_id: "default".into(),
                custom_colors: None,
                branding: OrganizationBranding::default(),
                enabled_for_all_users: false,
                allow_user_override: true,
                gradient_config: None,
                created_at: now.to_rfc3339(),
                updated_at: now.to_rfc3339(),
            }))
        }
    }
}

#[utoipa::path(post, path = "/api/v1/organization/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme created")))]
async fn create_org_theme(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<CreateOrganizationThemeInput>,
) -> Result<Json<OrganizationTheme>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;
    let row = repo
        .upsert_org_theme(
            tenant_uuid,
            &input.preset_id,
            input.custom_colors.as_ref(),
            &input.branding,
            input.enabled_for_all_users.unwrap_or(false),
            input.allow_user_override.unwrap_or(true),
            input.gradient_config.as_ref(),
        )
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(OrganizationTheme::from(row)))
}

#[utoipa::path(put, path = "/api/v1/organization/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme updated")))]
async fn update_org_theme(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<UpdateOrganizationThemeInput>,
) -> Result<Json<OrganizationTheme>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;

    // Merge with existing theme so partial updates preserve prior values.
    let existing = repo
        .get_org_theme(tenant_uuid)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (preset, colors, branding, enabled, allow_override, gradient) = match &existing {
        Some(e) => (
            input.preset_id.unwrap_or_else(|| e.preset_id.clone()),
            input.custom_colors.or_else(|| e.custom_colors.clone()),
            input.branding.unwrap_or_else(|| e.branding.clone()),
            input.enabled_for_all_users.unwrap_or(e.enabled_for_all_users),
            input.allow_user_override.unwrap_or(e.allow_user_override),
            input.gradient_config.or_else(|| e.gradient_config.clone()),
        ),
        None => (
            input.preset_id.unwrap_or_else(|| "default".into()),
            input.custom_colors,
            input.branding.unwrap_or_default(),
            input.enabled_for_all_users.unwrap_or(false),
            input.allow_user_override.unwrap_or(true),
            input.gradient_config,
        ),
    };

    let row = repo
        .upsert_org_theme(
            tenant_uuid,
            &preset,
            colors.as_ref(),
            &branding,
            enabled,
            allow_override,
            gradient.as_ref(),
        )
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(OrganizationTheme::from(row)))
}

#[utoipa::path(delete, path = "/api/v1/organization/theme", tag = "Theme", responses((status = 200, description = "Theme deleted")))]
async fn delete_org_theme(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;
    let deleted = repo
        .delete_org_theme(tenant_uuid)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "success": true, "deleted": deleted })))
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
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(logo_type): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;
    let removed = repo
        .delete_logo(tenant_uuid, &logo_type)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "success": true, "removed": removed })))
}

#[utoipa::path(post, path = "/api/v1/organization/theme/preview", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme preview CSS")))]
async fn preview_theme(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;
    let org = repo.get_org_theme(tenant_uuid).await.map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let colors = org
        .as_ref()
        .and_then(|o| o.custom_colors.clone())
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "css_variables": {
            "--color-primary": colors.primary,
            "--color-accent": colors.accent,
            "--color-capture": colors.capture,
            "--color-processing": colors.processing,
            "--color-vendor": colors.vendor,
            "--color-reporting": colors.reporting,
        }
    })))
}

#[utoipa::path(get, path = "/api/v1/organization/theme/export", tag = "Theme", responses((status = 200, description = "Theme export JSON")))]
async fn export_theme(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let repo = theme_repo(&state, &tenant_uuid).await?;
    let org = repo.get_org_theme(tenant_uuid).await.map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match org {
        Some(row) => Ok(Json(serde_json::json!({
            "config": OrganizationTheme::from(row),
            "version": "1.0",
            "exported_at": chrono::Utc::now().to_rfc3339()
        }))),
        None => Ok(Json(serde_json::json!({
            "config": null,
            "version": "1.0",
            "exported_at": chrono::Utc::now().to_rfc3339()
        }))),
    }
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
        .route(
            "/",
            get(get_user_theme)
                .post(create_user_theme)
                .put(update_user_theme)
                .delete(delete_user_theme),
        )
        .route("/effective", get(get_effective_theme))
}

#[utoipa::path(get, path = "/api/v1/user/theme", tag = "Theme", responses((status = 200, description = "User theme preference")))]
async fn get_user_theme(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<UserThemePreference>, (axum::http::StatusCode, String)> {
    let tenant_id = *user.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();
    let repo = theme_repo(&state, &tenant_id).await?;

    match repo.get_user_theme(tenant_id, user_uuid).await.map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => Ok(Json(UserThemePreference::from(row))),
        None => {
            let now = chrono::Utc::now();
            Ok(Json(UserThemePreference {
                id: Uuid::nil().to_string(),
                user_id: user_uuid.to_string(),
                preset_id: "default".into(),
                custom_colors: None,
                mode: "system".into(),
                created_at: now.to_rfc3339(),
                updated_at: now.to_rfc3339(),
            }))
        }
    }
}

#[utoipa::path(post, path = "/api/v1/user/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme preference created")))]
async fn create_user_theme(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<CreateUserThemeInput>,
) -> Result<Json<UserThemePreference>, (axum::http::StatusCode, String)> {
    let tenant_id = *user.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();
    let repo = theme_repo(&state, &tenant_id).await?;

    let row = repo
        .upsert_user_theme(tenant_id, user_uuid, &input.preset_id, input.custom_colors.as_ref(), &input.mode)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(UserThemePreference::from(row)))
}

#[utoipa::path(put, path = "/api/v1/user/theme", tag = "Theme", request_body = serde_json::Value, responses((status = 200, description = "Theme preference updated")))]
async fn update_user_theme(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<UpdateUserThemeInput>,
) -> Result<Json<UserThemePreference>, (axum::http::StatusCode, String)> {
    let tenant_id = *user.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();
    let repo = theme_repo(&state, &tenant_id).await?;

    // Merge with existing so partial updates preserve prior values.
    let existing = repo
        .get_user_theme(tenant_id, user_uuid)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (preset, colors, mode) = match &existing {
        Some(e) => (
            input.preset_id.unwrap_or_else(|| e.preset_id.clone()),
            input.custom_colors.or_else(|| e.custom_colors.clone()),
            input.mode.unwrap_or_else(|| e.mode.clone()),
        ),
        None => (
            input.preset_id.unwrap_or_else(|| "default".into()),
            input.custom_colors,
            input.mode.unwrap_or_else(|| "system".into()),
        ),
    };

    let row = repo
        .upsert_user_theme(tenant_id, user_uuid, &preset, colors.as_ref(), &mode)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(UserThemePreference::from(row)))
}

#[utoipa::path(delete, path = "/api/v1/user/theme", tag = "Theme", responses((status = 200, description = "Theme preference deleted")))]
async fn delete_user_theme(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = *user.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();
    let repo = theme_repo(&state, &tenant_id).await?;

    let deleted = repo
        .delete_user_theme(tenant_id, user_uuid)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "success": true, "deleted": deleted })))
}

#[utoipa::path(get, path = "/api/v1/user/theme/effective", tag = "Theme", responses((status = 200, description = "Effective theme (org + user)")))]
async fn get_effective_theme(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<EffectiveTheme>, (axum::http::StatusCode, String)> {
    let tenant_id = *user.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();
    let repo = theme_repo(&state, &tenant_id).await?;

    let org = repo.get_org_theme(tenant_id).await.map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let usr = repo.get_user_theme(tenant_id, user_uuid).await.map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let can_override = org
        .as_ref()
        .map(|o| o.allow_user_override)
        .unwrap_or(true);

    Ok(Json(EffectiveTheme {
        theme: org.clone().map(OrganizationTheme::from),
        user_preference: usr.clone().map(UserThemePreference::from),
        effective_colors: effective_colors(org.as_ref(), usr.as_ref()),
        effective_mode: effective_mode(usr.as_ref()),
        can_override,
    }))
}
