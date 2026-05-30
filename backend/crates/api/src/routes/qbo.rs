//! Lightweight QBO (QuickBooks Online) integration endpoints.
//!
//! Provides a self-contained OAuth 2.0 connect flow and one-way vendor
//! pull (QBO -> BillForge) that operates independently from the
//! feature-gated `quickbooks` module in `billforge-quickbooks`.
//!
//! Handlers:
//! - `GET  /api/qbo/connect`        - build OAuth authorize URL
//! - `GET  /api/qbo/callback`       - exchange code for tokens (public)
//! - `POST /api/qbo/sync/vendors`   - pull vendors from QBO

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Query params for the OAuth callback.
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
    #[serde(rename = "realmId")]
    pub realm_id: String,
}

/// JSON body for the vendor sync trigger.
#[derive(Debug, Deserialize)]
pub struct SyncVendorsRequest {
    /// Force full sync instead of incremental.
    #[serde(default)]
    pub full_sync: bool,
}

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

/// Mount all QBO routes under `/api/qbo`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/connect", get(qbo_connect))
        .route("/callback", get(qbo_callback))
        .route("/sync/vendors", post(qbo_sync_vendors))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/qbo/connect` - Build and return the QBO OAuth 2.0 authorize URL.
///
/// The handler generates a random `state` token, persists it in the
/// `quickbooks_oauth_states` table for the calling tenant, and returns the
/// authorize URL as JSON so the caller can redirect the user.
async fn qbo_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let client_id = std::env::var("QBO_CLIENT_ID")
        .map_err(|_| billforge_core::Error::Validation("QBO_CLIENT_ID not configured".into()))?;
    let redirect_uri = std::env::var("QBO_REDIRECT_URI")
        .map_err(|_| billforge_core::Error::Validation("QBO_REDIRECT_URI not configured".into()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Generate CSRF state.
    let csrf_state = format!("{}:{}", tenant.tenant_id.as_str(), Uuid::new_v4());

    // Persist state token (10-minute TTL).
    let expires_at = Utc::now() + Duration::minutes(10);
    sqlx::query(
        "INSERT INTO quickbooks_oauth_states (tenant_id, state_token, expires_at, created_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (tenant_id) DO UPDATE
            SET state_token = EXCLUDED.state_token,
                expires_at   = EXCLUDED.expires_at,
                created_at   = NOW()",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&csrf_state)
    .bind(expires_at)
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to store OAuth state");
        billforge_core::Error::Internal(format!("Database error: {e}"))
    })?;

    let scope = "com.intuit.quickbooks.accounting";
    let authorize_url = format!(
        "https://appcenter.intuit.com/connect/oauth2?client_id={client_id}&redirect_uri={redirect_uri}&scope={scope}&state={csrf_state}&response_type=code"
    );

    Ok(Json(serde_json::json!({ "authorize_url": authorize_url })))
}

/// `GET /api/qbo/callback` - Exchange the OAuth authorization code for tokens.
///
/// Verifies the `state` parameter against the persisted CSRF token, exchanges
/// the code via the Intuit token endpoint, and upserts a row in
/// `quickbooks_connections`. This endpoint is public (called by Intuit redirect).
async fn qbo_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> ApiResult<impl IntoResponse> {
    // Extract tenant_id from state token (format: "<tenant_id>:<uuid>").
    let parts: Vec<&str> = params.state.split(':').collect();
    if parts.len() != 2 {
        return Err(billforge_core::Error::Validation("Invalid state token".into()).into());
    }
    let tenant_id: billforge_core::TenantId = parts[0].parse().map_err(|_| {
        billforge_core::Error::Validation("Invalid tenant ID in state token".into())
    })?;

    let pool = state.db.tenant(&tenant_id).await?;

    // Verify state token.
    let stored: Option<(String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT state_token, expires_at FROM quickbooks_oauth_states WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch OAuth state");
        billforge_core::Error::Internal(format!("Database error: {e}"))
    })?;

    let valid = stored
        .map(|(token, exp)| token == params.state && exp > Utc::now())
        .unwrap_or(false);

    if !valid {
        return Err(
            billforge_core::Error::Validation("Invalid or expired state token".into()).into(),
        );
    }

    // Exchange code for tokens.
    let client_id = std::env::var("QBO_CLIENT_ID")
        .map_err(|_| billforge_core::Error::Validation("QBO_CLIENT_ID not configured".into()))?;
    let client_secret = std::env::var("QBO_CLIENT_SECRET").map_err(|_| {
        billforge_core::Error::Validation("QBO_CLIENT_SECRET not configured".into())
    })?;
    let redirect_uri = std::env::var("QBO_REDIRECT_URI")
        .map_err(|_| billforge_core::Error::Validation("QBO_REDIRECT_URI not configured".into()))?;

    let token_response: serde_json::Value = reqwest::Client::new()
        .post("https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer")
        .basic_auth(&client_id, Some(&client_secret))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &params.code),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "Token exchange request failed");
            billforge_core::Error::Internal(format!("Token exchange failed: {e}"))
        })?
        .json()
        .await
        .map_err(|e| {
            error!(error = %e, "Token exchange response parse failed");
            billforge_core::Error::Internal(format!("Token exchange parse failed: {e}"))
        })?;

    let access_token = token_response["access_token"].as_str().ok_or_else(|| {
        billforge_core::Error::Validation("Missing access_token in response".into())
    })?;
    let refresh_token = token_response["refresh_token"].as_str().ok_or_else(|| {
        billforge_core::Error::Validation("Missing refresh_token in response".into())
    })?;
    let expires_in: i64 = token_response["expires_in"].as_i64().unwrap_or(3600);
    let x_refresh_token_expires_in: i64 = token_response["x_refresh_token_expires_in"]
        .as_i64()
        .unwrap_or(8726400); // ~100 days

    let now = Utc::now();
    let access_expires = now + Duration::seconds(expires_in);
    let refresh_expires = now + Duration::seconds(x_refresh_token_expires_in);

    // Upsert connection row.
    sqlx::query(
        "INSERT INTO quickbooks_connections (
            tenant_id, company_id, access_token, refresh_token,
            access_token_expires_at, refresh_token_expires_at,
            environment, sync_enabled, last_sync_status,
            created_at, updated_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, 'sandbox', true, 'idle', NOW(), NOW())
         ON CONFLICT (tenant_id, company_id) DO UPDATE SET
            access_token            = EXCLUDED.access_token,
            refresh_token           = EXCLUDED.refresh_token,
            access_token_expires_at = EXCLUDED.access_token_expires_at,
            refresh_token_expires_at= EXCLUDED.refresh_token_expires_at,
            updated_at              = NOW()",
    )
    .bind(tenant_id.as_uuid())
    .bind(&params.realm_id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(access_expires)
    .bind(refresh_expires)
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to upsert QBO connection");
        billforge_core::Error::Internal(format!("Database error: {e}"))
    })?;

    // Clean up state token.
    if let Err(e) = sqlx::query("DELETE FROM quickbooks_oauth_states WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(&*pool)
        .await
    {
        warn!(error = %e, "Failed to clean up OAuth state");
    }

    info!(realm_id = %params.realm_id, "QBO OAuth callback successful");

    Ok(Json(serde_json::json!({
        "connected": true,
        "realm_id": params.realm_id,
    })))
}

/// `POST /api/qbo/sync/vendors` - Pull vendors from QBO and upsert into BillForge.
///
/// Reads the QBO connection for the tenant, refreshes the access token if
/// expired, queries the QBO Vendor API, and upserts rows into the `vendors`
/// table keyed by `external_id = 'qbo:{Id}'`.
async fn qbo_sync_vendors(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(_req): Json<SyncVendorsRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // BEC fraud prevention: skip ERP sync for vendors with pending banking verification.
    // This same guard pattern should be applied to NetSuite/Xero/Sage syncs (deferred).
    // The check is per-vendor during the upsert loop below (see inline comment).

    // Load connection.

    // Load connection.
    let conn: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT company_id, access_token, access_token_expires_at
         FROM quickbooks_connections
         WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to load QBO connection");
        billforge_core::Error::Internal(format!("Database error: {e}"))
    })?;

    let (realm_id, mut access_token, token_expires_at) = conn.ok_or_else(|| {
        billforge_core::Error::Validation("QBO not connected or sync disabled".into())
    })?;

    // Refresh token if expired or near-expiry (5-minute buffer).
    if token_expires_at <= Utc::now() + Duration::minutes(5) {
        let refreshed = refresh_access_token(&tenant.tenant_id, &pool).await?;
        access_token = refreshed;
    }

    // Query QBO vendors.
    let query_url = format!(
        "https://sandbox-quickbooks.api.intuit.com/v3/company/{realm_id}/query?query=SELECT%20*%20FROM%20Vendor"
    );

    let qbo_response: serde_json::Value = reqwest::Client::new()
        .get(&query_url)
        .bearer_auth(&access_token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "QBO vendor query request failed");
            billforge_core::Error::Internal(format!("QBO request failed: {e}"))
        })?
        .json()
        .await
        .map_err(|e| {
            error!(error = %e, "QBO vendor query response parse failed");
            billforge_core::Error::Internal(format!("QBO response parse failed: {e}"))
        })?;

    let vendors = qbo_response
        .get("QueryResponse")
        .and_then(|qr| qr.get("Vendor"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut synced: u64 = 0;

    for vendor in &vendors {
        let qb_id = match vendor.get("Id").and_then(|id| id.as_str()) {
            Some(id) => id,
            None => continue,
        };
        let external_id = format!("qbo:{qb_id}");

        // BEC fraud prevention: skip upsert if existing vendor has pending banking verification.
        // The same guard pattern applies to NetSuite/Xero/Sage syncs (deferred to a follow-up).
        let existing_hold: Option<bool> = sqlx::query_scalar(
            "SELECT payment_hold FROM vendors WHERE tenant_id = $1 AND external_id = $2",
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&external_id)
        .fetch_optional(&*pool)
        .await
        .unwrap_or(None)
        .flatten();

        if existing_hold == Some(true) {
            warn!(qb_id = %qb_id, "Skipping vendor sync - pending banking verification");
            continue;
        }

        let display_name = vendor
            .get("DisplayName")
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown");
        let email = vendor
            .get("PrimaryEmailAddr")
            .and_then(|e| e.get("Address"))
            .and_then(|a| a.as_str())
            .unwrap_or("");
        let phone = vendor
            .get("PrimaryPhone")
            .and_then(|p| p.get("FreeFormNumber"))
            .and_then(|n| n.as_str())
            .unwrap_or("");
        let active = vendor
            .get("Active")
            .and_then(|a| a.as_bool())
            .unwrap_or(true);
        let status = if active { "active" } else { "inactive" };
        let vendor_type = if vendor.get("CompanyName").is_some() {
            "business"
        } else {
            "contractor"
        };

        let result = sqlx::query(
            "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, external_id, created_at, updated_at)
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
             ON CONFLICT (tenant_id, external_id) WHERE external_id IS NOT NULL DO UPDATE SET
                name       = EXCLUDED.name,
                vendor_type= EXCLUDED.vendor_type,
                email      = EXCLUDED.email,
                phone      = EXCLUDED.phone,
                status     = EXCLUDED.status,
                updated_at = NOW()",
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(display_name)
        .bind(vendor_type)
        .bind(email)
        .bind(phone)
        .bind(status)
        .bind(&external_id)
        .execute(&*pool)
        .await;

        match result {
            Ok(_) => synced += 1,
            Err(e) => {
                warn!(error = %e, qb_id = %qb_id, "Failed to upsert vendor");
            }
        }
    }

    // Update sync status.
    sqlx::query(
        "UPDATE quickbooks_connections
         SET last_sync_at = NOW(),
             last_sync_status = 'success',
             last_sync_error = NULL,
             updated_at = NOW()
         WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to update sync status");
        billforge_core::Error::Internal(format!("Database error: {e}"))
    })?;

    info!(synced = synced, "QBO vendor sync complete");

    Ok(Json(serde_json::json!({ "synced": synced })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Refresh the access token for a tenant using the stored refresh token.
/// Persists the new tokens and returns the new access token.
async fn refresh_access_token(
    tenant_id: &billforge_core::TenantId,
    pool: &sqlx::PgPool,
) -> ApiResult<String> {
    let client_id = std::env::var("QBO_CLIENT_ID")
        .map_err(|_| billforge_core::Error::Validation("QBO_CLIENT_ID not configured".into()))?;
    let client_secret = std::env::var("QBO_CLIENT_SECRET").map_err(|_| {
        billforge_core::Error::Validation("QBO_CLIENT_SECRET not configured".into())
    })?;

    let (refresh_token_val,): (String,) =
        sqlx::query_as("SELECT refresh_token FROM quickbooks_connections WHERE tenant_id = $1")
            .bind(tenant_id.as_uuid())
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to fetch refresh token");
                billforge_core::Error::Internal(format!("Database error: {e}"))
            })?;

    let token_response: serde_json::Value = reqwest::Client::new()
        .post("https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer")
        .basic_auth(&client_id, Some(&client_secret))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh_token_val),
        ])
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "Token refresh request failed");
            billforge_core::Error::Internal(format!("Token refresh failed: {e}"))
        })?
        .json()
        .await
        .map_err(|e| {
            error!(error = %e, "Token refresh response parse failed");
            billforge_core::Error::Internal(format!("Token refresh parse failed: {e}"))
        })?;

    let new_access = token_response["access_token"].as_str().ok_or_else(|| {
        billforge_core::Error::Validation("Missing access_token in refresh response".into())
    })?;
    let new_refresh = token_response["refresh_token"].as_str().ok_or_else(|| {
        billforge_core::Error::Validation("Missing refresh_token in refresh response".into())
    })?;
    let expires_in: i64 = token_response["expires_in"].as_i64().unwrap_or(3600);
    let x_refresh_expires_in: i64 = token_response["x_refresh_token_expires_in"]
        .as_i64()
        .unwrap_or(8726400);

    let now = Utc::now();
    sqlx::query(
        "UPDATE quickbooks_connections
         SET access_token = $2,
             refresh_token = $3,
             access_token_expires_at = $4,
             refresh_token_expires_at = $5,
             updated_at = NOW()
         WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .bind(new_access)
    .bind(new_refresh)
    .bind(now + Duration::seconds(expires_in))
    .bind(now + Duration::seconds(x_refresh_expires_in))
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to persist refreshed tokens");
        billforge_core::Error::Internal(format!("Database error: {e}"))
    })?;

    Ok(new_access.to_owned())
}
