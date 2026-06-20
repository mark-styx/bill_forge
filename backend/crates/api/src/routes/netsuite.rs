//! NetSuite ERP integration endpoints
//!
//! NetSuite is gated as a paid add-on but the connect path is intentionally
//! disabled: `POST /connect` returns HTTP 501 with a stable
//! `netsuite_jwt_not_implemented` error code rather than attempting any auth
//! exchange. Real NetSuite OAuth 2.0 M2M requires JWT `client_assertion`
//! signing (RS256 against a NetSuite-issued private key), tracked as separate
//! work. `/status` therefore reports `connected: false` with a `reason` of
//! `jwt_not_implemented` for entitled tenants, and the integrations UI renders
//! a visible "Auth setup pending" card so the entitled-but-unsupported state is
//! disclosed instead of silent.
//!
//! Endpoints:
//! - POST /connect: returns 501 (JWT signing not yet implemented)
//! - POST /disconnect: removes any previously stored connection row
//! - GET /status: reports connection state, including the unsupported reason
//! - POST /sync/vendors: queued no-op until connect is real

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
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
    /// Machine-readable reason the integration is unavailable, when applicable.
    /// Returns `"jwt_not_implemented"` for entitled tenants while JWT
    /// `client_assertion` signing is pending.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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

/// Response body returned by `/connect` while NetSuite JWT signing is pending.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NetSuiteConnectUnsupportedResponse {
    /// Stable machine-readable error code. Always `"netsuite_jwt_not_implemented"`.
    pub error: String,
    /// Human-readable explanation of why the connect path is disabled.
    pub message: String,
    /// Pointer for support / engineering on what is missing.
    pub docs_hint: String,
}

/// Response body for NetSuite disconnect.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NetSuiteDisconnectResponse {
    /// Always "disconnected".
    pub status: String,
}

/// Stable error code returned by `/connect` until JWT signing ships.
pub const NETSUITE_JWT_NOT_IMPLEMENTED_CODE: &str = "netsuite_jwt_not_implemented";

/// Build the body returned by `POST /api/v1/netsuite/connect` while JWT
/// `client_assertion` signing is pending. Exposed for tests so they can verify
/// the contract without spinning up the full AppState.
pub fn netsuite_jwt_not_implemented_body() -> NetSuiteConnectUnsupportedResponse {
    NetSuiteConnectUnsupportedResponse {
        error: NETSUITE_JWT_NOT_IMPLEMENTED_CODE.to_string(),
        message:
            "NetSuite OAuth 2.0 M2M requires JWT client_assertion signing, which is not yet \
             implemented. Connect is disabled until JWT support ships."
                .to_string(),
        docs_hint: "JWT client_assertion signing pending".to_string(),
    }
}

// ──────────────────────────── Handlers ────────────────────────────

/// Connect to NetSuite (intentionally unsupported until JWT signing ships).
///
/// Returns HTTP 501 with a stable `netsuite_jwt_not_implemented` error code.
/// No network call is attempted and no row is written to `netsuite_connections`.
#[utoipa::path(
    post,
    path = "/api/v1/netsuite/connect",
    tag = "NetSuite",
    request_body = NetSuiteConnectRequest,
    responses(
        (status = 501, description = "NetSuite JWT signing not yet implemented", body = NetSuiteConnectUnsupportedResponse),
        (status = 401, description = "Unauthorized"),
        (status = 402, description = "NetSuite add-on not entitled"),
    )
)]
async fn netsuite_connect(
    State(_state): State<AppState>,
    TenantCtx(_tenant): TenantCtx,
    Json(_request): Json<NetSuiteConnectRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::NOT_IMPLEMENTED,
        Json(netsuite_jwt_not_implemented_body()),
    ))
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
            reason: None,
        }
    } else {
        NetSuiteStatus {
            connected: false,
            account_id: None,
            last_sync_at: None,
            sync_enabled: false,
            reason: Some("jwt_not_implemented".to_string()),
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
