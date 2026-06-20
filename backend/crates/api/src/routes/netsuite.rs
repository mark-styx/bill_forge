//! NetSuite ERP integration endpoints
//!
//! NetSuite uses OAuth 2.0 Machine-to-Machine authentication.
//!
//! NOTE: The underlying NetSuiteClient crate currently models a simplified
//! client_credentials flow. Real NetSuite OAuth 2.0 M2M requires JWT client
//! assertion (signed with a private key). Until that follow-up lands, the
//! /connect endpoint will surface an honest auth error when called against a
//! real NetSuite account. The route structure, middleware gating, and
//! connections table are fully wired so the integration is reachable and
//! subscription-gated like every other ERP module.
//!
//! Endpoints:
//! - POST /connect — Save NetSuite credentials & test connection
//! - POST /disconnect — Remove stored credentials
//! - GET /status — Check connection status
//! - POST /sync/vendors — Sync vendors from NetSuite

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    extract::State,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use billforge_netsuite::{NetSuiteClient, NetSuiteConfig};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/connect", post(netsuite_connect))
        .route("/disconnect", post(netsuite_disconnect))
        .route("/status", get(netsuite_status))
        .route("/sync/vendors", post(sync_vendors))
}

// ──────────────────────────── Types ────────────────────────────

/// NetSuite connection request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NetSuiteConnectRequest {
    /// NetSuite account ID (e.g. "TSTDRV1234567")
    pub account_id: String,
    /// OAuth 2.0 client ID
    pub client_id: String,
    /// OAuth 2.0 client secret
    pub client_secret: String,
}

/// NetSuite connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NetSuiteStatus {
    /// Whether NetSuite is connected
    pub connected: bool,
    /// NetSuite account ID
    pub account_id: Option<String>,
    /// Last sync timestamp
    pub last_sync_at: Option<String>,
    /// Sync enabled
    pub sync_enabled: bool,
}

/// Sync response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncResponse {
    /// Number of records imported
    pub imported: u64,
    /// Number of records updated
    pub updated: u64,
    /// Number of records skipped
    pub skipped: u64,
}

/// Response body for a successful NetSuite connect.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NetSuiteConnectResponse {
    /// Always "connected".
    pub status: String,
    /// NetSuite account id that was registered.
    pub account_id: String,
}

/// Response body for NetSuite disconnect.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NetSuiteDisconnectResponse {
    /// Always "disconnected".
    pub status: String,
}

// ──────────────────────────── Handlers ────────────────────────────

/// Connect to NetSuite (save credentials & test connection)
#[utoipa::path(
    post,
    path = "/api/v1/netsuite/connect",
    tag = "NetSuite",
    request_body = NetSuiteConnectRequest,
    responses(
        (status = 200, description = "NetSuite connected", body = NetSuiteConnectResponse),
        (status = 400, description = "Invalid credentials or auth not yet implemented"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn netsuite_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<NetSuiteConnectRequest>,
) -> ApiResult<impl IntoResponse> {
    // Test connection by attempting authentication.
    let config = NetSuiteConfig {
        account_id: request.account_id.clone(),
        client_id: request.client_id.clone(),
        client_secret: request.client_secret.clone(),
        base_url: None,
    };
    let mut client = NetSuiteClient::new(config);

    client.authenticate().await.map_err(|e| {
        billforge_core::Error::Validation(format!(
            "NetSuite authentication failed: {}. \
             The current integration uses a simplified client_credentials flow; \
             real NetSuite OAuth 2.0 M2M requires JWT client assertion (follow-up work).",
            e
        ))
    })?;

    // Store credentials in database (encrypted in production)
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query(
        "INSERT INTO netsuite_connections (
            tenant_id, account_id, client_id, client_secret,
            sync_enabled, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            account_id = $2,
            client_id = $3,
            client_secret = $4,
            sync_enabled = true,
            updated_at = NOW()",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&request.account_id)
    .bind(&request.client_id)
    .bind(&request.client_secret) // TODO: encrypt in production
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(NetSuiteConnectResponse {
        status: "connected".to_string(),
        account_id: request.account_id,
    }))
}

/// Disconnect NetSuite
#[utoipa::path(
    post,
    path = "/api/v1/netsuite/disconnect",
    tag = "NetSuite",
    responses(
        (status = 200, description = "NetSuite disconnected", body = NetSuiteDisconnectResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn netsuite_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query("DELETE FROM netsuite_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(NetSuiteDisconnectResponse {
        status: "disconnected".to_string(),
    }))
}

/// Get NetSuite connection status
#[utoipa::path(
    get,
    path = "/api/v1/netsuite/status",
    tag = "NetSuite",
    responses(
        (status = 200, description = "NetSuite status", body = NetSuiteStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn netsuite_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let connection: Option<(String, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT account_id, sync_enabled, last_sync_at
         FROM netsuite_connections
         WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let status = if let Some((account_id, sync_enabled, last_sync_at)) = connection {
        NetSuiteStatus {
            connected: true,
            account_id: Some(account_id),
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        NetSuiteStatus {
            connected: false,
            account_id: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Sync vendors from NetSuite
#[utoipa::path(
    post,
    path = "/api/v1/netsuite/sync/vendors",
    tag = "NetSuite",
    operation_id = "netsuite_sync_vendors",
    responses(
        (status = 200, description = "Vendors synced", body = SyncResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_vendors(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    crate::routes::erp_jobs::enqueue_erp_job(
        state.redis.as_ref(),
        crate::routes::erp_jobs::job_type::NETSUITE_VENDOR_SYNC,
        &tenant.tenant_id,
        serde_json::json!({}),
    )
    .await
}
