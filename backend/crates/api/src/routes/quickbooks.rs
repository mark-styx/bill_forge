//! QuickBooks Online integration endpoints
//!
//! Provides OAuth 2.0 flow and sync capabilities for QuickBooks Online:
//! - OAuth connection/disconnection
//! - Vendor sync (QuickBooks → BillForge)
//! - Invoice export (BillForge → QuickBooks)
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
use billforge_core::TenantId;
use billforge_quickbooks::{QuickBooksClient, QuickBooksOAuth, QuickBooksOAuthConfig, QuickBooksEnvironment};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // OAuth endpoints
        .route("/connect", get(quickbooks_connect))
        .route("/callback", get(quickbooks_callback))
        .route("/disconnect", post(quickbooks_disconnect))
        .route("/status", get(quickbooks_status))
        // Sync endpoints
        .route("/sync/vendors", post(sync_vendors))
        .route("/sync/accounts", post(sync_accounts))
        .route("/export/invoice/:id", post(export_invoice_to_quickbooks))
        // Mapping endpoints
        .route("/mappings/accounts", get(get_account_mappings))
        .route("/mappings/accounts", post(update_account_mappings))
}

/// QuickBooks connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QuickBooksStatus {
    /// Whether QuickBooks is connected
    pub connected: bool,
    /// Company ID
    pub company_id: Option<String>,
    /// Company name
    pub company_name: Option<String>,
    /// Last sync timestamp
    pub last_sync_at: Option<String>,
    /// Sync enabled
    pub sync_enabled: bool,
}

/// Sync vendors from QuickBooks
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncVendorsRequest {
    /// Force full sync (vs incremental)
    pub full_sync: bool,
}

/// Sync vendors response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncVendorsResponse {
    /// Number of vendors imported
    pub imported: u64,
    /// Number of vendors updated
    pub updated: u64,
    /// Number of vendors skipped
    pub skipped: u64,
    /// Number of vendors that failed to sync
    pub errors: u64,
}

/// Account mapping
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AccountMapping {
    /// BillForge account/category ID
    pub billforge_account_id: String,
    /// QuickBooks account ID
    pub quickbooks_account_id: String,
    /// Account name
    pub account_name: String,
    /// Account type
    pub account_type: String,
}

/// Export invoice to QuickBooks
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceRequest {
    /// Invoice ID to export
    pub invoice_id: String,
    /// QuickBooks account to use
    pub quickbooks_account_id: String,
}

/// Export invoice response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceResponse {
    /// QuickBooks invoice ID
    pub quickbooks_invoice_id: String,
    /// Sync status
    pub status: String,
}

/// Initiate QuickBooks OAuth connection
#[utoipa::path(
    get,
    path = "/api/v1/quickbooks/connect",
    tag = "QuickBooks",
    responses(
        (status = 302, description = "Redirect to QuickBooks OAuth"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn quickbooks_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    // Check if QuickBooks is configured
    let qb_config = state.config.quickbooks.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("QuickBooks integration not configured".to_string()))?;

    let oauth = QuickBooksOAuth::new(QuickBooksOAuthConfig {
        client_id: qb_config.client_id.clone(),
        client_secret: qb_config.client_secret.clone(),
        redirect_uri: qb_config.redirect_uri.clone(),
        environment: match qb_config.environment {
            crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
            crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
        },
    });

    // Generate state token with tenant ID for verification
    let state_token = format!("{}:{}", tenant.tenant_id.as_str(), Uuid::new_v4());

    // Store state token in database for verification (expires in 10 minutes)
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let expires_at = Utc::now() + Duration::minutes(10);
    sqlx::query(
        "INSERT INTO quickbooks_oauth_states (tenant_id, state_token, expires_at, created_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (tenant_id) DO UPDATE SET state_token = $2, expires_at = $3, created_at = NOW()"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&state_token)
    .bind(expires_at)
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to store OAuth state for tenant {}", tenant.tenant_id.as_str());
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let oauth_url = oauth.authorization_url(&state_token);
    Ok(Redirect::temporary(&oauth_url))
}

/// Handle QuickBooks OAuth callback
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
    #[serde(rename = "realmId")]
    pub realm_id: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/quickbooks/callback",
    tag = "QuickBooks",
    params(
        ("code" = String, Query, description = "OAuth authorization code"),
        ("state" = String, Query, description = "State token for CSRF protection"),
        ("realmId" = String, Query, description = "QuickBooks company ID")
    ),
    responses(
        (status = 302, description = "Redirect to success page"),
        (status = 400, description = "Invalid OAuth response"),
        (status = 500, description = "Internal server error")
    )
)]
async fn quickbooks_callback(
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
        "SELECT state_token, expires_at FROM quickbooks_oauth_states WHERE tenant_id = $1"
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch OAuth state for tenant {}", tenant_id.as_str());
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let is_valid = stored_state
        .map(|(token, expires_at)| token == params.state && expires_at > Utc::now())
        .unwrap_or(false);

    if !is_valid {
        return Err(billforge_core::Error::Validation("Invalid or expired state token".to_string()).into());
    }

    // Get QuickBooks configuration
    let qb_config = state.config.quickbooks.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation("QuickBooks integration not configured".to_string()))?;

    let oauth = QuickBooksOAuth::new(QuickBooksOAuthConfig {
        client_id: qb_config.client_id.clone(),
        client_secret: qb_config.client_secret.clone(),
        redirect_uri: qb_config.redirect_uri.clone(),
        environment: match qb_config.environment {
            crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
            crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
        },
    });

    // Exchange authorization code for tokens
    let tokens = oauth.exchange_code(&params.code, &params.realm_id).await
        .map_err(|e| billforge_core::Error::Validation(format!("OAuth token exchange failed: {}", e)))?;

    // Calculate token expiry times
    let now = Utc::now();
    let access_token_expires_at = now + Duration::seconds(tokens.expires_in);
    let refresh_token_expires_at = now + Duration::seconds(tokens.x_refresh_token_expires_in);

    // Get company info from QuickBooks
    let client = QuickBooksClient::new(
        tokens.access_token.clone(),
        params.realm_id.clone(),
        match qb_config.environment {
            crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
            crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
        },
    );

    let company_info = match client.get_company_info().await {
        Ok(info) => Some(info),
        Err(e) => {
            warn!(error = %e, "Failed to fetch company info from QuickBooks — using defaults");
            None
        }
    };
    let company_name = company_info
        .and_then(|info| info.get("CompanyName")?.as_str().map(|s| s.to_string()));

    // Store tokens in database
    sqlx::query(
        "INSERT INTO quickbooks_connections (
            tenant_id, company_id, company_name, access_token, refresh_token,
            access_token_expires_at, refresh_token_expires_at, environment, sync_enabled,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            company_id = $2,
            company_name = $3,
            access_token = $4,
            refresh_token = $5,
            access_token_expires_at = $6,
            refresh_token_expires_at = $7,
            updated_at = NOW()"
    )
    .bind(tenant_id.as_uuid())
    .bind(&params.realm_id)
    .bind(&company_name)
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(access_token_expires_at)
    .bind(refresh_token_expires_at)
    .bind(match qb_config.environment {
        crate::config::QuickBooksEnvironment::Sandbox => "sandbox",
        crate::config::QuickBooksEnvironment::Production => "production",
    })
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to store QuickBooks tokens: {}", e)))?;

    // Clean up state token
    if let Err(e) = sqlx::query("DELETE FROM quickbooks_oauth_states WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(&*pool)
        .await
    {
        warn!(error = %e, "Failed to clean up OAuth state — will expire naturally");
    }

    // Redirect to success page
    Ok(Redirect::temporary(&format!("{}/dashboard?quickbooks=connected", state.config.frontend_url)))
}

/// Disconnect QuickBooks
#[utoipa::path(
    post,
    path = "/api/v1/quickbooks/disconnect",
    tag = "QuickBooks",
    responses(
        (status = 200, description = "QuickBooks disconnected"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn quickbooks_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get current connection
    let connection: Option<(String,)> = sqlx::query_as(
        "SELECT refresh_token FROM quickbooks_connections WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch connection for disconnect");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    if let Some((refresh_token,)) = connection {
        // Revoke token with QuickBooks
        if let Some(qb_config) = &state.config.quickbooks {
            let oauth = QuickBooksOAuth::new(QuickBooksOAuthConfig {
                client_id: qb_config.client_id.clone(),
                client_secret: qb_config.client_secret.clone(),
                redirect_uri: qb_config.redirect_uri.clone(),
                environment: match qb_config.environment {
                    crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
                    crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
                },
            });

            if let Err(e) = oauth.revoke_token(&refresh_token).await {
                warn!(error = %e, "Failed to revoke QuickBooks token — it will expire naturally");
            }
        }
    }

    // Delete connection from database
    if let Err(e) = sqlx::query("DELETE FROM quickbooks_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
    {
        warn!(error = %e, "Failed to delete QuickBooks connection record");
    }

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

/// Get QuickBooks connection status
#[utoipa::path(
    get,
    path = "/api/v1/quickbooks/status",
    tag = "QuickBooks",
    responses(
        (status = 200, description = "QuickBooks status", body = QuickBooksStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn quickbooks_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get connection status
    let connection: Option<(String, Option<String>, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT company_id, company_name, sync_enabled, last_sync_at
         FROM quickbooks_connections
         WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch connection status");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let status = if let Some((company_id, company_name, sync_enabled, last_sync_at)) = connection {
        QuickBooksStatus {
            connected: true,
            company_id: Some(company_id),
            company_name,
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        QuickBooksStatus {
            connected: false,
            company_id: None,
            company_name: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Sync vendors from QuickBooks
#[utoipa::path(
    post,
    path = "/api/v1/quickbooks/sync/vendors",
    tag = "QuickBooks",
    request_body = SyncVendorsRequest,
    responses(
        (status = 200, description = "Vendors synced", body = SyncVendorsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_vendors(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(_request): Json<SyncVendorsRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let client = get_authenticated_qb_client(&state, &tenant.tenant_id).await?;

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO quickbooks_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'vendors', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to create vendor sync log");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    // Fetch vendors from QuickBooks (paginate through all results)
    let mut all_vendors = Vec::new();
    let mut start_position = 1;
    let max_results = 100;

    loop {
        let vendors = client.query_vendors(start_position, max_results).await
            .map_err(|e| billforge_core::Error::Validation(format!("QuickBooks API error: {}", e)))?;

        if vendors.is_empty() {
            break;
        }

        all_vendors.extend(vendors);
        start_position += max_results;
    }

    let mut imported = 0u64;
    let mut updated = 0u64;
    let skipped = 0u64;
    let mut errors = 0u64;

    // Sync each vendor
    for qb_vendor in all_vendors {
        // Check if vendor already exists
        let existing: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
            "SELECT v.id FROM vendors v
             INNER JOIN quickbooks_vendor_mappings m ON m.billforge_vendor_id = v.id
             WHERE m.tenant_id = $1 AND m.quickbooks_vendor_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&qb_vendor.Id)
        .fetch_optional(&*pool)
        .await
        .map_err(|e| {
            error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to look up vendor mapping");
            billforge_core::Error::Internal(format!("Database error: {}", e))
        })?;

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            let email = qb_vendor.PrimaryEmailAddr.as_ref().map(|e| e.Address.as_str()).unwrap_or("");
            let phone = qb_vendor.PrimaryPhone.as_ref().map(|p| p.FreeFormNumber.as_str()).unwrap_or("");

            if let Err(e) = sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW()
                 WHERE id = $1"
            )
            .bind(vendor_id)
            .bind(&qb_vendor.DisplayName)
            .bind(email)
            .bind(phone)
            .execute(&*pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to update vendor");
                errors += 1;
                continue;
            }

            // Update mapping
            if let Err(e) = sqlx::query(
                "UPDATE quickbooks_vendor_mappings
                 SET quickbooks_vendor_name = $3, sync_token = $4, last_synced_at = NOW(), updated_at = NOW()
                 WHERE tenant_id = $1 AND quickbooks_vendor_id = $2"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&qb_vendor.Id)
            .bind(&qb_vendor.DisplayName)
            .bind(&qb_vendor.SyncToken)
            .execute(&*pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to update vendor mapping");
                errors += 1;
                continue;
            }

            updated += 1;
        } else {
            // Create new vendor
            let vendor_id = Uuid::new_v4();
            let email = qb_vendor.PrimaryEmailAddr.as_ref().map(|e| e.Address.as_str()).unwrap_or("");
            let phone = qb_vendor.PrimaryPhone.as_ref().map(|p| p.FreeFormNumber.as_str()).unwrap_or("");
            let vendor_type = if qb_vendor.CompanyName.is_some() { "business" } else { "contractor" };

            if let Err(e) = sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())"
            )
            .bind(vendor_id)
            .bind(&qb_vendor.DisplayName)
            .bind(vendor_type)
            .bind(email)
            .bind(phone)
            .bind(if qb_vendor.Active { "active" } else { "inactive" })
            .execute(&*pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to insert vendor");
                errors += 1;
                continue;
            }

            // Create mapping
            if let Err(e) = sqlx::query(
                "INSERT INTO quickbooks_vendor_mappings
                 (tenant_id, quickbooks_vendor_id, billforge_vendor_id, quickbooks_vendor_name, sync_token, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&qb_vendor.Id)
            .bind(vendor_id)
            .bind(&qb_vendor.DisplayName)
            .bind(&qb_vendor.SyncToken)
            .execute(&*pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to insert vendor mapping");
                errors += 1;
                continue;
            }

            imported += 1;
        }
    }

    // Update sync log
    sqlx::query(
        "UPDATE quickbooks_sync_log
         SET status = 'completed', completed_at = NOW(), records_processed = $2, records_created = $3, records_updated = $4
         WHERE id = $1"
    )
    .bind(sync_id)
    .bind((imported + updated + skipped) as i32)
    .bind(imported as i32)
    .bind(updated as i32)
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to complete vendor sync log");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    // Update last sync time on connection
    sqlx::query(
        "UPDATE quickbooks_connections SET last_sync_at = NOW() WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to update last sync time");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let response = SyncVendorsResponse {
        imported,
        updated,
        skipped,
        errors,
    };

    Ok(Json(response))
}

/// Sync accounts from QuickBooks
#[utoipa::path(
    post,
    path = "/api/v1/quickbooks/sync/accounts",
    tag = "QuickBooks",
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
    let client = get_authenticated_qb_client(&state, &tenant.tenant_id).await?;

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO quickbooks_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'accounts', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to create account sync log");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    // Fetch accounts from QuickBooks
    let mut all_accounts = Vec::new();
    let mut start_position = 1;
    let max_results = 100;

    loop {
        let accounts = client.query_accounts(start_position, max_results).await
            .map_err(|e| billforge_core::Error::Validation(format!("QuickBooks API error: {}", e)))?;

        if accounts.is_empty() {
            break;
        }

        all_accounts.extend(accounts);
        start_position += max_results;
    }

    // Filter to expense accounts only
    let expense_accounts: Vec<_> = all_accounts
        .into_iter()
        .filter(|a| a.Classification == "Expense" && a.Active)
        .collect();

    let mut created = 0u64;
    let mut errors = 0u64;

    // Upsert account mappings
    for qb_account in expense_accounts {
        if let Err(e) = sqlx::query(
            "INSERT INTO quickbooks_account_mappings
             (tenant_id, quickbooks_account_id, quickbooks_account_name, quickbooks_account_type, billforge_gl_code, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $2, NOW(), NOW())
             ON CONFLICT (tenant_id, quickbooks_account_id) DO UPDATE SET
                quickbooks_account_name = $3,
                quickbooks_account_type = $4,
                updated_at = NOW()"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&qb_account.Id)
        .bind(&qb_account.Name)
        .bind(&qb_account.AccountType)
        .execute(&*pool)
        .await
        {
            error!(error = %e, account_id = %qb_account.Id, "Failed to upsert account mapping");
            errors += 1;
            continue;
        }

        created += 1;
    }

    // Update sync log
    sqlx::query(
        "UPDATE quickbooks_sync_log
         SET status = 'completed', completed_at = NOW(), records_processed = $2, records_created = $2
         WHERE id = $1"
    )
    .bind(sync_id)
    .bind(created as i32)
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to complete account sync log");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    Ok(Json(serde_json::json!({ "status": "synced", "count": created, "errors": errors })))
}

/// Export invoice to QuickBooks
#[utoipa::path(
    post,
    path = "/api/v1/quickbooks/export/invoice/{id}",
    tag = "QuickBooks",
    request_body = ExportInvoiceRequest,
    responses(
        (status = 200, description = "Invoice exported", body = ExportInvoiceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Invoice not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn export_invoice_to_quickbooks(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get invoice from database
    let invoice_id: billforge_core::domain::InvoiceId = request.invoice_id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, due_date, po_number
         FROM invoices WHERE id = $1"
    )
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch invoice for export");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let (vendor_name, invoice_number, total_cents, due_date, po_number) = invoice
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: request.invoice_id.clone(),
        })?;

    // Get vendor mapping
    let vendor_mapping: Option<(String,)> = sqlx::query_as(
        "SELECT quickbooks_vendor_id FROM quickbooks_vendor_mappings
         WHERE tenant_id = $1 AND billforge_vendor_id IN
         (SELECT vendor_id FROM invoices WHERE id = $2)"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch vendor mapping for export");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let quickbooks_vendor_id = vendor_mapping
        .map(|(id,)| id)
        .ok_or_else(|| billforge_core::Error::Validation("Vendor not found in QuickBooks. Please sync vendors first.".to_string()))?;

    let client = get_authenticated_qb_client(&state, &tenant.tenant_id).await?;

    // Build QuickBooks bill
    use billforge_quickbooks::{QBBill, QBBillLine, QBAccountBasedExpenseLineDetail, QBReference};

    let bill = QBBill {
        Id: "".to_string(),  // Empty for create
        VendorRef: QBReference {
            value: quickbooks_vendor_id,
            name: Some(vendor_name),
        },
        CurrencyRef: Some(QBReference {
            value: "USD".to_string(),
            name: Some("US Dollar".to_string()),
        }),
        DueDate: due_date.unwrap_or_else(|| chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()),
        TotalAmt: total_cents,
        Balance: total_cents,
        Line: vec![
            QBBillLine {
                Id: None,
                LineNum: Some(1),
                Description: Some(format!("Invoice {}", invoice_number)),
                Amount: total_cents,
                AccountBasedExpenseLineDetail: Some(QBAccountBasedExpenseLineDetail {
                    AccountRef: QBReference {
                        value: request.quickbooks_account_id.clone(),
                        name: None,
                    },
                    BillableStatus: None,
                    TaxCodeRef: None,
                }),
            },
        ],
        SyncToken: "0".to_string(),
        PrivateNote: po_number.clone(),
        MetaData: None,
    };

    // Create bill in QuickBooks
    let created_bill = client.create_bill(&bill).await
        .map_err(|e| billforge_core::Error::Validation(format!("QuickBooks API error: {}", e)))?;

    // Store export record
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO quickbooks_invoice_exports (id, tenant_id, invoice_id, quickbooks_bill_id, exported_at, export_status)
         VALUES ($1, $2, $3, $4, NOW(), 'synced')
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET
            quickbooks_bill_id = $4,
            exported_at = NOW(),
            export_status = 'synced',
            sync_error = NULL"
    )
    .bind(export_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&created_bill.Id)
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            quickbooks_bill_id = %created_bill.Id,
            invoice_id = %invoice_id.as_uuid(),
            "Invoice exported to QuickBooks but failed to save export record"
        );
        billforge_core::Error::Internal(format!(
            "Invoice exported to QuickBooks (bill ID: {}) but failed to save export record. Contact support.",
            created_bill.Id
        ))
    })?;

    let response = ExportInvoiceResponse {
        quickbooks_invoice_id: created_bill.Id,
        status: "synced".to_string(),
    };

    Ok(Json(response))
}

/// Get account mappings
#[utoipa::path(
    get,
    path = "/api/v1/quickbooks/mappings/accounts",
    tag = "QuickBooks",
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

    let mappings: Vec<AccountMapping> = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT quickbooks_account_id, billforge_gl_code, quickbooks_account_name, quickbooks_account_type, billforge_department
         FROM quickbooks_account_mappings
         WHERE tenant_id = $1
         ORDER BY quickbooks_account_name"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get account mappings: {}", e)))?
    .into_iter()
    .map(|(qb_id, bf_id, name, acct_type, _dept)| AccountMapping {
        billforge_account_id: bf_id,
        quickbooks_account_id: qb_id,
        account_name: name,
        account_type: acct_type,
    })
    .collect();

    Ok(Json(mappings))
}

/// Update account mappings
#[utoipa::path(
    post,
    path = "/api/v1/quickbooks/mappings/accounts",
    tag = "QuickBooks",
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
    let total = mappings.len() as u64;
    let mut errors = 0u64;

    for mapping in mappings {
        if let Err(e) = sqlx::query(
            "UPDATE quickbooks_account_mappings
             SET billforge_gl_code = $3, updated_at = NOW()
             WHERE tenant_id = $1 AND quickbooks_account_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&mapping.quickbooks_account_id)
        .bind(&mapping.billforge_account_id)
        .execute(&*pool)
        .await
        {
            error!(error = %e, account_id = %mapping.quickbooks_account_id, "Failed to update account mapping");
            errors += 1;
        }
    }

    if errors == total && total > 0 {
        return Err(billforge_core::Error::Internal(
            "All account mapping updates failed".to_string()
        ).into());
    }

    Ok(Json(serde_json::json!({ "status": "updated", "errors": errors })))
}

/// Build a QuickBooksOAuth instance from the app's QuickBooks config.
fn build_qb_oauth(
    qb_config: &crate::config::QuickBooksConfig,
) -> QuickBooksOAuth {
    QuickBooksOAuth::new(QuickBooksOAuthConfig {
        client_id: qb_config.client_id.clone(),
        client_secret: qb_config.client_secret.clone(),
        redirect_uri: qb_config.redirect_uri.clone(),
        environment: match qb_config.environment {
            crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
            crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
        },
    })
}

/// Map from the app config environment enum to the quickbooks crate environment enum.
fn qb_environment(qb_config: &crate::config::QuickBooksConfig) -> QuickBooksEnvironment {
    match qb_config.environment {
        crate::config::QuickBooksEnvironment::Sandbox => QuickBooksEnvironment::Sandbox,
        crate::config::QuickBooksEnvironment::Production => QuickBooksEnvironment::Production,
    }
}

/// Fetch the QuickBooks connection for a tenant, automatically refreshing the access token
/// if it is expired or near-expiry (within 5 minutes). Returns a ready-to-use `QuickBooksClient`.
async fn get_authenticated_qb_client(
    state: &AppState,
    tenant_id: &TenantId,
) -> crate::error::ApiResult<QuickBooksClient> {
    let pool = state.db.tenant(tenant_id).await?;

    // Fetch connection including refresh_token
    let connection: Option<(String, String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT company_id, access_token, refresh_token, access_token_expires_at \
         FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch QuickBooks connection for token refresh");
        billforge_core::Error::Internal(format!("Database error: {}", e))
    })?;

    let (company_id, mut access_token, refresh_token_val, token_expires_at) = connection
        .ok_or_else(|| billforge_core::Error::Validation(
            "QuickBooks not connected or sync disabled".to_string()
        ))?;

    // Refresh if token is expired or will expire within 5 minutes
    if token_expires_at <= Utc::now() + Duration::minutes(5) {
        let qb_config = state.config.quickbooks.as_ref()
            .ok_or_else(|| billforge_core::Error::Validation(
                "QuickBooks integration not configured".to_string()
            ))?;

        let oauth = build_qb_oauth(qb_config);

        let new_tokens = oauth.refresh_token(&refresh_token_val).await
            .map_err(|e| billforge_core::Error::Validation(
                format!("QuickBooks token refresh failed: {}. Please reconnect.", e)
            ))?;

        // Persist refreshed tokens
        let now = Utc::now();
        sqlx::query(
            "UPDATE quickbooks_connections \
             SET access_token = $2, refresh_token = $3, \
                 access_token_expires_at = $4, refresh_token_expires_at = $5, updated_at = NOW() \
             WHERE tenant_id = $1"
        )
        .bind(tenant_id.as_uuid())
        .bind(&new_tokens.access_token)
        .bind(&new_tokens.refresh_token)
        .bind(now + Duration::seconds(new_tokens.expires_in))
        .bind(now + Duration::seconds(new_tokens.x_refresh_token_expires_in))
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(
            format!("Failed to update tokens: {}", e)
        ))?;

        access_token = new_tokens.access_token;
    }

    let qb_config = state.config.quickbooks.as_ref()
        .ok_or_else(|| billforge_core::Error::Validation(
            "QuickBooks integration not configured".to_string()
        ))?;

    Ok(QuickBooksClient::new(access_token, company_id, qb_environment(qb_config)))
}
