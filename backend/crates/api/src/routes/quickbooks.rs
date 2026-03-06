//! QuickBooks Online integration endpoints
//!
//! Provides OAuth 2.0 flow and sync capabilities for QuickBooks Online:
//! - OAuth connection/disconnection
//! - Vendor sync (QuickBooks → BillForge)
//! - Invoice export (BillForge → QuickBooks)
//! - Account/Category mapping
//! - Sync status tracking

use crate::error::ApiResult;
use crate::state::AppState;
use axum::{
    extract::State,
    response::{IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual OAuth flow
    // 1. Generate state token
    // 2. Build QuickBooks OAuth URL
    // 3. Redirect to QuickBooks

    let oauth_url = format!(
        "https://appcenter.intuit.com/connect/oauth2?client_id={}&redirect_uri={}&response_type=code&scope=com.intuit.quickbooks.accounting&state={}",
        "YOUR_CLIENT_ID",
        "YOUR_REDIRECT_URI",
        "state_token"
    );

    Ok(Redirect::temporary(&oauth_url))
}

/// Handle QuickBooks OAuth callback
#[utoipa::path(
    get,
    path = "/api/v1/quickbooks/callback",
    tag = "QuickBooks",
    params(
        ("code" = String, Path, description = "OAuth authorization code"),
        ("state" = String, Path, description = "State token for CSRF protection"),
        ("realmId" = String, Path, description = "QuickBooks company ID")
    ),
    responses(
        (status = 302, description = "Redirect to success page"),
        (status = 400, description = "Invalid OAuth response"),
        (status = 500, description = "Internal server error")
    )
)]
async fn quickbooks_callback(
    State(state): State<AppState>,
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual OAuth callback
    // 1. Validate state token
    // 2. Exchange code for access token
    // 3. Store tokens securely
    // 4. Redirect to success page

    // For now, redirect to dashboard
    Ok(Redirect::temporary("/dashboard?quickbooks=connected"))
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
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement disconnect
    // 1. Revoke tokens
    // 2. Delete stored credentials

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
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual status check

    let status = QuickBooksStatus {
        connected: false,
        company_id: None,
        company_name: None,
        last_sync_at: None,
        sync_enabled: false,
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
    Json(request): Json<SyncVendorsRequest>,
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual vendor sync
    // 1. Fetch vendors from QuickBooks API
    // 2. Match with existing vendors
    // 3. Create/update vendors in BillForge

    let response = SyncVendorsResponse {
        imported: 0,
        updated: 0,
        skipped: 0,
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
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual account sync

    Ok(Json(serde_json::json!({ "status": "synced", "count": 0 })))
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
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual invoice export
    // 1. Fetch invoice from BillForge
    // 2. Map to QuickBooks format
    // 3. Create bill in QuickBooks
    // 4. Store sync status

    let response = ExportInvoiceResponse {
        quickbooks_invoice_id: "qb-invoice-123".to_string(),
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
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual mapping retrieval

    let mappings: Vec<AccountMapping> = vec![];

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
    Json(mappings): Json<Vec<AccountMapping>>,
) -> ApiResult<impl IntoResponse> {
    let tenant_id = "sandbox".to_string(); // TODO: Extract from auth

    // TODO: Implement actual mapping update

    Ok(Json(serde_json::json!({ "status": "updated" })))
}
