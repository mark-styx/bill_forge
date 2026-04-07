//! Salesforce CRM integration endpoints
//!
//! Salesforce uses OAuth 2.0 (Web Server flow) like QuickBooks.
//! The integration provides:
//! - Account sync (Salesforce Accounts → BillForge Vendors)
//! - Contact sync (vendor contact enrichment)
//! - PO number linkage via Opportunities
//!
//! Endpoints:
//! - GET /connect — Initiate Salesforce OAuth flow
//! - GET /callback — Handle OAuth callback
//! - POST /disconnect — Revoke tokens & disconnect
//! - GET /status — Check connection status
//! - POST /sync/accounts — Sync Salesforce Accounts as vendors
//! - POST /sync/contacts — Sync vendor contacts
//! - GET /mappings/accounts — Get account-to-vendor mappings
//! - POST /mappings/accounts — Update mappings

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
use billforge_salesforce::{SalesforceClient, SalesforceOAuth, SalesforceOAuthConfig, SalesforceEnvironment};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // OAuth endpoints
        .route("/connect", get(salesforce_connect))
        .route("/callback", get(salesforce_callback))
        .route("/disconnect", post(salesforce_disconnect))
        .route("/status", get(salesforce_status))
        // Sync endpoints
        .route("/sync/accounts", post(sync_accounts))
        .route("/sync/contacts", post(sync_contacts))
        // Mapping endpoints
        .route("/mappings/accounts", get(get_account_mappings))
        .route("/mappings/accounts", post(update_account_mappings))
        // Webhook secret configuration (requires auth)
        .route("/webhook/configure", post(configure_salesforce_webhook))
        // Webhook (no auth - verified via HMAC signature; tenant_id in path)
        .route("/webhook/:tenant_id", post(salesforce_webhook))
}

// ──────────────────────────── Types ────────────────────────────

/// Salesforce connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SalesforceStatus {
    /// Whether Salesforce is connected
    pub connected: bool,
    /// Salesforce instance URL
    pub instance_url: Option<String>,
    /// Salesforce org name
    pub org_name: Option<String>,
    /// Last sync timestamp
    pub last_sync_at: Option<String>,
    /// Sync enabled
    pub sync_enabled: bool,
}

/// Sync accounts request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncAccountsRequest {
    /// Force full sync (vs incremental)
    pub full_sync: bool,
    /// Custom SOQL WHERE filter (optional)
    pub custom_filter: Option<String>,
}

/// Sync accounts response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncAccountsResponse {
    /// Number of accounts imported
    pub imported: u64,
    /// Number of accounts updated
    pub updated: u64,
    /// Number of accounts skipped
    pub skipped: u64,
}

/// Account-to-vendor mapping
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SalesforceAccountMapping {
    /// BillForge vendor ID
    pub billforge_vendor_id: String,
    /// Salesforce Account ID
    pub salesforce_account_id: String,
    /// Account name
    pub account_name: String,
    /// Account type in Salesforce
    pub account_type: Option<String>,
}

// ──────────────────────────── Handlers ────────────────────────────

/// Initiate Salesforce OAuth connection
#[utoipa::path(
    get,
    path = "/api/v1/salesforce/connect",
    tag = "Salesforce",
    responses(
        (status = 302, description = "Redirect to Salesforce OAuth"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn salesforce_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let sf_config = state.config.salesforce.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Salesforce integration not configured".to_string()))?;

    let oauth = SalesforceOAuth::new(SalesforceOAuthConfig {
        client_id: sf_config.client_id.clone(),
        client_secret: sf_config.client_secret.clone(),
        redirect_uri: sf_config.redirect_uri.clone(),
        environment: match sf_config.environment {
            crate::config::SalesforceEnvironment::Sandbox => SalesforceEnvironment::Sandbox,
            crate::config::SalesforceEnvironment::Production => SalesforceEnvironment::Production,
        },
    });

    // Generate state token with tenant ID
    let state_token = format!("{}:{}", tenant.tenant_id.as_str(), Uuid::new_v4());

    // Store state token for verification
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let expires_at = Utc::now() + Duration::minutes(10);
    sqlx::query(
        "INSERT INTO salesforce_oauth_states (tenant_id, state_token, expires_at, created_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (tenant_id) DO UPDATE SET state_token = $2, expires_at = $3, created_at = NOW()"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&state_token)
    .bind(expires_at)
    .execute(&*pool)
    .await
    .ok();

    let oauth_url = oauth.authorization_url(&state_token);
    Ok(Redirect::temporary(&oauth_url))
}

/// Handle Salesforce OAuth callback
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/salesforce/callback",
    tag = "Salesforce",
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
async fn salesforce_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> ApiResult<impl IntoResponse> {
    // Extract tenant ID from state token
    let parts: Vec<&str> = params.state.split(':').collect();
    if parts.len() != 2 {
        return Err(billforge_core::Error::Validation("Invalid state token".to_string()).into());
    }

    let tenant_id: billforge_core::TenantId = parts[0].parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid tenant ID in state token".to_string()))?;

    let pool = state.db.tenant(&tenant_id).await?;

    // Verify state token
    let stored_state: Option<(String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT state_token, expires_at FROM salesforce_oauth_states WHERE tenant_id = $1"
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
        return Err(billforge_core::Error::Validation("Invalid or expired state token".to_string()).into());
    }

    // Get Salesforce configuration
    let sf_config = state.config.salesforce.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Salesforce integration not configured".to_string()))?;

    let oauth = SalesforceOAuth::new(SalesforceOAuthConfig {
        client_id: sf_config.client_id.clone(),
        client_secret: sf_config.client_secret.clone(),
        redirect_uri: sf_config.redirect_uri.clone(),
        environment: match sf_config.environment {
            crate::config::SalesforceEnvironment::Sandbox => SalesforceEnvironment::Sandbox,
            crate::config::SalesforceEnvironment::Production => SalesforceEnvironment::Production,
        },
    });

    // Exchange authorization code for tokens
    let tokens = oauth.exchange_code(&params.code).await
        .map_err(|e| billforge_core::Error::Validation(format!("OAuth token exchange failed: {}", e)))?;

    // Salesforce access tokens last ~2 hours
    let now = Utc::now();
    let access_token_expires_at = now + Duration::hours(2);

    // Get user info from Salesforce
    let client = SalesforceClient::new(
        tokens.access_token.clone(),
        tokens.instance_url.clone(),
    );

    let user_info = client.get_user_info().await.ok();
    let org_name = user_info
        .and_then(|info| info.get("organization_id").and_then(|v| v.as_str()).map(|s| s.to_string()));

    // Store tokens in database
    sqlx::query(
        "INSERT INTO salesforce_connections (
            tenant_id, instance_url, access_token, refresh_token,
            access_token_expires_at, org_id, sync_enabled,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            instance_url = $2,
            access_token = $3,
            refresh_token = COALESCE($4, salesforce_connections.refresh_token),
            access_token_expires_at = $5,
            org_id = COALESCE($6, salesforce_connections.org_id),
            updated_at = NOW()"
    )
    .bind(tenant_id.as_uuid())
    .bind(&tokens.instance_url)
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(access_token_expires_at)
    .bind(&org_name)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to store Salesforce tokens: {}", e)))?;

    // Clean up state token
    sqlx::query("DELETE FROM salesforce_oauth_states WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Redirect::temporary(&format!("{}/dashboard?salesforce=connected", state.config.frontend_url)))
}

/// Disconnect Salesforce
#[utoipa::path(
    post,
    path = "/api/v1/salesforce/disconnect",
    tag = "Salesforce",
    responses(
        (status = 200, description = "Salesforce disconnected"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn salesforce_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get current connection
    let connection: Option<(String, String)> = sqlx::query_as(
        "SELECT access_token, instance_url FROM salesforce_connections WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    if let Some((access_token, _instance_url)) = connection {
        // Revoke token with Salesforce
        if let Some(sf_config) = &state.config.salesforce {
            let oauth = SalesforceOAuth::new(SalesforceOAuthConfig {
                client_id: sf_config.client_id.clone(),
                client_secret: sf_config.client_secret.clone(),
                redirect_uri: sf_config.redirect_uri.clone(),
                environment: match sf_config.environment {
                    crate::config::SalesforceEnvironment::Sandbox => SalesforceEnvironment::Sandbox,
                    crate::config::SalesforceEnvironment::Production => SalesforceEnvironment::Production,
                },
            });

            oauth.revoke_token(&access_token).await.ok();
        }
    }

    // Delete connection from database
    sqlx::query("DELETE FROM salesforce_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

/// Get Salesforce connection status
#[utoipa::path(
    get,
    path = "/api/v1/salesforce/status",
    tag = "Salesforce",
    responses(
        (status = 200, description = "Salesforce status", body = SalesforceStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn salesforce_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let connection: Option<(String, Option<String>, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT instance_url, org_id, sync_enabled, last_sync_at
         FROM salesforce_connections
         WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let status = if let Some((instance_url, org_id, sync_enabled, last_sync_at)) = connection {
        SalesforceStatus {
            connected: true,
            instance_url: Some(instance_url),
            org_name: org_id,
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        SalesforceStatus {
            connected: false,
            instance_url: None,
            org_name: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Sync Salesforce Accounts as BillForge vendors
#[utoipa::path(
    post,
    path = "/api/v1/salesforce/sync/accounts",
    tag = "Salesforce",
    request_body = SyncAccountsRequest,
    responses(
        (status = 200, description = "Accounts synced", body = SyncAccountsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_accounts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<SyncAccountsRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get Salesforce connection
    let connection: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT instance_url, access_token, access_token_expires_at
         FROM salesforce_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (instance_url, access_token, token_expires_at) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Salesforce not connected or sync disabled".to_string()))?;

    // Check if token needs refresh
    if token_expires_at <= Utc::now() {
        // Attempt token refresh
        let refresh_result: Option<(String,)> = sqlx::query_as(
            "SELECT refresh_token FROM salesforce_connections WHERE tenant_id = $1"
        )
        .bind(tenant.tenant_id.as_uuid())
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        if let Some((refresh_token,)) = refresh_result {
            if let Some(sf_config) = &state.config.salesforce {
                let oauth = SalesforceOAuth::new(SalesforceOAuthConfig {
                    client_id: sf_config.client_id.clone(),
                    client_secret: sf_config.client_secret.clone(),
                    redirect_uri: sf_config.redirect_uri.clone(),
                    environment: match sf_config.environment {
                        crate::config::SalesforceEnvironment::Sandbox => SalesforceEnvironment::Sandbox,
                        crate::config::SalesforceEnvironment::Production => SalesforceEnvironment::Production,
                    },
                });

                match oauth.refresh_token(&refresh_token).await {
                    Ok(new_tokens) => {
                        let new_expires = Utc::now() + Duration::hours(2);
                        sqlx::query(
                            "UPDATE salesforce_connections SET access_token = $2, access_token_expires_at = $3, updated_at = NOW() WHERE tenant_id = $1"
                        )
                        .bind(tenant.tenant_id.as_uuid())
                        .bind(&new_tokens.access_token)
                        .bind(new_expires)
                        .execute(&*pool)
                        .await
                        .ok();
                    }
                    Err(_) => {
                        return Err(billforge_core::Error::Validation("Salesforce token expired. Please reconnect.".to_string()).into());
                    }
                }
            }
        } else {
            return Err(billforge_core::Error::Validation("Salesforce token expired. Please reconnect.".to_string()).into());
        }
    }

    let client = SalesforceClient::new(access_token, instance_url);

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO salesforce_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'accounts', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    // Fetch accounts from Salesforce
    let accounts = client.query_vendor_accounts(request.custom_filter.as_deref()).await
        .map_err(|e| billforge_core::Error::Validation(format!("Salesforce API error: {}", e)))?;

    let mut imported = 0u64;
    let mut updated = 0u64;
    let skipped = 0u64;

    // Sync each account as a vendor
    for sf_account in &accounts {
        // Check if mapping exists
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v
             INNER JOIN salesforce_account_mappings m ON m.billforge_vendor_id = v.id
             WHERE m.tenant_id = $1 AND m.salesforce_account_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&sf_account.id)
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        let phone = sf_account.phone.as_deref().unwrap_or("");

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            sqlx::query(
                "UPDATE vendors SET name = $2, phone = $3, updated_at = NOW() WHERE id = $1"
            )
            .bind(vendor_id)
            .bind(&sf_account.name)
            .bind(phone)
            .execute(&*pool)
            .await
            .ok();

            updated += 1;
        } else {
            // Create new vendor
            let vendor_id = Uuid::new_v4();
            let vendor_type = sf_account.account_type.as_deref().unwrap_or("business");

            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, phone, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, 'active', NOW(), NOW())"
            )
            .bind(vendor_id)
            .bind(&sf_account.name)
            .bind(vendor_type)
            .bind(phone)
            .execute(&*pool)
            .await
            .ok();

            // Create mapping
            sqlx::query(
                "INSERT INTO salesforce_account_mappings
                 (tenant_id, salesforce_account_id, billforge_vendor_id, salesforce_account_name, salesforce_account_type, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&sf_account.id)
            .bind(vendor_id)
            .bind(&sf_account.name)
            .bind(&sf_account.account_type)
            .execute(&*pool)
            .await
            .ok();

            imported += 1;
        }
    }

    // Update sync log
    sqlx::query(
        "UPDATE salesforce_sync_log
         SET status = 'completed', completed_at = NOW(), records_processed = $2, records_created = $3, records_updated = $4
         WHERE id = $1"
    )
    .bind(sync_id)
    .bind((imported + updated + skipped) as i32)
    .bind(imported as i32)
    .bind(updated as i32)
    .execute(&*pool)
    .await
    .ok();

    // Update last sync time
    sqlx::query("UPDATE salesforce_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(SyncAccountsResponse { imported, updated, skipped }))
}

/// Sync vendor contacts from Salesforce
#[utoipa::path(
    post,
    path = "/api/v1/salesforce/sync/contacts",
    tag = "Salesforce",
    responses(
        (status = 200, description = "Contacts synced"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_contacts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get Salesforce connection
    let connection: Option<(String, String)> = sqlx::query_as(
        "SELECT instance_url, access_token
         FROM salesforce_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (instance_url, access_token) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Salesforce not connected or sync disabled".to_string()))?;

    let client = SalesforceClient::new(access_token, instance_url);

    // Fetch vendor contacts
    let contacts = client.query_vendor_contacts().await
        .map_err(|e| billforge_core::Error::Validation(format!("Salesforce API error: {}", e)))?;

    let mut synced = 0u64;

    for contact in &contacts {
        if let Some(account_id) = &contact.account_id {
            // Find vendor mapping
            let vendor_mapping: Option<(Uuid,)> = sqlx::query_as(
                "SELECT billforge_vendor_id FROM salesforce_account_mappings
                 WHERE tenant_id = $1 AND salesforce_account_id = $2"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(account_id)
            .fetch_optional(&*pool)
            .await
            .ok()
            .flatten();

            if let Some((vendor_id,)) = vendor_mapping {
                let full_name = format!(
                    "{} {}",
                    contact.first_name.as_deref().unwrap_or(""),
                    contact.last_name
                ).trim().to_string();

                // Upsert contact info on vendor
                if let Some(email) = &contact.email {
                    sqlx::query(
                        "UPDATE vendors SET email = $2, contact_name = $3, updated_at = NOW() WHERE id = $1 AND (email IS NULL OR email = '')"
                    )
                    .bind(vendor_id)
                    .bind(email)
                    .bind(&full_name)
                    .execute(&*pool)
                    .await
                    .ok();
                }

                synced += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({ "status": "synced", "contacts_processed": synced })))
}

/// Get account-to-vendor mappings
#[utoipa::path(
    get,
    path = "/api/v1/salesforce/mappings/accounts",
    tag = "Salesforce",
    responses(
        (status = 200, description = "Account mappings", body = Vec<SalesforceAccountMapping>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let mappings: Vec<SalesforceAccountMapping> = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT salesforce_account_id, billforge_vendor_id::text, salesforce_account_name, salesforce_account_type
         FROM salesforce_account_mappings
         WHERE tenant_id = $1
         ORDER BY salesforce_account_name"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get account mappings: {}", e)))?
    .into_iter()
    .map(|(sf_id, bf_id, name, acct_type)| SalesforceAccountMapping {
        billforge_vendor_id: bf_id,
        salesforce_account_id: sf_id,
        account_name: name,
        account_type: acct_type,
    })
    .collect();

    Ok(Json(mappings))
}

/// Update account-to-vendor mappings
#[utoipa::path(
    post,
    path = "/api/v1/salesforce/mappings/accounts",
    tag = "Salesforce",
    request_body = Vec<SalesforceAccountMapping>,
    responses(
        (status = 200, description = "Mappings updated"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(mappings): Json<Vec<SalesforceAccountMapping>>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    for mapping in mappings {
        sqlx::query(
            "UPDATE salesforce_account_mappings
             SET billforge_vendor_id = $3::uuid, updated_at = NOW()
             WHERE tenant_id = $1 AND salesforce_account_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&mapping.salesforce_account_id)
        .bind(&mapping.billforge_vendor_id)
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

/// Configure the webhook secret for Salesforce integration.
async fn configure_salesforce_webhook(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(body): Json<ConfigureWebhookRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query(
        "UPDATE salesforce_connections SET webhook_secret = $2 WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&body.webhook_secret)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to update webhook secret: {}", e)))?;

    Ok(Json(serde_json::json!({ "status": "configured" })))
}

/// Receive and verify Salesforce webhook notifications.
///
/// Salesforce signs webhooks with HMAC-SHA256 using a shared secret.
/// The signature is hex-encoded in the `x-salesforce-signature` header.
/// The tenant_id is embedded in the webhook URL registered with Salesforce.
async fn salesforce_webhook(
    State(state): State<AppState>,
    axum::extract::Path(tenant_id_str): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let signature = headers
        .get("x-salesforce-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let tenant_id: billforge_core::TenantId = tenant_id_str.parse().map_err(|_| {
        tracing::error!("Salesforce webhook invalid tenant_id in path");
        StatusCode::BAD_REQUEST
    })?;

    let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let webhook_secret: Option<String> = sqlx::query_scalar(
        "SELECT webhook_secret FROM salesforce_connections WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*tenant_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch webhook secret: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret = webhook_secret
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Salesforce webhook rejected: no webhook secret configured");
            StatusCode::UNAUTHORIZED
        })?;

    if !webhook::verify_webhook_signature(&body, signature, &secret) {
        tracing::warn!("Salesforce webhook signature verification failed");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let (event_type, nonce) = match serde_json::from_slice::<WebhookEnvelope>(&body) {
        Ok(envelope) => {
            if !webhook::validate_timestamp_freshness(envelope.timestamp, 300) {
                tracing::warn!(timestamp = %envelope.timestamp, "Salesforce webhook timestamp too old or in future");
                return Err(StatusCode::UNAUTHORIZED);
            }
            let nonce = envelope.nonce.unwrap_or_else(|| webhook::compute_payload_nonce(&body));
            (envelope.event_type, nonce)
        }
        Err(_) => ("provider_native".to_string(), webhook::compute_payload_nonce(&body)),
    };

    if !webhook::check_replay_nonce(&*tenant_pool, "salesforce", *tenant_id.as_uuid(), &nonce)
        .await
        .map_err(|e| {
            tracing::error!("Replay nonce check failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        tracing::warn!(nonce = %nonce, "Salesforce webhook replay detected");
        return Err(StatusCode::CONFLICT);
    }

    tracing::info!(
        event_type = %event_type,
        tenant_id = %tenant_id_str,
        "Salesforce webhook received and verified"
    );

    Ok(StatusCode::OK)
}
