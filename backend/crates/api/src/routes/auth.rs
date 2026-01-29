//! Authentication routes

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{extract::State, routing::post, Json, Router};
use billforge_auth::{AuthResponse, LoginInput, RegisterInput};
use billforge_core::TenantId;
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", post(me))
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub tenant_id: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub tenant_id: String,
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

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

async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let response = state.auth.refresh(&req.refresh_token).await?;
    Ok(Json(response))
}

async fn logout(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    state.auth.logout(&user.user_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Serialize)]
pub struct MeResponse {
    pub user_id: String,
    pub tenant_id: String,
    pub email: String,
    pub roles: Vec<String>,
}

async fn me(AuthUser(user): AuthUser) -> ApiResult<Json<MeResponse>> {
    Ok(Json(MeResponse {
        user_id: user.user_id.0.to_string(),
        tenant_id: user.tenant_id.as_str(),
        email: user.email,
        roles: user.roles.iter().map(|r| r.as_str().to_string()).collect(),
    }))
}
