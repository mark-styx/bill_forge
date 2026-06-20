//! Xero accounting integration endpoints
//!
//! Provides OAuth 2.0 flow and sync capabilities for Xero:
//! - OAuth connection/disconnection
//! - Contact sync (Xero → BillForge vendors)
//! - Invoice export (BillForge → Xero)
//! - Account/Category mapping
//! - Sync status tracking

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use billforge_core::webhook::{self, WebhookEnvelope};
use billforge_xero::{XeroClient, XeroEnvironment, XeroOAuth, XeroOAuthConfig};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // OAuth endpoints
        .route("/connect", get(xero_connect))
        .route("/callback", get(xero_callback))
        .route("/disconnect", post(xero_disconnect))
        .route("/status", get(xero_status))
        // Sync endpoints
        .route("/sync/contacts", post(sync_contacts))
        .route("/sync/accounts", post(sync_accounts))
        .route("/export/invoice/:id", post(export_invoice_to_xero))
        // Mapping endpoints
        .route("/mappings/accounts", get(get_account_mappings))
        .route("/mappings/accounts", post(update_account_mappings))
        // Webhook secret configuration (requires auth)
        .route("/webhook/configure", post(configure_xero_webhook))
        // Webhook (no auth - verified via HMAC signature; tenant_id in path)
        .route("/webhook/:tenant_id", post(xero_webhook))
}

/// Xero connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct XeroStatus {
    /// Whether Xero is connected
    pub connected: bool,
    /// Xero organization name
    pub organization_name: Option<String>,
    /// Xero tenant ID
    pub tenant_id: Option<String>,
    /// Last sync timestamp
    pub last_sync_at: Option<String>,
    /// Sync enabled
    pub sync_enabled: bool,
}

/// Sync contacts from Xero
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncContactsRequest {
    /// Force full sync (vs incremental)
    pub full_sync: bool,
}

/// Sync contacts response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncContactsResponse {
    /// Number of contacts imported
    pub imported: u64,
    /// Number of contacts updated
    pub updated: u64,
    /// Number of contacts skipped
    pub skipped: u64,
    /// Number of contacts that failed to sync due to database errors
    #[schema(example = 0)]
    pub failed: u64,
}

/// Account mapping
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AccountMapping {
    /// BillForge account/category ID
    pub billforge_account_id: String,
    /// Xero account ID
    pub xero_account_id: String,
    /// Account code
    pub account_code: String,
    /// Account name
    pub account_name: String,
    /// Account type
    pub account_type: String,
}

/// Export invoice to Xero
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceRequest {
    /// Invoice ID to export
    pub invoice_id: String,
    /// Xero account code to use
    pub xero_account_code: String,
}

/// Export invoice response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceResponse {
    /// Xero invoice ID
    pub xero_invoice_id: String,
    /// Sync status
    pub status: String,
}

/// Initiate Xero OAuth connection
#[utoipa::path(
    get,
    path = "/api/v1/xero/connect",
    tag = "Xero",
    responses(
        (status = 302, description = "Redirect to Xero OAuth"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn xero_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    // Check if Xero is configured
    let xero_config = state.config.xero.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("Xero integration not configured".to_string())
    })?;

    let oauth = XeroOAuth::new(XeroOAuthConfig {
        client_id: xero_config.client_id.clone(),
        client_secret: xero_config.client_secret.clone(),
        redirect_uri: xero_config.redirect_uri.clone(),
        environment: match xero_config.environment {
            crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
            crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
        },
    });

    // Generate state token with tenant ID for verification
    let state_token = format!("{}:{}", tenant.tenant_id.as_str(), Uuid::new_v4());

    // Store state token in database for verification (expires in 10 minutes)
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let expires_at = Utc::now() + Duration::minutes(10);
    sqlx::query(
        "INSERT INTO xero_oauth_states (tenant_id, state_token, expires_at, created_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (tenant_id) DO UPDATE SET state_token = $2, expires_at = $3, created_at = NOW()"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&state_token)
    .bind(expires_at)
    .execute(&*pool)
    .await
    .ok(); // Ignore errors if table doesn't exist yet

    let oauth_url = oauth.authorization_url(&state_token);
    Ok(Redirect::temporary(&oauth_url))
}

/// Handle Xero OAuth callback
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/xero/callback",
    tag = "Xero",
    params(
        ("code" = String, Query, description = "OAuth authorization code"),
        ("state" = String, Query, description = "State token for CSRF protection")
    ),
    responses(
        (status = 302, description = "Redirect to success page"),
        (status = 400, description = "Invalid OAuth response"),
        (status = 500, description = "Internal server error")
    )
)]
async fn xero_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> ApiResult<impl IntoResponse> {
    // Extract tenant ID from state token
    let parts: Vec<&str> = params.state.split(':').collect();
    if parts.len() != 2 {
        return Err(billforge_core::Error::Validation("Invalid state token".to_string()).into());
    }

    let tenant_id: billforge_core::TenantId = parts[0].parse().map_err(|_| {
        billforge_core::Error::Validation("Invalid tenant ID in state token".to_string())
    })?;

    let pool = state.db.tenant(&tenant_id).await?;

    // Verify state token
    let stored_state: Option<(String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT state_token, expires_at FROM xero_oauth_states WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let is_valid = stored_state
        .map(|(token, expires_at)| token == params.state && expires_at > Utc::now())
        .unwrap_or(false);

    if !is_valid {
        return Err(billforge_core::Error::Validation(
            "Invalid or expired state token".to_string(),
        )
        .into());
    }

    // Get Xero configuration
    let xero_config = state.config.xero.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("Xero integration not configured".to_string())
    })?;

    let oauth = XeroOAuth::new(XeroOAuthConfig {
        client_id: xero_config.client_id.clone(),
        client_secret: xero_config.client_secret.clone(),
        redirect_uri: xero_config.redirect_uri.clone(),
        environment: match xero_config.environment {
            crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
            crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
        },
    });

    // Exchange authorization code for tokens
    let tokens = oauth.exchange_code(&params.code).await.map_err(|e| {
        billforge_core::Error::Validation(format!("OAuth token exchange failed: {}", e))
    })?;

    // Get tenant connections (organizations)
    let connections = oauth
        .get_connections(&tokens.access_token)
        .await
        .map_err(|e| {
            billforge_core::Error::Validation(format!("Failed to get Xero connections: {}", e))
        })?;

    // Use the first organization (tenant can only connect to one org at a time)
    let xero_tenant = connections.into_iter().next().ok_or_else(|| {
        billforge_core::Error::Validation("No Xero organizations found".to_string())
    })?;

    // Calculate token expiry times
    let now = Utc::now();
    let access_token_expires_at = now + Duration::seconds(tokens.expires_in);

    // Store tokens in database
    sqlx::query(
        "INSERT INTO xero_connections (
            tenant_id, xero_tenant_id, organization_name, access_token, refresh_token,
            access_token_expires_at, refresh_token_expires_at, environment, sync_enabled,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            xero_tenant_id = $2,
            organization_name = $3,
            access_token = $4,
            refresh_token = $5,
            access_token_expires_at = $6,
            updated_at = NOW()",
    )
    .bind(tenant_id.as_uuid())
    .bind(&xero_tenant.tenant_id)
    .bind(&xero_tenant.tenant_name)
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(access_token_expires_at)
    .bind(match xero_config.environment {
        crate::config::XeroEnvironment::Sandbox => "sandbox",
        crate::config::XeroEnvironment::Production => "production",
    })
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to store Xero tokens: {}", e)))?;

    // Clean up state token
    sqlx::query("DELETE FROM xero_oauth_states WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    // Redirect to success page
    Ok(Redirect::temporary(&format!(
        "{}/dashboard?xero=connected",
        state.config.frontend_url
    )))
}

/// Disconnect Xero
#[utoipa::path(
    post,
    path = "/api/v1/xero/disconnect",
    tag = "Xero",
    responses(
        (status = 200, description = "Xero disconnected"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn xero_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get current connection
    let connection: Option<(String,)> =
        sqlx::query_as("SELECT refresh_token FROM xero_connections WHERE tenant_id = $1")
            .bind(tenant.tenant_id.as_uuid())
            .fetch_optional(&*pool)
            .await
            .ok()
            .flatten();

    if let Some((refresh_token,)) = connection {
        // Revoke token with Xero
        if let Some(xero_config) = &state.config.xero {
            let oauth = XeroOAuth::new(XeroOAuthConfig {
                client_id: xero_config.client_id.clone(),
                client_secret: xero_config.client_secret.clone(),
                redirect_uri: xero_config.redirect_uri.clone(),
                environment: match xero_config.environment {
                    crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
                    crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
                },
            });

            oauth.revoke_token(&refresh_token).await.ok();
        }
    }

    // Delete connection from database
    sqlx::query("DELETE FROM xero_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

/// Get Xero connection status
#[utoipa::path(
    get,
    path = "/api/v1/xero/status",
    tag = "Xero",
    responses(
        (status = 200, description = "Xero status", body = XeroStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn xero_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get connection status
    let connection: Option<(String, Option<String>, bool, Option<chrono::DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT organization_name, xero_tenant_id, sync_enabled, last_sync_at
         FROM xero_connections
         WHERE tenant_id = $1",
        )
        .bind(tenant.tenant_id.as_uuid())
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

    let status = if let Some((org_name, xero_tenant_id, sync_enabled, last_sync_at)) = connection {
        XeroStatus {
            connected: true,
            organization_name: Some(org_name),
            tenant_id: xero_tenant_id,
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        XeroStatus {
            connected: false,
            organization_name: None,
            tenant_id: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Sync contacts from Xero (asynchronous)
///
/// The sync is performed by the background worker. The response returns 202
/// Accepted with the enqueued job id; clients should poll xero_sync_log for
/// status. This replaces the previous inline pagination loop that violated
/// the sub-200ms API SLO and offered no retry on transient Xero failures.
#[utoipa::path(
    post,
    path = "/api/v1/xero/sync/contacts",
    tag = "Xero",
    request_body = SyncContactsRequest,
    responses(
        (status = 202, description = "Sync enqueued"),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Background queue unavailable")
    )
)]
async fn sync_contacts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<SyncContactsRequest>,
) -> ApiResult<impl IntoResponse> {
    // Hand the work to the worker so the API request returns inside the
    // sub-200ms SLO and the sync gets the same retry/backoff as QuickBooks.
    let payload = serde_json::json!({ "full_sync": request.full_sync });
    crate::routes::erp_jobs::enqueue_erp_job(
        state.redis.as_ref(),
        crate::routes::erp_jobs::job_type::XERO_CONTACT_SYNC,
        &tenant.tenant_id,
        payload,
    )
    .await
}

/// Sync accounts from Xero (asynchronous)
#[utoipa::path(
    post,
    path = "/api/v1/xero/sync/accounts",
    tag = "Xero",
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
        crate::routes::erp_jobs::job_type::XERO_ACCOUNT_SYNC,
        &tenant.tenant_id,
        serde_json::json!({}),
    )
    .await
}

/// Export invoice to Xero (asynchronous)
#[utoipa::path(
    post,
    path = "/api/v1/xero/export/invoice/{id}",
    tag = "Xero",
    request_body = ExportInvoiceRequest,
    responses(
        (status = 202, description = "Export enqueued"),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Background queue unavailable")
    )
)]
async fn export_invoice_to_xero(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let payload = serde_json::json!({
        "invoice_id": request.invoice_id,
        "xero_account_code": request.xero_account_code,
    });
    crate::routes::erp_jobs::enqueue_erp_job(
        state.redis.as_ref(),
        crate::routes::erp_jobs::job_type::XERO_INVOICE_EXPORT,
        &tenant.tenant_id,
        payload,
    )
    .await
}

/// Get account mappings
#[utoipa::path(
    get,
    path = "/api/v1/xero/mappings/accounts",
    tag = "Xero",
    responses(
        (status = 200, description = "Account mappings", body = Vec<AccountMapping>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let mappings: Vec<AccountMapping> = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT xero_account_id, xero_account_code, xero_account_name, xero_account_type, billforge_gl_code
         FROM xero_account_mappings
         WHERE tenant_id = $1
         ORDER BY xero_account_name"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get account mappings: {}", e)))?
    .into_iter()
    .map(|(xero_id, code, name, acct_type, bf_id)| AccountMapping {
        billforge_account_id: bf_id,
        xero_account_id: xero_id,
        account_code: code,
        account_name: name,
        account_type: acct_type,
    })
    .collect();

    Ok(Json(mappings))
}

/// Update account mappings
#[utoipa::path(
    post,
    path = "/api/v1/xero/mappings/accounts",
    tag = "Xero",
    request_body = Vec<AccountMapping>,
    responses(
        (status = 200, description = "Mappings updated"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(mappings): Json<Vec<AccountMapping>>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    for mapping in mappings {
        sqlx::query(
            "UPDATE xero_account_mappings
             SET billforge_gl_code = $3, updated_at = NOW()
             WHERE tenant_id = $1 AND xero_account_id = $2",
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&mapping.xero_account_id)
        .bind(&mapping.billforge_account_id)
        .execute(&*pool)
        .await
        .ok();
    }

    Ok(Json(serde_json::json!({ "status": "updated" })))
}

/// Request body for configuring a webhook secret
#[derive(Debug, Deserialize)]
struct ConfigureWebhookRequest {
    webhook_secret: String,
}

/// Configure the webhook secret for Xero integration.
async fn configure_xero_webhook(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(body): Json<ConfigureWebhookRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query("UPDATE xero_connections SET webhook_secret = $2 WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .bind(&body.webhook_secret)
        .execute(&*pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to update webhook secret: {}", e))
        })?;

    Ok(Json(serde_json::json!({ "status": "configured" })))
}

/// Receive and verify Xero webhook notifications.
///
/// Xero signs webhooks with HMAC-SHA256 using a webhook key.
/// The signature is base64-encoded in the `x-xero-signature` header.
/// The tenant_id is embedded in the webhook URL registered with Xero.
///
/// Xero sends an "Intent to Receive" validation request before delivering
/// real events. If the signature is valid, we respond with 200; if invalid,
/// we respond with 401. Xero uses this to confirm we own the webhook key.
async fn xero_webhook(
    State(state): State<AppState>,
    axum::extract::Path(tenant_id_str): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let signature = headers
        .get("x-xero-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let tenant_id: billforge_core::TenantId = tenant_id_str.parse().map_err(|_| {
        tracing::error!("Xero webhook invalid tenant_id in path");
        StatusCode::BAD_REQUEST
    })?;

    let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let webhook_secret: Option<String> =
        sqlx::query_scalar("SELECT webhook_secret FROM xero_connections WHERE tenant_id = $1")
            .bind(tenant_id.as_uuid())
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch webhook secret: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let secret = webhook_secret.filter(|s| !s.is_empty()).ok_or_else(|| {
        tracing::warn!("Xero webhook rejected: no webhook secret configured");
        StatusCode::UNAUTHORIZED
    })?;

    if !webhook::verify_webhook_signature(&body, signature, &secret) {
        tracing::warn!("Xero webhook signature verification failed");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Parse envelope if possible; Xero validation challenges and provider-native
    // payloads may not match our format. Signature verification above is sufficient
    // for Xero's "Intent to Receive" validation.
    let (event_type, nonce) = match serde_json::from_slice::<WebhookEnvelope>(&body) {
        Ok(envelope) => {
            if !webhook::validate_timestamp_freshness(envelope.timestamp, 300) {
                tracing::warn!(timestamp = %envelope.timestamp, "Xero webhook timestamp too old or in future");
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

    if !webhook::check_replay_nonce(&*tenant_pool, "xero", *tenant_id.as_uuid(), &nonce)
        .await
        .map_err(|e| {
            tracing::error!("Replay nonce check failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        tracing::warn!(nonce = %nonce, "Xero webhook replay detected");
        return Err(StatusCode::CONFLICT);
    }

    tracing::info!(
        event_type = %event_type,
        tenant_id = %tenant_id_str,
        "Xero webhook received and verified"
    );

    Ok(StatusCode::OK)
}

/// Fetch the Xero connection for a tenant, automatically refreshing the access token
/// if it is expired or near-expiry (within 5 minutes). Returns a ready-to-use `XeroClient`
/// and the Xero tenant ID.
async fn get_authenticated_xero_client(
    state: &AppState,
    tenant_id: &billforge_core::TenantId,
) -> ApiResult<(XeroClient, String)> {
    let pool = state.db.tenant(tenant_id).await?;

    // Fetch connection including refresh_token
    let connection: Option<(String, String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT xero_tenant_id, access_token, refresh_token, access_token_expires_at \
         FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to fetch Xero connection for token refresh");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let (xero_tenant_id, mut access_token, refresh_token_val, token_expires_at) = connection
        .ok_or_else(|| {
            billforge_core::Error::Validation("Xero not connected or sync disabled".to_string())
        })?;

    // Refresh if token is expired or will expire within 5 minutes
    if token_expires_at <= Utc::now() + Duration::minutes(5) {
        let xero_config = state.config.xero.as_ref().ok_or_else(|| {
            billforge_core::Error::Validation("Xero integration not configured".to_string())
        })?;

        let oauth = XeroOAuth::new(XeroOAuthConfig {
            client_id: xero_config.client_id.clone(),
            client_secret: xero_config.client_secret.clone(),
            redirect_uri: xero_config.redirect_uri.clone(),
            environment: match xero_config.environment {
                crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
                crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
            },
        });

        let new_tokens = oauth.refresh_token(&refresh_token_val).await.map_err(|e| {
            tracing::warn!(error = %e, "Xero token refresh failed");
            billforge_core::Error::Validation("Xero token expired. Please reconnect.".to_string())
        })?;

        // Persist refreshed tokens
        let now = Utc::now();
        let new_access_expires = now + Duration::seconds(new_tokens.expires_in);
        sqlx::query(
            "UPDATE xero_connections \
             SET access_token = $2, refresh_token = $3, \
                 access_token_expires_at = $4, updated_at = NOW() \
             WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&new_tokens.access_token)
        .bind(&new_tokens.refresh_token)
        .bind(new_access_expires)
        .execute(&*pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to persist refreshed Xero tokens");
            billforge_core::Error::Database(format!("Failed to update tokens: {}", e))
        })?;

        access_token = new_tokens.access_token;
    }

    let xero_config = state.config.xero.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("Xero integration not configured".to_string())
    })?;

    let client = XeroClient::new(
        access_token,
        xero_tenant_id.clone(),
        match xero_config.environment {
            crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
            crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
        },
    );

    Ok((client, xero_tenant_id))
}
