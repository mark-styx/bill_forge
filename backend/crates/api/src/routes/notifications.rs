//! API routes for notification system
//!
//! Provides endpoints for:
//! - Slack OAuth installation
//! - Teams webhook configuration
//! - User notification preferences
//! - Notification delivery tracking

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Slack OAuth
        .route("/slack/install", post(install_slack))
        .route("/slack/callback", get(slack_callback))
        .route("/slack/status", get(get_slack_status))
        .route("/slack/disconnect", post(disconnect_slack))
        // Teams webhooks
        .route("/teams/configure", post(configure_teams))
        .route("/teams/status", get(get_teams_status))
        .route("/teams/disconnect", post(disconnect_teams))
        // User preferences
        .route("/preferences", get(get_notification_preferences))
        .route("/preferences", put(update_notification_preferences))
}

/// Configure Slack OAuth installation
#[derive(Debug, Deserialize)]
pub struct SlackInstallRequest {
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SlackInstallResponse {
    pub authorize_url: String,
    pub state: String,
}

/// Install Slack for a tenant
#[utoipa::path(post, path = "/api/v1/notifications/slack/install", tag = "Notifications",
    params(("redirect_url" = Option<String>, Query, description = "Custom redirect URL")),
    responses((status = 200, description = "Slack OAuth URL generated")))]
async fn install_slack(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SlackInstallRequest>,
) -> ApiResult<Json<SlackInstallResponse>> {
    // Generate OAuth state
    let state_nonce = Uuid::new_v4().to_string();

    // Store state in database
    sqlx::query!(
        r#"
        INSERT INTO slack_oauth_states (tenant_id, user_id, state_nonce, redirect_url, expires_at)
        VALUES ($1, $2, $3, $4, NOW() + INTERVAL '10 minutes')
        "#,
        &auth_user.0.tenant_id.0,
        &auth_user.0.user_id.0,
        state_nonce,
        query.redirect_url,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    // Generate authorization URL
    let client_id = std::env::var("SLACK_CLIENT_ID")
        .map_err(|_| ApiError(billforge_core::Error::Validation("SLACK_CLIENT_ID not configured".to_string())))?;

    let redirect_uri = std::env::var("SLACK_REDIRECT_URI")
        .map_err(|_| ApiError(billforge_core::Error::Validation("SLACK_REDIRECT_URI not configured".to_string())))?;

    let authorize_url = format!(
        "https://slack.com/oauth/v2/authorize?client_id={}&scope=chat:write,users:read,im:write&redirect_uri={}&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        state_nonce
    );

    Ok(Json(SlackInstallResponse {
        authorize_url,
        state: state_nonce,
    }))
}

/// Slack OAuth callback
#[derive(Debug, Deserialize)]
pub struct SlackCallbackRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct SlackCallbackResponse {
    pub success: bool,
    pub slack_team_name: String,
}

#[utoipa::path(get, path = "/api/v1/notifications/slack/callback", tag = "Notifications",
    responses((status = 200, description = "Slack OAuth callback")))]
async fn slack_callback(
    State(state): State<AppState>,
    Query(query): Query<SlackCallbackRequest>,
) -> ApiResult<Json<SlackCallbackResponse>> {
    // Validate state
    let state_row = sqlx::query!(
        r#"
        SELECT id, tenant_id, user_id, redirect_url
        FROM slack_oauth_states
        WHERE state_nonce = $1
            AND used_at IS NULL
            AND expires_at > NOW()
        "#,
        query.state,
    )
    .fetch_optional(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?
    .ok_or_else(|| ApiError(billforge_core::Error::Validation("Invalid or expired OAuth state".to_string())))?;

    // Mark state as used
    sqlx::query!(
        r#"
        UPDATE slack_oauth_states
        SET used_at = NOW()
        WHERE id = $1
        "#,
        state_row.id,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    // Exchange code for tokens
    let client_id = std::env::var("SLACK_CLIENT_ID")
        .map_err(|_| ApiError(billforge_core::Error::Validation("SLACK_CLIENT_ID not configured".to_string())))?;

    let client_secret = std::env::var("SLACK_CLIENT_SECRET")
        .map_err(|_| ApiError(billforge_core::Error::Validation("SLACK_CLIENT_SECRET not configured".to_string())))?;

    let redirect_uri = std::env::var("SLACK_REDIRECT_URI")
        .map_err(|_| ApiError(billforge_core::Error::Validation("SLACK_REDIRECT_URI not configured".to_string())))?;

    let http_client = reqwest::Client::new();
    let response = http_client
        .post("https://slack.com/api/oauth.v2.access")
        .query(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", query.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|e| ApiError(billforge_core::Error::ExternalService {
            service: "Slack".to_string(),
            message: format!("OAuth failed: {}", e)
        }))?;

    let oauth_response: serde_json::Value = response
        .json()
        .await
        .map_err(|e| ApiError(billforge_core::Error::ExternalService {
            service: "Slack".to_string(),
            message: format!("Failed to parse response: {}", e)
        }))?;

    if !oauth_response["ok"].as_bool().unwrap_or(false) {
        return Err(ApiError(billforge_core::Error::Validation(format!(
            "Slack OAuth failed: {}",
            oauth_response["error"].as_str().unwrap_or("Unknown error")
        ))));
    }

    let slack_team_id = oauth_response["team"]["id"].as_str().unwrap_or("");
    let slack_team_name = oauth_response["team"]["name"].as_str().unwrap_or("");
    let slack_user_id = oauth_response["authed_user"]["id"].as_str().unwrap_or("");
    let bot_user_id = oauth_response["bot_user_id"].as_str().unwrap_or("");
    let access_token = oauth_response["access_token"].as_str().unwrap_or("");
    let bot_access_token = oauth_response["authed_user"]["access_token"].as_str().unwrap_or("");
    let scope = oauth_response["scope"].as_str().unwrap_or("");

    // Store connection in database
    sqlx::query!(
        r#"
        INSERT INTO slack_connections (
            tenant_id, user_id, slack_team_id, slack_team_name, slack_user_id,
            bot_user_id, access_token, bot_access_token, scope, installed_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
        ON CONFLICT (tenant_id, slack_team_id)
        DO UPDATE SET
            slack_team_name = $4,
            slack_user_id = $5,
            bot_user_id = $6,
            access_token = $7,
            bot_access_token = $8,
            scope = $9,
            updated_at = NOW(),
            is_active = true
        "#,
        state_row.tenant_id,
        state_row.user_id,
        slack_team_id,
        slack_team_name,
        slack_user_id,
        bot_user_id,
        access_token,
        bot_access_token,
        scope,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(Json(SlackCallbackResponse {
        success: true,
        slack_team_name: slack_team_name.to_string(),
    }))
}

/// Configure Teams webhook
#[derive(Debug, Deserialize)]
pub struct ConfigureTeamsRequest {
    pub webhook_url: String,
    pub channel_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigureTeamsResponse {
    pub success: bool,
    pub webhook_id: Uuid,
}

#[utoipa::path(post, path = "/api/v1/notifications/teams/configure", tag = "Notifications", request_body = serde_json::Value,
    responses((status = 200, description = "Teams webhook configured")))]
async fn configure_teams(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Json(json): Json<ConfigureTeamsRequest>,
) -> ApiResult<Json<ConfigureTeamsResponse>> {
    // Validate webhook URL format
    if !json.webhook_url.starts_with("https://outlook.office.com/webhook/") {
        return Err(ApiError(billforge_core::Error::Validation("Invalid Teams webhook URL".to_string())));
    }

    let webhook_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO teams_webhooks (
            id, tenant_id, user_id, webhook_url, channel_name, created_at, is_active
        ) VALUES ($1, $2, $3, $4, $5, NOW(), true)
        ON CONFLICT (tenant_id, user_id)
        DO UPDATE SET
            webhook_url = $4,
            channel_name = $5,
            updated_at = NOW(),
            is_active = true
        "#,
        webhook_id,
        &auth_user.0.tenant_id.0,
        &auth_user.0.user_id.0,
        json.webhook_url,
        json.channel_name,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(Json(ConfigureTeamsResponse {
        success: true,
        webhook_id,
    }))
}

/// Get user notification preferences
#[utoipa::path(get, path = "/api/v1/notifications/preferences", tag = "Notifications",
    responses((status = 200, description = "User notification preferences")))]
async fn get_notification_preferences(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let rows = sqlx::query!(
        r#"
        SELECT channel, enabled, notification_types, priority_filter,
               quiet_hours_start, quiet_hours_end, quiet_hours_timezone
        FROM user_notification_preferences
        WHERE user_id = $1
        "#,
        &auth_user.0.user_id.0,
    )
    .fetch_all(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    let preferences: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "channel": row.channel,
                "enabled": row.enabled,
                "notification_types": row.notification_types,
                "priority_filter": row.priority_filter,
                "quiet_hours_start": row.quiet_hours_start,
                "quiet_hours_end": row.quiet_hours_end,
                "quiet_hours_timezone": row.quiet_hours_timezone,
            })
        })
        .collect();

    Ok(Json(preferences))
}

/// Update user notification preferences
#[derive(Debug, Deserialize)]
pub struct UpdatePreferencesRequest {
    pub channel: String,
    pub enabled: bool,
    pub notification_types: Option<Vec<String>>,
    pub priority_filter: Option<String>,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub quiet_hours_timezone: Option<String>,
}

#[utoipa::path(put, path = "/api/v1/notifications/preferences", tag = "Notifications", request_body = serde_json::Value,
    responses((status = 200, description = "Preferences updated")))]
async fn update_notification_preferences(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Json(json): Json<UpdatePreferencesRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        r#"
        INSERT INTO user_notification_preferences (
            tenant_id, user_id, channel, enabled, notification_types,
            priority_filter, quiet_hours_start, quiet_hours_end, quiet_hours_timezone
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (user_id, channel)
        DO UPDATE SET
            enabled = $4,
            notification_types = $5,
            priority_filter = $6,
            quiet_hours_start = $7,
            quiet_hours_end = $8,
            quiet_hours_timezone = $9,
            updated_at = NOW()
        "#,
        &auth_user.0.tenant_id.0,
        &auth_user.0.user_id.0,
        json.channel,
        json.enabled,
        json.notification_types.as_deref().unwrap_or(&[]),
        json.priority_filter,
        json.quiet_hours_start.and_then(|s| chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S").ok()),
        json.quiet_hours_end.and_then(|s| chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S").ok()),
        json.quiet_hours_timezone,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Get Slack connection status
#[utoipa::path(get, path = "/api/v1/notifications/slack/status", tag = "Notifications",
    responses((status = 200, description = "Slack connection status")))]
async fn get_slack_status(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Option<serde_json::Value>>> {
    let connection = sqlx::query!(
        r#"
        SELECT slack_team_id, slack_team_name, installed_at, is_active
        FROM slack_connections
        WHERE tenant_id = $1 AND is_active = true
        LIMIT 1
        "#,
        &auth_user.0.tenant_id.0,
    )
    .fetch_optional(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    let result = connection.map(|row| {
        serde_json::json!({
            "slack_team_id": row.slack_team_id,
            "slack_team_name": row.slack_team_name,
            "installed_at": row.installed_at,
            "is_active": row.is_active,
        })
    });

    Ok(Json(result))
}

/// Get Teams webhook status
#[utoipa::path(get, path = "/api/v1/notifications/teams/status", tag = "Notifications",
    responses((status = 200, description = "Teams connection status")))]
async fn get_teams_status(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Option<serde_json::Value>>> {
    let webhook = sqlx::query!(
        r#"
        SELECT id, channel_name, created_at, is_active
        FROM teams_webhooks
        WHERE user_id = $1 AND is_active = true
        LIMIT 1
        "#,
        &auth_user.0.user_id.0,
    )
    .fetch_optional(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    let result = webhook.map(|row| {
        serde_json::json!({
            "id": row.id,
            "channel_name": row.channel_name,
            "created_at": row.created_at,
            "is_active": row.is_active,
        })
    });

    Ok(Json(result))
}

/// Disconnect Slack
#[utoipa::path(post, path = "/api/v1/notifications/slack/disconnect", tag = "Notifications", request_body = serde_json::Value,
    responses((status = 200, description = "Slack disconnected")))]
async fn disconnect_slack(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        r#"
        UPDATE slack_connections
        SET is_active = false, updated_at = NOW()
        WHERE tenant_id = $1
        "#,
        &auth_user.0.tenant_id.0,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Disconnect Teams
#[utoipa::path(post, path = "/api/v1/notifications/teams/disconnect", tag = "Notifications", request_body = serde_json::Value,
    responses((status = 200, description = "Teams disconnected")))]
async fn disconnect_teams(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        r#"
        UPDATE teams_webhooks
        SET is_active = false, updated_at = NOW()
        WHERE user_id = $1
        "#,
        &auth_user.0.user_id.0,
    )
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(Json(serde_json::json!({ "success": true })))
}
