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
    extract::{Query, State},
    response::{IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use billforge_xero::{XeroClient, XeroOAuth, XeroOAuthConfig, XeroEnvironment};
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
    let xero_config = state.config.xero.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Xero integration not configured".to_string()))?;

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
    .ok();  // Ignore errors if table doesn't exist yet

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

    let tenant_id: billforge_core::TenantId = parts[0].parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid tenant ID in state token".to_string()))?;

    let pool = state.db.tenant(&tenant_id).await?;

    // Verify state token
    let stored_state: Option<(String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT state_token, expires_at FROM xero_oauth_states WHERE tenant_id = $1"
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

    // Get Xero configuration
    let xero_config = state.config.xero.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Xero integration not configured".to_string()))?;

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
    let tokens = oauth.exchange_code(&params.code).await
        .map_err(|e| billforge_core::Error::Validation(format!("OAuth token exchange failed: {}", e)))?;

    // Get tenant connections (organizations)
    let connections = oauth.get_connections(&tokens.access_token).await
        .map_err(|e| billforge_core::Error::Validation(format!("Failed to get Xero connections: {}", e)))?;

    // Use the first organization (tenant can only connect to one org at a time)
    let xero_tenant = connections.into_iter().next()
        .ok_or_else(|| billforge_core::Error::Validation("No Xero organizations found".to_string()))?;

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
            updated_at = NOW()"
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
    Ok(Redirect::temporary(&format!("{}/dashboard?xero=connected", state.config.frontend_url)))
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
    let connection: Option<(String,)> = sqlx::query_as(
        "SELECT refresh_token FROM xero_connections WHERE tenant_id = $1"
    )
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
    let connection: Option<(String, Option<String>, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT organization_name, xero_tenant_id, sync_enabled, last_sync_at
         FROM xero_connections
         WHERE tenant_id = $1"
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

/// Sync contacts from Xero
#[utoipa::path(
    post,
    path = "/api/v1/xero/sync/contacts",
    tag = "Xero",
    request_body = SyncContactsRequest,
    responses(
        (status = 200, description = "Contacts synced", body = SyncContactsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_contacts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(_request): Json<SyncContactsRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get Xero connection
    let connection: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT xero_tenant_id, access_token, access_token_expires_at FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (xero_tenant_id, access_token, token_expires_at) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Xero not connected or sync disabled".to_string()))?;

    // Check if token needs refresh
    if token_expires_at <= Utc::now() {
        return Err(billforge_core::Error::Validation("Xero token expired. Please reconnect.".to_string()).into());
    }

    let xero_config = state.config.xero.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Xero integration not configured".to_string()))?;

    let client = XeroClient::new(
        access_token,
        xero_tenant_id,
        match xero_config.environment {
            crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
            crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
        },
    );

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO xero_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'contacts', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    // Fetch contacts from Xero (paginate through all results)
    let mut all_contacts = Vec::new();
    let mut page = 1;
    let page_size = 100;

    loop {
        let contacts = client.query_contacts(page, page_size).await
            .map_err(|e| billforge_core::Error::Validation(format!("Xero API error: {}", e)))?;

        if contacts.is_empty() {
            break;
        }

        all_contacts.extend(contacts);
        page += 1;
    }

    // Filter to suppliers only
    let suppliers: Vec<_> = all_contacts
        .into_iter()
        .filter(|c| c.IsSupplier.unwrap_or(false))
        .collect();

    let mut imported = 0u64;
    let mut updated = 0u64;
    let skipped = 0u64;

    // Sync each contact
    for xero_contact in suppliers {
        // Check if contact already exists
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v
             INNER JOIN xero_contact_mappings m ON m.billforge_vendor_id = v.id
             WHERE m.tenant_id = $1 AND m.xero_contact_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&xero_contact.ContactID)
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, updated_at = NOW()
                 WHERE id = $1"
            )
            .bind(vendor_id)
            .bind(&xero_contact.Name)
            .bind(&xero_contact.EmailAddress)
            .execute(&*pool)
            .await
            .ok();

            // Update mapping
            sqlx::query(
                "UPDATE xero_contact_mappings
                 SET xero_contact_name = $3, last_synced_at = NOW(), updated_at = NOW()
                 WHERE tenant_id = $1 AND xero_contact_id = $2"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&xero_contact.ContactID)
            .bind(&xero_contact.Name)
            .execute(&*pool)
            .await
            .ok();

            updated += 1;
        } else {
            // Create new vendor
            let vendor_id = Uuid::new_v4();

            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, status, created_at, updated_at)
                 VALUES ($1, $2, 'business', $3, $4, NOW(), NOW())"
            )
            .bind(vendor_id)
            .bind(&xero_contact.Name)
            .bind(&xero_contact.EmailAddress)
            .bind(if xero_contact.ContactStatus == "ACTIVE" { "active" } else { "inactive" })
            .execute(&*pool)
            .await
            .ok();

            // Create mapping
            sqlx::query(
                "INSERT INTO xero_contact_mappings
                 (tenant_id, xero_contact_id, billforge_vendor_id, xero_contact_name, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&xero_contact.ContactID)
            .bind(vendor_id)
            .bind(&xero_contact.Name)
            .execute(&*pool)
            .await
            .ok();

            imported += 1;
        }
    }

    // Update sync log
    sqlx::query(
        "UPDATE xero_sync_log
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

    // Update last sync time on connection
    sqlx::query(
        "UPDATE xero_connections SET last_sync_at = NOW() WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    let response = SyncContactsResponse {
        imported,
        updated,
        skipped,
    };

    Ok(Json(response))
}

/// Sync accounts from Xero
#[utoipa::path(
    post,
    path = "/api/v1/xero/sync/accounts",
    tag = "Xero",
    responses(
        (status = 200, description = "Accounts synced"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_accounts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get Xero connection
    let connection: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT xero_tenant_id, access_token, access_token_expires_at FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (xero_tenant_id, access_token, token_expires_at) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Xero not connected or sync disabled".to_string()))?;

    // Check if token needs refresh
    if token_expires_at <= Utc::now() {
        return Err(billforge_core::Error::Validation("Xero token expired. Please reconnect.".to_string()).into());
    }

    let xero_config = state.config.xero.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Xero integration not configured".to_string()))?;

    let client = XeroClient::new(
        access_token,
        xero_tenant_id,
        match xero_config.environment {
            crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
            crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
        },
    );

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO xero_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'accounts', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    // Fetch accounts from Xero
    let mut all_accounts = Vec::new();
    let mut page = 1;
    let page_size = 100;

    loop {
        let accounts = client.query_accounts(page, page_size).await
            .map_err(|e| billforge_core::Error::Validation(format!("Xero API error: {}", e)))?;

        if accounts.is_empty() {
            break;
        }

        all_accounts.extend(accounts);
        page += 1;
    }

    // Filter to expense accounts only
    let expense_accounts: Vec<_> = all_accounts
        .into_iter()
        .filter(|a| a.Class == "EXPENSE" && a.Status == "ACTIVE")
        .collect();

    let mut created = 0u64;

    // Upsert account mappings
    for xero_account in expense_accounts {
        sqlx::query(
            "INSERT INTO xero_account_mappings
             (tenant_id, xero_account_id, xero_account_code, xero_account_name, xero_account_type, billforge_gl_code, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $3, NOW(), NOW())
             ON CONFLICT (tenant_id, xero_account_id) DO UPDATE SET
                xero_account_code = $3,
                xero_account_name = $4,
                xero_account_type = $5,
                updated_at = NOW()"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&xero_account.AccountID)
        .bind(&xero_account.Code)
        .bind(&xero_account.Name)
        .bind(&xero_account.AccountType)
        .execute(&*pool)
        .await
        .ok();

        created += 1;
    }

    // Update sync log
    sqlx::query(
        "UPDATE xero_sync_log
         SET status = 'completed', completed_at = NOW(), records_processed = $2, records_created = $2
         WHERE id = $1"
    )
    .bind(sync_id)
    .bind(created as i32)
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(serde_json::json!({ "status": "synced", "count": created })))
}

/// Export invoice to Xero
#[utoipa::path(
    post,
    path = "/api/v1/xero/export/invoice/{id}",
    tag = "Xero",
    request_body = ExportInvoiceRequest,
    responses(
        (status = 200, description = "Invoice exported", body = ExportInvoiceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Invoice not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn export_invoice_to_xero(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get Xero connection
    let connection: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT xero_tenant_id, access_token, access_token_expires_at FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (xero_tenant_id, access_token, token_expires_at) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Xero not connected or sync disabled".to_string()))?;

    // Check if token needs refresh
    if token_expires_at <= Utc::now() {
        return Err(billforge_core::Error::Validation("Xero token expired. Please reconnect.".to_string()).into());
    }

    // Get invoice from database
    let invoice_id: billforge_core::domain::InvoiceId = request.invoice_id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, due_date, po_number, currency
         FROM invoices WHERE id = $1"
    )
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (_vendor_name, invoice_number, total_cents, due_date, po_number, currency) = invoice
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: request.invoice_id.clone(),
        })?;

    // Get vendor mapping
    let vendor_mapping: Option<(String, String)> = sqlx::query_as(
        "SELECT xero_contact_id, xero_contact_name FROM xero_contact_mappings
         WHERE tenant_id = $1 AND billforge_vendor_id IN
         (SELECT vendor_id FROM invoices WHERE id = $2)"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (xero_contact_id, xero_contact_name) = vendor_mapping
        .ok_or_else(|| billforge_core::Error::Validation("Vendor not found in Xero. Please sync contacts first.".to_string()))?;

    let xero_config = state.config.xero.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("Xero integration not configured".to_string()))?;

    let client = XeroClient::new(
        access_token,
        xero_tenant_id,
        match xero_config.environment {
            crate::config::XeroEnvironment::Sandbox => XeroEnvironment::Sandbox,
            crate::config::XeroEnvironment::Production => XeroEnvironment::Production,
        },
    );

    // Build Xero invoice (bill)
    use billforge_xero::{XeroInvoice, XeroLineItem, XeroContact};

    let total_amount = total_cents as f64 / 100.0;

    let invoice = XeroInvoice {
        InvoiceID: None,
        InvoiceNumber: Some(invoice_number.clone()),
        Reference: po_number.clone(),
        Contact: XeroContact {
            ContactID: xero_contact_id.clone(),
            Name: xero_contact_name,
            ContactStatus: "ACTIVE".to_string(),
            EmailAddress: None,
            Phones: None,
            Addresses: None,
            IsSupplier: Some(true),
            IsCustomer: None,
            DefaultCurrency: None,
            UpdatedDateUTC: None,
        },
        InvoiceType: "ACCPAY".to_string(),  // Accounts payable (bill)
        Status: Some("DRAFT".to_string()),
        LineItems: vec![
            XeroLineItem {
                LineItemID: None,
                Description: Some(format!("Invoice {}", invoice_number)),
                Quantity: Some(1.0),
                UnitAmount: Some(total_amount),
                AccountCode: Some(request.xero_account_code.clone()),
                TaxType: None,
                TaxAmount: Some(0.0),
                LineAmount: Some(total_amount),
                Tracking: None,
            },
        ],
        Date: chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string(),
        DueDate: due_date.unwrap_or_else(|| chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()),
        CurrencyCode: currency,
        SubTotal: total_amount,
        TotalTax: 0.0,
        Total: total_amount,
        AmountDue: Some(total_amount),
        AmountPaid: Some(0.0),
        UpdatedDateUTC: None,
    };

    // Create invoice in Xero
    let created_invoice = client.create_invoice(&invoice).await
        .map_err(|e| billforge_core::Error::Validation(format!("Xero API error: {}", e)))?;

    let xero_invoice_id = created_invoice.InvoiceID.clone().unwrap_or_default();

    // Store export record
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO xero_invoice_exports (id, tenant_id, invoice_id, xero_invoice_id, exported_at, export_status)
         VALUES ($1, $2, $3, $4, NOW(), 'synced')
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET
            xero_invoice_id = $4,
            exported_at = NOW(),
            export_status = 'synced',
            sync_error = NULL"
    )
    .bind(export_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&xero_invoice_id)
    .execute(&*pool)
    .await
    .ok();

    let response = ExportInvoiceResponse {
        xero_invoice_id,
        status: "synced".to_string(),
    };

    Ok(Json(response))
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
             WHERE tenant_id = $1 AND xero_account_id = $2"
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
