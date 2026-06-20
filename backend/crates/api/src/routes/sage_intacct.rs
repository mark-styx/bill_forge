//! Sage Intacct integration endpoints
//!
//! Sage Intacct uses session-based XML Web Services authentication,
//! not OAuth 2.0. The connection flow is:
//! 1. User provides sender credentials + company login in settings UI
//! 2. BillForge stores encrypted credentials
//! 3. On sync, BillForge establishes a session and calls APIs
//!
//! Endpoints:
//! - POST /connect — Save Sage Intacct credentials & test connection
//! - POST /disconnect — Remove stored credentials
//! - GET /status — Check connection status
//! - POST /sync/vendors — Sync vendors from Sage Intacct
//! - POST /sync/accounts — Sync GL accounts from Sage Intacct
//! - POST /export/invoice/:id — Export approved invoice as AP bill
//! - GET /mappings/accounts — Get GL account mappings
//! - POST /mappings/accounts — Update GL account mappings
//! - GET /entities — List available entities (multi-entity support)

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use billforge_core::webhook::{self, WebhookEnvelope};
use billforge_sage_intacct::{SageIntacctAuth, SageIntacctAuthConfig, SageIntacctClient};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Connection endpoints (credential-based, not OAuth)
        .route("/connect", post(sage_intacct_connect))
        .route("/disconnect", post(sage_intacct_disconnect))
        .route("/status", get(sage_intacct_status))
        // Sync endpoints
        .route("/sync/vendors", post(sync_vendors))
        .route("/sync/accounts", post(sync_accounts))
        .route("/export/invoice/:id", post(export_invoice_to_sage))
        // Mapping endpoints
        .route("/mappings/accounts", get(get_account_mappings))
        .route("/mappings/accounts", post(update_account_mappings))
        // Multi-entity support
        .route("/entities", get(list_entities))
        // Webhook secret configuration (requires auth)
        .route("/webhook/configure", post(configure_sage_intacct_webhook))
        // Webhook (no auth - verified via HMAC signature; tenant_id in path)
        .route("/webhook/:tenant_id", post(sage_intacct_webhook))
}

// ──────────────────────────── Types ────────────────────────────

/// Sage Intacct connection request (credential-based)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SageIntacctConnectRequest {
    /// Web Services sender ID
    pub sender_id: String,
    /// Web Services sender password
    pub sender_password: String,
    /// Company ID
    pub company_id: String,
    /// Entity ID (for multi-entity companies, optional)
    pub entity_id: Option<String>,
    /// User ID for company login
    pub user_id: String,
    /// User password
    pub user_password: String,
}

/// Sage Intacct connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SageIntacctStatus {
    /// Whether Sage Intacct is connected
    pub connected: bool,
    /// Company ID
    pub company_id: Option<String>,
    /// Entity ID (if multi-entity)
    pub entity_id: Option<String>,
    /// Last sync timestamp
    pub last_sync_at: Option<String>,
    /// Sync enabled
    pub sync_enabled: bool,
}

/// Sync request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncRequest {
    /// Force full sync (vs incremental)
    pub full_sync: bool,
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

/// Account mapping
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SageAccountMapping {
    /// BillForge GL code
    pub billforge_gl_code: String,
    /// Sage Intacct account number
    pub sage_account_no: String,
    /// Account title
    pub account_title: String,
    /// Account type
    pub account_type: String,
}

/// Export invoice to Sage Intacct
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceRequest {
    /// Invoice ID to export
    pub invoice_id: String,
    /// Sage GL account number
    pub sage_account_no: String,
    /// Department ID (optional)
    pub department_id: Option<String>,
    /// Location/entity ID (optional)
    pub location_id: Option<String>,
}

/// Export invoice response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceResponse {
    /// Sage Intacct record number
    pub sage_record_no: String,
    /// Export status
    pub status: String,
}

// ──────────────────────────── Handlers ────────────────────────────

/// Connect to Sage Intacct (save credentials & test connection)
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/connect",
    tag = "Sage Intacct",
    request_body = SageIntacctConnectRequest,
    responses(
        (status = 200, description = "Sage Intacct connected"),
        (status = 400, description = "Invalid credentials"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sage_intacct_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<SageIntacctConnectRequest>,
) -> ApiResult<impl IntoResponse> {
    // Test connection by establishing a session
    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id: request.sender_id.clone(),
        sender_password: request.sender_password.clone(),
        company_id: request.company_id.clone(),
        entity_id: request.entity_id.clone(),
        user_id: request.user_id.clone(),
        user_password: request.user_password.clone(),
    });

    let session = auth.get_session().await.map_err(|e| {
        billforge_core::Error::Validation(format!(
            "Failed to connect to Sage Intacct: {}. Please verify your credentials.",
            e
        ))
    })?;

    // Store credentials in database (encrypted in production)
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query(
        "INSERT INTO sage_intacct_connections (
            tenant_id, company_id, entity_id, sender_id, sender_password,
            user_id, user_password, sync_enabled, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            company_id = $2,
            entity_id = $3,
            sender_id = $4,
            sender_password = $5,
            user_id = $6,
            user_password = $7,
            sync_enabled = true,
            updated_at = NOW()",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&request.company_id)
    .bind(&request.entity_id)
    .bind(&request.sender_id)
    .bind(&request.sender_password) // TODO: encrypt in production
    .bind(&request.user_id)
    .bind(&request.user_password) // TODO: encrypt in production
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(serde_json::json!({
        "status": "connected",
        "company_id": request.company_id,
        "entity_id": request.entity_id,
        "session_endpoint": session.endpoint,
    })))
}

/// Disconnect Sage Intacct
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/disconnect",
    tag = "Sage Intacct",
    responses(
        (status = 200, description = "Sage Intacct disconnected"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sage_intacct_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query("DELETE FROM sage_intacct_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

/// Get Sage Intacct connection status
#[utoipa::path(
    get,
    path = "/api/v1/sage-intacct/status",
    tag = "Sage Intacct",
    responses(
        (status = 200, description = "Sage Intacct status", body = SageIntacctStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sage_intacct_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let connection: Option<(String, Option<String>, bool, Option<chrono::DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT company_id, entity_id, sync_enabled, last_sync_at
         FROM sage_intacct_connections
         WHERE tenant_id = $1",
        )
        .bind(tenant.tenant_id.as_uuid())
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

    let status = if let Some((company_id, entity_id, sync_enabled, last_sync_at)) = connection {
        SageIntacctStatus {
            connected: true,
            company_id: Some(company_id),
            entity_id,
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        SageIntacctStatus {
            connected: false,
            company_id: None,
            entity_id: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Sync vendors from Sage Intacct (asynchronous)
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/sync/vendors",
    tag = "Sage Intacct",
    request_body = SyncRequest,
    responses(
        (status = 202, description = "Sync enqueued"),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Background queue unavailable")
    )
)]
async fn sync_vendors(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<SyncRequest>,
) -> ApiResult<impl IntoResponse> {
    let payload = serde_json::json!({ "full_sync": request.full_sync });
    crate::routes::erp_jobs::enqueue_erp_job(
        state.redis.as_ref(),
        crate::routes::erp_jobs::job_type::SAGE_INTACCT_VENDOR_SYNC,
        &tenant.tenant_id,
        payload,
    )
    .await
}

/// Sync GL accounts from Sage Intacct (asynchronous)
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/sync/accounts",
    tag = "Sage Intacct",
    responses(
        (status = 202, description = "Sync enqueued"),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Background queue unavailable")
    )
)]
async fn sync_accounts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    crate::routes::erp_jobs::enqueue_erp_job(
        state.redis.as_ref(),
        crate::routes::erp_jobs::job_type::SAGE_INTACCT_ACCOUNT_SYNC,
        &tenant.tenant_id,
        serde_json::json!({}),
    )
    .await
}

/// Export invoice to Sage Intacct as AP bill (asynchronous)
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/export/invoice/{id}",
    tag = "Sage Intacct",
    request_body = ExportInvoiceRequest,
    responses(
        (status = 202, description = "Export enqueued"),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Background queue unavailable")
    )
)]
async fn export_invoice_to_sage(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let payload = serde_json::json!({
        "invoice_id": request.invoice_id,
        "sage_account_no": request.sage_account_no,
        "department_id": request.department_id,
        "location_id": request.location_id,
    });
    crate::routes::erp_jobs::enqueue_erp_job(
        state.redis.as_ref(),
        crate::routes::erp_jobs::job_type::SAGE_INTACCT_INVOICE_EXPORT,
        &tenant.tenant_id,
        payload,
    )
    .await
}

/// Get GL account mappings
#[utoipa::path(
    get,
    path = "/api/v1/sage-intacct/mappings/accounts",
    tag = "Sage Intacct",
    responses(
        (status = 200, description = "Account mappings", body = Vec<SageAccountMapping>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let mappings: Vec<SageAccountMapping> = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT sage_account_no, billforge_gl_code, sage_account_title, sage_account_type
         FROM sage_intacct_account_mappings
         WHERE tenant_id = $1
         ORDER BY sage_account_no",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get account mappings: {}", e)))?
    .into_iter()
    .map(|(sage_no, bf_code, title, acct_type)| SageAccountMapping {
        billforge_gl_code: bf_code,
        sage_account_no: sage_no,
        account_title: title,
        account_type: acct_type,
    })
    .collect();

    Ok(Json(mappings))
}

/// Update GL account mappings
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/mappings/accounts",
    tag = "Sage Intacct",
    request_body = Vec<SageAccountMapping>,
    responses(
        (status = 200, description = "Mappings updated"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(mappings): Json<Vec<SageAccountMapping>>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    for mapping in mappings {
        sqlx::query(
            "UPDATE sage_intacct_account_mappings
             SET billforge_gl_code = $3, updated_at = NOW()
             WHERE tenant_id = $1 AND sage_account_no = $2",
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&mapping.sage_account_no)
        .bind(&mapping.billforge_gl_code)
        .execute(&*pool)
        .await
        .ok();
    }

    Ok(Json(serde_json::json!({ "status": "updated" })))
}

/// List available entities (for multi-entity companies)
#[utoipa::path(
    get,
    path = "/api/v1/sage-intacct/entities",
    tag = "Sage Intacct",
    responses(
        (status = 200, description = "Available entities"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_entities(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get stored credentials
    let connection: Option<(String, String, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT company_id, sender_id, sender_password, user_id, user_password, entity_id
         FROM sage_intacct_connections
         WHERE tenant_id = $1",
        )
        .bind(tenant.tenant_id.as_uuid())
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

    let (company_id, sender_id, sender_password, user_id, user_password, entity_id) = connection
        .ok_or_else(|| {
            billforge_core::Error::Validation("Sage Intacct not connected".to_string())
        })?;

    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id,
        sender_password,
        company_id,
        entity_id,
        user_id,
        user_password,
    });

    let session = auth.get_session().await.map_err(|e| {
        billforge_core::Error::Validation(format!("Sage Intacct session failed: {}", e))
    })?;

    let client = SageIntacctClient::new(session);

    let entities = client
        .list_entities()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct API error: {}", e)))?;

    Ok(Json(entities))
}

/// Request body for configuring a webhook secret
#[derive(Debug, Deserialize)]
struct ConfigureWebhookRequest {
    webhook_secret: String,
}

/// Configure the webhook secret for Sage Intacct integration.
async fn configure_sage_intacct_webhook(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(body): Json<ConfigureWebhookRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query("UPDATE sage_intacct_connections SET webhook_secret = $2 WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .bind(&body.webhook_secret)
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update webhook secret: {}", e))
        })?;

    Ok(Json(serde_json::json!({ "status": "configured" })))
}

/// Receive and verify Sage Intacct webhook notifications.
///
/// Sage Intacct signs webhooks with HMAC-SHA256 using a shared secret.
/// The signature is hex-encoded in the `x-intacct-signature` header.
/// The tenant_id is embedded in the webhook URL registered with Sage Intacct.
async fn sage_intacct_webhook(
    State(state): State<AppState>,
    axum::extract::Path(tenant_id_str): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let signature = headers
        .get("x-intacct-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let tenant_id: billforge_core::TenantId = tenant_id_str.parse().map_err(|_| {
        tracing::error!("Sage Intacct webhook invalid tenant_id in path");
        StatusCode::BAD_REQUEST
    })?;

    let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let webhook_secret: Option<String> = sqlx::query_scalar(
        "SELECT webhook_secret FROM sage_intacct_connections WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*tenant_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch webhook secret: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret = webhook_secret.filter(|s| !s.is_empty()).ok_or_else(|| {
        tracing::warn!("Sage Intacct webhook rejected: no webhook secret configured");
        StatusCode::UNAUTHORIZED
    })?;

    if !webhook::verify_webhook_signature(&body, signature, &secret) {
        tracing::warn!("Sage Intacct webhook signature verification failed");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let (event_type, nonce) = match serde_json::from_slice::<WebhookEnvelope>(&body) {
        Ok(envelope) => {
            if !webhook::validate_timestamp_freshness(envelope.timestamp, 300) {
                tracing::warn!(timestamp = %envelope.timestamp, "Sage Intacct webhook timestamp too old or in future");
                return Err(StatusCode::UNAUTHORIZED);
            }
            let nonce = envelope
                .nonce
                .unwrap_or_else(|| webhook::compute_payload_nonce(&body));
            (envelope.event_type, nonce)
        }
        Err(_) => (
            "provider_native".to_string(),
            webhook::compute_payload_nonce(&body),
        ),
    };

    if !webhook::check_replay_nonce(&*tenant_pool, "sage_intacct", *tenant_id.as_uuid(), &nonce)
        .await
        .map_err(|e| {
            tracing::error!("Replay nonce check failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        tracing::warn!(nonce = %nonce, "Sage Intacct webhook replay detected");
        return Err(StatusCode::CONFLICT);
    }

    tracing::info!(
        event_type = %event_type,
        tenant_id = %tenant_id_str,
        "Sage Intacct webhook received and verified"
    );

    Ok(StatusCode::OK)
}
