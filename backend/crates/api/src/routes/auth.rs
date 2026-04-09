//! Authentication routes

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{extract::State, routing::post, Json, Router};
use billforge_auth::{AuthResponse, LoginInput, ProvisionInput, RegisterInput};
use billforge_core::TenantId;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/provision", post(provision))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", post(me))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub tenant_id: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub tenant_id: String,
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "Authentication",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = crate::openapi::LoginResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Invalid credentials")
    )
)]
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let tenant_id: TenantId = req.tenant_id.parse()
        .map_err(|_| ApiError(billforge_core::Error::Validation("Invalid tenant ID".to_string())))?;
    
    let response = state
        .auth
        .login(LoginInput {
            tenant_id,
            email: req.email,
            password: req.password,
        })
        .await?;

    Ok(Json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProvisionRequest {
    pub company_name: String,
    pub admin_email: String,
    pub admin_password: String,
    pub admin_name: String,
    pub timezone: Option<String>,
    pub default_currency: Option<String>,
}

#[utoipa::path(
    post,
    path = "/auth/provision",
    tag = "Authentication",
    request_body = ProvisionRequest,
    responses(
        (status = 200, description = "Tenant provisioned successfully", body = crate::openapi::LoginResponse),
        (status = 400, description = "Invalid request")
    )
)]
async fn provision(
    State(state): State<AppState>,
    Json(req): Json<ProvisionRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let response = state
        .auth
        .provision(ProvisionInput {
            company_name: req.company_name,
            admin_email: req.admin_email,
            admin_password: req.admin_password,
            admin_name: req.admin_name,
            timezone: req.timezone,
            default_currency: req.default_currency,
        })
        .await
        .map_err(|e| {
            // Sanitize database errors for unauthenticated callers
            match &e {
                billforge_core::Error::AlreadyExists { .. } => {
                    ApiError(billforge_core::Error::Validation(
                        "An account with this email already exists".to_string(),
                    ))
                }
                billforge_core::Error::Database(_) => {
                    tracing::error!("Provision database error: {}", e);
                    ApiError(billforge_core::Error::Validation(
                        "Unable to create account. Please try again.".to_string(),
                    ))
                }
                _ => ApiError(e),
            }
        })?;

    // Pre-create the tenant database and run migrations so the first API call works
    let tenant_id: billforge_core::TenantId = response.tenant.id.clone();
    match state.db.tenant(&tenant_id).await {
        Ok(pool) => {
            if let Err(e) = state.db.run_tenant_migrations(&pool).await {
                tracing::warn!("Failed to run tenant migrations for {}: {}", tenant_id.as_str(), e);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to pre-create tenant database for {}: {}", tenant_id.as_str(), e);
        }
    }

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/auth/register",
    tag = "Authentication",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Registration successful", body = crate::openapi::LoginResponse),
        (status = 400, description = "Invalid request")
    )
)]
async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let tenant_id: TenantId = req.tenant_id.parse()
        .map_err(|_| ApiError(billforge_core::Error::Validation("Invalid tenant ID".to_string())))?;
    
    let response = state
        .auth
        .register(RegisterInput {
            tenant_id,
            email: req.email,
            password: req.password,
            name: req.name,
            roles: vec![billforge_core::Role::ApUser],
        })
        .await?;

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/auth/refresh",
    tag = "Authentication",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Token refreshed", body = crate::openapi::LoginResponse),
        (status = 401, description = "Invalid refresh token")
    )
)]
async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let response = state.auth.refresh(&req.refresh_token).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "Authentication",
    responses(
        (status = 200, description = "Logged out successfully"),
        (status = 401, description = "Unauthorized")
    )
)]
async fn logout(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    state.auth.logout(&user.user_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    pub user_id: String,
    pub tenant_id: String,
    pub email: String,
    pub roles: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/auth/me",
    tag = "Authentication",
    responses(
        (status = 200, description = "Current user info", body = MeResponse),
        (status = 401, description = "Unauthorized")
    )
)]
async fn me(AuthUser(user): AuthUser) -> ApiResult<Json<MeResponse>> {
    Ok(Json(MeResponse {
        user_id: user.user_id.0.to_string(),
        tenant_id: user.tenant_id.as_str(),
        email: user.email,
        roles: user.roles.iter().map(|r| r.as_str().to_string()).collect(),
    }))
}
