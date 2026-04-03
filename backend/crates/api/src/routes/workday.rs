//! Workday Financial Management integration endpoints
//!
//! Workday uses OAuth 2.0 (Authorization Code Grant) like Salesforce.
//! The integration provides:
//! - Supplier sync (Workday Suppliers → BillForge Vendors)
//! - Supplier invoice export (BillForge → Workday)
//! - Ledger account mapping
//! - Multi-company support
//!
//! Endpoints:
//! - GET /connect — Initiate Workday OAuth flow
//! - GET /callback — Handle OAuth callback
//! - POST /disconnect — Revoke tokens & disconnect
//! - GET /status — Check connection status
//! - POST /sync/suppliers — Sync suppliers from Workday
//! - POST /sync/accounts — Sync ledger accounts from Workday
//! - POST /export/invoice/:id — Export approved invoice as Workday supplier invoice
//! - GET /mappings/accounts — Get ledger account mappings
//! - POST /mappings/accounts — Update ledger account mappings
//! - GET /companies — List available companies (multi-company)

use crate::error::ApiResult;
use crate::extractors::TenantCtx;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use billforge_workday::{WorkdayClient, WorkdayOAuth, WorkdayOAuthConfig};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // OAuth endpoints
        .route("/connect", get(workday_connect))
        .route("/callback", get(workday_callback))
        .route("/disconnect", post(workday_disconnect))
        .route("/status", get(workday_status))
        // Sync endpoints
        .route("/sync/suppliers", post(sync_suppliers))
        .route("/sync/accounts", post(sync_accounts))
        .route("/export/invoice/:id", post(export_invoice_to_workday))
        // Mapping endpoints
        .route("/mappings/accounts", get(get_account_mappings))
        .route("/mappings/accounts", post(update_account_mappings))
        // Multi-company
        .route("/companies", get(list_companies))
}

// ──────────────────────────── Types ────────────────────────────

/// Workday connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkdayStatus {
    /// Whether Workday is connected
    pub connected: bool,
    /// Workday tenant URL
    pub tenant_url: Option<String>,
    /// Workday tenant name
    pub tenant_name: Option<String>,
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

/// Workday ledger account mapping
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkdayAccountMapping {
    /// BillForge GL code
    pub billforge_gl_code: String,
    /// Workday account ID
    pub workday_account_id: String,
    /// Account name
    pub workday_account_name: String,
    /// Account type (Asset, Liability, Revenue, Expense, Equity)
    pub workday_account_type: String,
}

/// Export invoice to Workday request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceRequest {
    /// Invoice ID to export
    pub invoice_id: String,
    /// Workday ledger account ID for the line
    pub ledger_account_id: Option<String>,
    /// Spend category reference
    pub spend_category: Option<String>,
    /// Cost center reference
    pub cost_center: Option<String>,
    /// Company reference (for multi-company)
    pub company_reference: Option<String>,
}

/// Export invoice response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportInvoiceResponse {
    /// Workday invoice ID
    pub workday_invoice_id: String,
    /// Export status
    pub status: String,
}

// ──────────────────────────── Handlers ────────────────────────────

/// Initiate Workday OAuth connection
#[utoipa::path(
    get,
    path = "/api/v1/workday/connect",
    tag = "Workday",
    responses(
        (status = 302, description = "Redirect to Workday OAuth"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn workday_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let wd_config = state.config.workday.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("Workday integration not configured".to_string())
    })?;

    let oauth = WorkdayOAuth::new(WorkdayOAuthConfig {
        client_id: wd_config.client_id.clone(),
        client_secret: wd_config.client_secret.clone(),
        refresh_token: String::new(),
        tenant_url: wd_config.tenant_url.clone(),
        tenant_name: wd_config.tenant_name.clone(),
    });

    // Generate state token with tenant ID
    let state_token = format!("{}:{}", tenant.tenant_id.as_str(), Uuid::new_v4());

    // Store state token for verification
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let expires_at = Utc::now() + Duration::minutes(10);
    sqlx::query(
        "INSERT INTO workday_oauth_states (tenant_id, state_token, expires_at, created_at)
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

/// Handle Workday OAuth callback
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/workday/callback",
    tag = "Workday",
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
async fn workday_callback(
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
        "SELECT state_token, expires_at FROM workday_oauth_states WHERE tenant_id = $1",
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

    // Get Workday configuration
    let wd_config = state.config.workday.as_ref().ok_or_else(|| {
        billforge_core::Error::Validation("Workday integration not configured".to_string())
    })?;

    let oauth = WorkdayOAuth::new(WorkdayOAuthConfig {
        client_id: wd_config.client_id.clone(),
        client_secret: wd_config.client_secret.clone(),
        refresh_token: String::new(),
        tenant_url: wd_config.tenant_url.clone(),
        tenant_name: wd_config.tenant_name.clone(),
    });

    // Exchange authorization code for tokens
    let tokens = oauth.exchange_code(&params.code).await.map_err(|e| {
        billforge_core::Error::Validation(format!("Workday OAuth token exchange failed: {}", e))
    })?;

    // Workday access tokens expire based on the expires_in field
    let now = Utc::now();
    let access_token_expires_at = now + Duration::seconds(tokens.expires_in);

    // Get worker info from Workday
    let client = WorkdayClient::new(
        tokens.access_token.clone(),
        wd_config.tenant_url.clone(),
        wd_config.tenant_name.clone(),
    );

    let _worker_info = client.get_worker_info().await.ok();

    // Store tokens in database
    sqlx::query(
        "INSERT INTO workday_connections (
            tenant_id, workday_tenant_url, workday_tenant_name, access_token, refresh_token,
            access_token_expires_at, sync_enabled, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            workday_tenant_url = $2,
            workday_tenant_name = $3,
            access_token = $4,
            refresh_token = COALESCE($5, workday_connections.refresh_token),
            access_token_expires_at = $6,
            updated_at = NOW()",
    )
    .bind(tenant_id.as_uuid())
    .bind(&wd_config.tenant_url)
    .bind(&wd_config.tenant_name)
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(access_token_expires_at)
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to store Workday tokens: {}", e))
    })?;

    // Clean up state token
    sqlx::query("DELETE FROM workday_oauth_states WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Redirect::temporary(&format!(
        "{}/dashboard?workday=connected",
        state.config.frontend_url
    )))
}

/// Disconnect Workday
#[utoipa::path(
    post,
    path = "/api/v1/workday/disconnect",
    tag = "Workday",
    responses(
        (status = 200, description = "Workday disconnected"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn workday_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get current connection to revoke token
    let connection: Option<(String, String, String)> = sqlx::query_as(
        "SELECT access_token, workday_tenant_url, workday_tenant_name
         FROM workday_connections WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    if let Some((access_token, tenant_url, tenant_name)) = connection {
        // Revoke token with Workday
        if let Some(wd_config) = &state.config.workday {
            let oauth = WorkdayOAuth::new(WorkdayOAuthConfig {
                client_id: wd_config.client_id.clone(),
                client_secret: wd_config.client_secret.clone(),
                refresh_token: String::new(),
                tenant_url,
                tenant_name,
            });

            oauth.revoke_token(&access_token).await.ok();
        }
    }

    // Delete connection from database
    sqlx::query("DELETE FROM workday_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

/// Get Workday connection status
#[utoipa::path(
    get,
    path = "/api/v1/workday/status",
    tag = "Workday",
    responses(
        (status = 200, description = "Workday status", body = WorkdayStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn workday_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let connection: Option<(String, String, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT workday_tenant_url, workday_tenant_name, sync_enabled, last_sync_at
         FROM workday_connections
         WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let status = if let Some((tenant_url, tenant_name, sync_enabled, last_sync_at)) = connection {
        WorkdayStatus {
            connected: true,
            tenant_url: Some(tenant_url),
            tenant_name: Some(tenant_name),
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        WorkdayStatus {
            connected: false,
            tenant_url: None,
            tenant_name: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Helper: get a Workday client with token refresh support
async fn get_workday_client(
    state: &AppState,
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
) -> Result<WorkdayClient, billforge_core::Error> {
    let connection: Option<(String, String, String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT access_token, refresh_token, workday_tenant_url, workday_tenant_name, access_token_expires_at
         FROM workday_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (mut access_token, refresh_token, tenant_url, tenant_name, token_expires_at) = connection
        .ok_or_else(
        || billforge_core::Error::Validation("Workday not connected or sync disabled".to_string()),
    )?;

    // Check if token needs refresh
    if token_expires_at <= Utc::now() {
        if let Some(wd_config) = &state.config.workday {
            let oauth = WorkdayOAuth::new(WorkdayOAuthConfig {
                client_id: wd_config.client_id.clone(),
                client_secret: wd_config.client_secret.clone(),
                refresh_token: refresh_token.clone(),
                tenant_url: tenant_url.clone(),
                tenant_name: tenant_name.clone(),
            });

            match oauth.refresh_token(&refresh_token).await {
                Ok(new_tokens) => {
                    let new_expires = Utc::now() + Duration::seconds(new_tokens.expires_in);
                    sqlx::query(
                        "UPDATE workday_connections
                         SET access_token = $2, refresh_token = $3, access_token_expires_at = $4, updated_at = NOW()
                         WHERE tenant_id = $1"
                    )
                    .bind(tenant_id.as_uuid())
                    .bind(&new_tokens.access_token)
                    .bind(&new_tokens.refresh_token)
                    .bind(new_expires)
                    .execute(pool)
                    .await
                    .ok();

                    access_token = new_tokens.access_token;
                }
                Err(_) => {
                    return Err(billforge_core::Error::Validation(
                        "Workday token expired. Please reconnect.".to_string(),
                    ));
                }
            }
        } else {
            return Err(billforge_core::Error::Validation(
                "Workday integration not configured".to_string(),
            ));
        }
    }

    Ok(WorkdayClient::new(access_token, tenant_url, tenant_name))
}

/// Sync suppliers from Workday
#[utoipa::path(
    post,
    path = "/api/v1/workday/sync/suppliers",
    tag = "Workday",
    request_body = SyncRequest,
    responses(
        (status = 200, description = "Suppliers synced", body = SyncResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_suppliers(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<SyncRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let client = get_workday_client(&state, &pool, &tenant.tenant_id).await?;

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workday_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'suppliers', 'running', NOW())",
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    // Fetch suppliers (paginate through all)
    let mut all_suppliers = Vec::new();
    let mut page = 0;
    let page_size = 100;

    loop {
        let result = client
            .query_suppliers(page, page_size)
            .await
            .map_err(|e| billforge_core::Error::Validation(format!("Workday API error: {}", e)))?;

        let fetched = result.data.len();
        all_suppliers.extend(result.data);

        if fetched < page_size as usize {
            break;
        }
        page += 1;
    }

    let mut imported = 0u64;
    let mut updated = 0u64;
    let skipped = 0u64;

    // Sync each supplier
    for supplier in &all_suppliers {
        // Check if supplier mapping exists
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v
             INNER JOIN workday_supplier_mappings m ON m.billforge_vendor_id = v.id
             WHERE m.tenant_id = $1 AND m.workday_supplier_id = $2",
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&supplier.supplier_id)
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        let email = supplier.primary_email.as_deref().unwrap_or("");
        let phone = supplier.primary_phone.as_deref().unwrap_or("");

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&supplier.supplier_name)
            .bind(email)
            .bind(phone)
            .execute(&*pool)
            .await
            .ok();

            updated += 1;
        } else {
            // Create new vendor
            let vendor_id = Uuid::new_v4();
            let vendor_type = supplier.supplier_category.as_deref().unwrap_or("business");

            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())"
            )
            .bind(vendor_id)
            .bind(&supplier.supplier_name)
            .bind(vendor_type)
            .bind(email)
            .bind(phone)
            .bind(if supplier.status == "Active" { "active" } else { "inactive" })
            .execute(&*pool)
            .await
            .ok();

            // Create mapping
            sqlx::query(
                "INSERT INTO workday_supplier_mappings
                 (tenant_id, workday_supplier_id, billforge_vendor_id, workday_supplier_name, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&supplier.supplier_id)
            .bind(vendor_id)
            .bind(&supplier.supplier_name)
            .execute(&*pool)
            .await
            .ok();

            imported += 1;
        }
    }

    // Update sync log
    sqlx::query(
        "UPDATE workday_sync_log
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
    sqlx::query("UPDATE workday_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(SyncResponse {
        imported,
        updated,
        skipped,
    }))
}

/// Sync ledger accounts from Workday
#[utoipa::path(
    post,
    path = "/api/v1/workday/sync/accounts",
    tag = "Workday",
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
    let client = get_workday_client(&state, &pool, &tenant.tenant_id).await?;

    // Fetch ledger accounts (paginate through all)
    let mut all_accounts = Vec::new();
    let mut page = 0;
    let page_size = 100;

    loop {
        let result = client
            .query_ledger_accounts(page, page_size)
            .await
            .map_err(|e| billforge_core::Error::Validation(format!("Workday API error: {}", e)))?;

        let fetched = result.data.len();
        all_accounts.extend(result.data);

        if fetched < page_size as usize {
            break;
        }
        page += 1;
    }

    let mut created = 0u64;

    // Upsert account mappings
    for account in &all_accounts {
        sqlx::query(
            "INSERT INTO workday_account_mappings
             (tenant_id, workday_account_id, workday_account_name, workday_account_type, billforge_gl_code, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $2, NOW(), NOW())
             ON CONFLICT (tenant_id, workday_account_id) DO UPDATE SET
                workday_account_name = $3,
                workday_account_type = $4,
                updated_at = NOW()"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&account.ledger_account_id)
        .bind(&account.name)
        .bind(&account.account_type)
        .execute(&*pool)
        .await
        .ok();

        created += 1;
    }

    Ok(Json(
        serde_json::json!({ "status": "synced", "count": created }),
    ))
}

/// Export invoice to Workday as supplier invoice
#[utoipa::path(
    post,
    path = "/api/v1/workday/export/invoice/{id}",
    tag = "Workday",
    request_body = ExportInvoiceRequest,
    responses(
        (status = 200, description = "Invoice exported", body = ExportInvoiceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Invoice not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn export_invoice_to_workday(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let client = get_workday_client(&state, &pool, &tenant.tenant_id).await?;

    // Get invoice from database
    let invoice_id: billforge_core::domain::InvoiceId = request
        .invoice_id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, due_date, po_number
         FROM invoices WHERE id = $1",
    )
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (vendor_name, invoice_number, total_cents, due_date, po_number) =
        invoice.ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: request.invoice_id.clone(),
        })?;

    // Get supplier mapping
    let supplier_mapping: Option<(String,)> = sqlx::query_as(
        "SELECT workday_supplier_id FROM workday_supplier_mappings
         WHERE tenant_id = $1 AND billforge_vendor_id IN
         (SELECT vendor_id FROM invoices WHERE id = $2)",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let workday_supplier_id = supplier_mapping.map(|(id,)| id).ok_or_else(|| {
        billforge_core::Error::Validation(
            "Vendor not found in Workday. Please sync suppliers first.".to_string(),
        )
    })?;

    // Build Workday supplier invoice
    use billforge_workday::{WorkdayInvoiceLine, WorkdaySupplierInvoice};

    let amount = total_cents as f64 / 100.0;
    let today = chrono::Utc::now().date_naive();

    let wd_invoice = WorkdaySupplierInvoice {
        id: None,
        invoice_number: invoice_number.clone(),
        supplier_id: workday_supplier_id,
        invoice_date: today,
        due_date: due_date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        total_amount: amount,
        currency: Some("USD".to_string()),
        memo: Some(format!("BillForge export: Invoice {}", invoice_number)),
        lines: vec![WorkdayInvoiceLine {
            line_number: 1,
            amount,
            memo: Some(format!("Invoice {}", invoice_number)),
            spend_category: request.spend_category.clone(),
            ledger_account: request.ledger_account_id.clone(),
            cost_center: request.cost_center.clone(),
            project: None,
        }],
        status: None,
        company_reference: request.company_reference.clone(),
    };

    // Create supplier invoice in Workday
    let result = client
        .create_supplier_invoice(&wd_invoice)
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Workday API error: {}", e)))?;

    let workday_invoice_id = result.id.unwrap_or_default();

    // Store export record
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workday_invoice_exports (id, tenant_id, invoice_id, workday_invoice_id, exported_at, export_status)
         VALUES ($1, $2, $3, $4, NOW(), 'synced')
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET
            workday_invoice_id = $4,
            exported_at = NOW(),
            export_status = 'synced'"
    )
    .bind(export_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&workday_invoice_id)
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(ExportInvoiceResponse {
        workday_invoice_id,
        status: "synced".to_string(),
    }))
}

/// Get ledger account mappings
#[utoipa::path(
    get,
    path = "/api/v1/workday/mappings/accounts",
    tag = "Workday",
    responses(
        (status = 200, description = "Account mappings", body = Vec<WorkdayAccountMapping>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let mappings: Vec<WorkdayAccountMapping> = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT workday_account_id, billforge_gl_code, workday_account_name, workday_account_type
         FROM workday_account_mappings
         WHERE tenant_id = $1
         ORDER BY workday_account_id"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get account mappings: {}", e)))?
    .into_iter()
    .map(|(wd_id, bf_code, name, acct_type)| WorkdayAccountMapping {
        billforge_gl_code: bf_code,
        workday_account_id: wd_id,
        workday_account_name: name,
        workday_account_type: acct_type,
    })
    .collect();

    Ok(Json(mappings))
}

/// Update ledger account mappings
#[utoipa::path(
    post,
    path = "/api/v1/workday/mappings/accounts",
    tag = "Workday",
    request_body = Vec<WorkdayAccountMapping>,
    responses(
        (status = 200, description = "Mappings updated"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_account_mappings(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(mappings): Json<Vec<WorkdayAccountMapping>>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    for mapping in mappings {
        sqlx::query(
            "UPDATE workday_account_mappings
             SET billforge_gl_code = $3, updated_at = NOW()
             WHERE tenant_id = $1 AND workday_account_id = $2",
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&mapping.workday_account_id)
        .bind(&mapping.billforge_gl_code)
        .execute(&*pool)
        .await
        .ok();
    }

    Ok(Json(serde_json::json!({ "status": "updated" })))
}

/// List available Workday companies (multi-company support)
#[utoipa::path(
    get,
    path = "/api/v1/workday/companies",
    tag = "Workday",
    responses(
        (status = 200, description = "Available companies"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_companies(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let client = get_workday_client(&state, &pool, &tenant.tenant_id).await?;

    let companies = client
        .query_companies()
        .await
        .map_err(|e| billforge_core::Error::Validation(format!("Workday API error: {}", e)))?;

    Ok(Json(companies.data))
}
