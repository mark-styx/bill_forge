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
    extract::State,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
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

    let session = auth.get_session().await
        .map_err(|e| billforge_core::Error::Validation(
            format!("Failed to connect to Sage Intacct: {}. Please verify your credentials.", e)
        ))?;

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
            updated_at = NOW()"
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

    let connection: Option<(String, Option<String>, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT company_id, entity_id, sync_enabled, last_sync_at
         FROM sage_intacct_connections
         WHERE tenant_id = $1"
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

/// Sync vendors from Sage Intacct
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/sync/vendors",
    tag = "Sage Intacct",
    request_body = SyncRequest,
    responses(
        (status = 200, description = "Vendors synced", body = SyncResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn sync_vendors(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<SyncRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get stored credentials
    let connection: Option<(String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT sender_id, sender_password, user_id, user_password, entity_id
         FROM sage_intacct_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (sender_id, sender_password, user_id, user_password, entity_id) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Sage Intacct not connected or sync disabled".to_string()))?;

    // Get company_id separately
    let company_id_row: Option<(String,)> = sqlx::query_as(
        "SELECT company_id FROM sage_intacct_connections WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let company_id = company_id_row.map(|(id,)| id)
        .ok_or_else(|| billforge_core::Error::Validation("Sage Intacct company_id not found".to_string()))?;

    // Establish session
    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id,
        sender_password,
        company_id,
        entity_id,
        user_id,
        user_password,
    });

    let session = auth.get_session().await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct session failed: {}", e)))?;

    let client = SageIntacctClient::new(session);

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sage_intacct_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'vendors', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    // Fetch vendors (paginate through all)
    let mut all_vendors = Vec::new();
    let result = client.query_vendors(100, None).await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct API error: {}", e)))?;

    all_vendors.extend(result.records);

    // Handle pagination
    if result.num_remaining > 0 {
        if let Some(result_id) = &result.result_id {
            let mut remaining = result.num_remaining;
            let mut rid = result_id.clone();
            while remaining > 0 {
                match client.read_more_vendors(&rid).await {
                    Ok(more) => {
                        remaining = more.num_remaining;
                        if let Some(new_rid) = &more.result_id {
                            rid = new_rid.clone();
                        }
                        all_vendors.extend(more.records);
                    }
                    Err(_) => break,
                }
            }
        }
    }

    let mut imported = 0u64;
    let mut updated = 0u64;
    let skipped = 0u64;

    // Sync each vendor
    for sage_vendor in &all_vendors {
        // Check if vendor mapping exists
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v
             INNER JOIN sage_intacct_vendor_mappings m ON m.billforge_vendor_id = v.id
             WHERE m.tenant_id = $1 AND m.sage_vendor_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&sage_vendor.vendor_id)
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        let email = sage_vendor.display_contact.as_ref()
            .and_then(|c| c.email.as_deref())
            .unwrap_or("");
        let phone = sage_vendor.display_contact.as_ref()
            .and_then(|c| c.phone.as_deref())
            .unwrap_or("");

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW()
                 WHERE id = $1"
            )
            .bind(vendor_id)
            .bind(&sage_vendor.name)
            .bind(email)
            .bind(phone)
            .execute(&*pool)
            .await
            .ok();

            updated += 1;
        } else {
            // Create new vendor
            let vendor_id = Uuid::new_v4();
            let vendor_type = sage_vendor.vendor_type_id.as_deref().unwrap_or("business");

            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())"
            )
            .bind(vendor_id)
            .bind(&sage_vendor.name)
            .bind(vendor_type)
            .bind(email)
            .bind(phone)
            .bind(if sage_vendor.status == "active" { "active" } else { "inactive" })
            .execute(&*pool)
            .await
            .ok();

            // Create mapping
            sqlx::query(
                "INSERT INTO sage_intacct_vendor_mappings
                 (tenant_id, sage_vendor_id, sage_record_no, billforge_vendor_id, sage_vendor_name, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(&sage_vendor.vendor_id)
            .bind(&sage_vendor.record_no)
            .bind(vendor_id)
            .bind(&sage_vendor.name)
            .execute(&*pool)
            .await
            .ok();

            imported += 1;
        }
    }

    // Update sync log
    sqlx::query(
        "UPDATE sage_intacct_sync_log
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
    sqlx::query("UPDATE sage_intacct_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(SyncResponse { imported, updated, skipped }))
}

/// Sync GL accounts from Sage Intacct
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/sync/accounts",
    tag = "Sage Intacct",
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

    // Get stored credentials
    let connection: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT company_id, sender_id, sender_password, user_id, user_password, entity_id
         FROM sage_intacct_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (company_id, sender_id, sender_password, user_id, user_password, entity_id) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Sage Intacct not connected or sync disabled".to_string()))?;

    // Establish session
    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id,
        sender_password,
        company_id,
        entity_id,
        user_id,
        user_password,
    });

    let session = auth.get_session().await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct session failed: {}", e)))?;

    let client = SageIntacctClient::new(session);

    // Fetch GL accounts
    let result = client.query_gl_accounts(100, None).await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct API error: {}", e)))?;

    let mut created = 0u64;

    // Upsert account mappings
    for account in result.records {
        sqlx::query(
            "INSERT INTO sage_intacct_account_mappings
             (tenant_id, sage_account_no, sage_account_title, sage_account_type, billforge_gl_code, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $2, NOW(), NOW())
             ON CONFLICT (tenant_id, sage_account_no) DO UPDATE SET
                sage_account_title = $3,
                sage_account_type = $4,
                updated_at = NOW()"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&account.account_no)
        .bind(&account.title)
        .bind(&account.account_type)
        .execute(&*pool)
        .await
        .ok();

        created += 1;
    }

    Ok(Json(serde_json::json!({ "status": "synced", "count": created })))
}

/// Export invoice to Sage Intacct as AP bill
#[utoipa::path(
    post,
    path = "/api/v1/sage-intacct/export/invoice/{id}",
    tag = "Sage Intacct",
    request_body = ExportInvoiceRequest,
    responses(
        (status = 200, description = "Invoice exported", body = ExportInvoiceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Invoice not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn export_invoice_to_sage(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<ExportInvoiceRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get stored credentials
    let connection: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT company_id, sender_id, sender_password, user_id, user_password, entity_id
         FROM sage_intacct_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (company_id, sender_id, sender_password, user_id, user_password, entity_id) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Sage Intacct not connected or sync disabled".to_string()))?;

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
    .ok()
    .flatten();

    let (vendor_name, invoice_number, total_cents, due_date, po_number) = invoice
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: request.invoice_id.clone(),
        })?;

    // Get vendor mapping
    let vendor_mapping: Option<(String,)> = sqlx::query_as(
        "SELECT sage_vendor_id FROM sage_intacct_vendor_mappings
         WHERE tenant_id = $1 AND billforge_vendor_id IN
         (SELECT vendor_id FROM invoices WHERE id = $2)"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let sage_vendor_id = vendor_mapping
        .map(|(id,)| id)
        .ok_or_else(|| billforge_core::Error::Validation("Vendor not found in Sage Intacct. Please sync vendors first.".to_string()))?;

    // Establish session
    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id,
        sender_password,
        company_id,
        entity_id,
        user_id,
        user_password,
    });

    let session = auth.get_session().await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct session failed: {}", e)))?;

    let client = SageIntacctClient::new(session);

    // Build AP bill
    use billforge_sage_intacct::{SageAPBill, SageAPBillLine};

    let amount = total_cents as f64 / 100.0;
    let today = chrono::Utc::now().date_naive();

    let bill = SageAPBill {
        record_no: None,
        transaction_type: "Vendor Invoice".to_string(),
        vendor_id: sage_vendor_id,
        date_created: today,
        date_due: due_date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        document_number: Some(invoice_number.clone()),
        reference_number: po_number,
        description: Some(format!("BillForge export: Invoice {}", invoice_number)),
        currency: Some("USD".to_string()),
        exchange_rate_type: None,
        lines: vec![SageAPBillLine {
            gl_account_no: request.sage_account_no.clone(),
            amount,
            memo: Some(format!("Invoice {}", invoice_number)),
            department_id: request.department_id.clone(),
            location_id: request.location_id.clone(),
            project_id: None,
            class_id: None,
        }],
        state: None,
        total_amount: Some(amount),
        location_id: request.location_id.clone(),
        department_id: request.department_id.clone(),
    };

    // Create AP bill in Sage Intacct
    let result = client.create_ap_bill(&bill).await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct API error: {}", e)))?;

    if result.status != "success" {
        let error_msg = result.errors.first()
            .map(|e| e.description.clone())
            .unwrap_or_else(|| "Unknown error".to_string());
        return Err(billforge_core::Error::Validation(
            format!("Sage Intacct bill creation failed: {}", error_msg)
        ).into());
    }

    let sage_record_no = result.key.unwrap_or_default();

    // Store export record
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sage_intacct_invoice_exports (id, tenant_id, invoice_id, sage_record_no, exported_at, export_status)
         VALUES ($1, $2, $3, $4, NOW(), 'synced')
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET
            sage_record_no = $4,
            exported_at = NOW(),
            export_status = 'synced',
            sync_error = NULL"
    )
    .bind(export_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&sage_record_no)
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(ExportInvoiceResponse {
        sage_record_no,
        status: "synced".to_string(),
    }))
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
         ORDER BY sage_account_no"
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
             WHERE tenant_id = $1 AND sage_account_no = $2"
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
    let connection: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT company_id, sender_id, sender_password, user_id, user_password, entity_id
         FROM sage_intacct_connections
         WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (company_id, sender_id, sender_password, user_id, user_password, entity_id) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Sage Intacct not connected".to_string()))?;

    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id,
        sender_password,
        company_id,
        entity_id,
        user_id,
        user_password,
    });

    let session = auth.get_session().await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct session failed: {}", e)))?;

    let client = SageIntacctClient::new(session);

    let entities = client.list_entities().await
        .map_err(|e| billforge_core::Error::Validation(format!("Sage Intacct API error: {}", e)))?;

    Ok(Json(entities))
}
