//! Bill.com (BILL) AP payments integration endpoints
//!
//! Bill.com uses session-based authentication (like Sage Intacct),
//! not OAuth 2.0. The connection flow is:
//! 1. User provides devKey + orgId + credentials in settings UI
//! 2. BillForge stores encrypted credentials
//! 3. On API calls, BillForge establishes a session (login) and calls APIs
//!
//! Bill.com is unique in that it handles the PAYMENT execution step:
//! after an invoice is approved in BillForge and pushed to Bill.com
//! as a bill, the user can execute payment (ACH, check, virtual card)
//! directly from BillForge's UI.
//!
//! Endpoints:
//! - POST /connect — Save Bill.com credentials & test connection
//! - POST /disconnect — Remove stored credentials
//! - GET /status — Check connection status
//! - POST /sync/vendors — Sync vendors from Bill.com
//! - POST /push/bill/:id — Push approved invoice to Bill.com as a bill
//! - POST /pay/bill/:id — Create payment for a Bill.com bill
//! - POST /pay/bulk — Create bulk payments for multiple bills
//! - GET /payments — List payment records
//! - GET /funding-accounts — List available bank accounts for payment

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
use billforge_bill_com::{BillComAuth, BillComAuthConfig, BillComClient, BillComEnvironment};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Connection endpoints (credential-based, not OAuth)
        .route("/connect", post(bill_com_connect))
        .route("/disconnect", post(bill_com_disconnect))
        .route("/status", get(bill_com_status))
        // Sync endpoints
        .route("/sync/vendors", post(sync_vendors))
        .route("/push/bill/:id", post(push_bill_to_bill_com))
        // Payment endpoints
        .route("/pay/bill/:id", post(pay_bill))
        .route("/pay/bulk", post(pay_bulk))
        .route("/payments", get(list_payments))
        .route("/funding-accounts", get(list_funding_accounts))
        // Webhook secret configuration (requires auth)
        .route("/webhook/configure", post(configure_bill_com_webhook))
        // Webhook (no auth - verified via HMAC signature; tenant_id in path)
        .route("/webhook/:tenant_id", post(bill_com_webhook))
}

// ──────────────────────────── Types ────────────────────────────

/// Bill.com connection request (credential-based)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillComConnectRequest {
    /// Developer key (issued by Bill.com)
    pub dev_key: String,
    /// Organization ID
    pub org_id: String,
    /// User name (email)
    pub user_name: String,
    /// User password
    pub password: String,
    /// Environment ("sandbox" or "production")
    pub environment: String,
}

/// Bill.com connection status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BillComStatus {
    /// Whether Bill.com is connected
    pub connected: bool,
    /// Organization ID
    pub org_id: Option<String>,
    /// Environment
    pub environment: Option<String>,
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

/// Push bill request (additional fields for Bill.com bill creation)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PushBillRequest {
    /// Invoice ID to push
    pub invoice_id: String,
    /// Chart of account ID for the line
    pub chart_of_account_id: Option<String>,
    /// Department ID
    pub department_id: Option<String>,
    /// Location ID
    pub location_id: Option<String>,
}

/// Push bill response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PushBillResponse {
    /// Bill.com bill ID
    pub bill_com_bill_id: String,
    /// Export status
    pub status: String,
}

/// Pay bill request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PayBillRequest {
    /// Bill.com bill ID to pay
    pub bill_com_bill_id: String,
    /// Payment amount in dollars
    pub amount: f64,
    /// Scheduled process date (YYYY-MM-DD)
    pub process_date: String,
    /// Disbursement type (ACH, Check, VirtualCard)
    pub disbursement_type: String,
    /// Funding account ID (bank account to pay from)
    pub funding_account_id: String,
}

/// Pay bill response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PayBillResponse {
    /// Bill.com payment ID
    pub bill_com_payment_id: String,
    /// Payment status
    pub status: String,
}

/// Bulk pay request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkPayRequest {
    /// Scheduled process date (YYYY-MM-DD)
    pub process_date: String,
    /// Funding account ID
    pub funding_account_id: String,
    /// Individual payment items
    pub payments: Vec<BulkPayItem>,
}

/// Individual item in a bulk pay request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkPayItem {
    /// Bill.com bill ID
    pub bill_com_bill_id: String,
    /// Payment amount in dollars
    pub amount: f64,
}

/// Payment record (from database)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaymentRecord {
    /// Payment record ID
    pub id: String,
    /// BillForge invoice ID
    pub invoice_id: String,
    /// Bill.com payment ID
    pub bill_com_payment_id: String,
    /// Amount in cents
    pub amount_cents: i64,
    /// Process date
    pub process_date: String,
    /// Payment status
    pub status: String,
    /// Disbursement type
    pub disbursement_type: String,
    /// Confirmation number
    pub confirmation_number: Option<String>,
}

// ──────────────────────────── Handlers ────────────────────────────

/// Helper: parse environment string to BillComEnvironment
fn parse_environment(env: &str) -> Result<BillComEnvironment, billforge_core::Error> {
    match env.to_lowercase().as_str() {
        "production" => Ok(BillComEnvironment::Production),
        "sandbox" => Ok(BillComEnvironment::Sandbox),
        _ => Err(billforge_core::Error::Validation(
            format!("Invalid environment '{}'. Must be 'production' or 'sandbox'.", env)
        )),
    }
}

/// Helper: get a Bill.com client from stored credentials
async fn get_bill_com_client(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
) -> Result<(BillComClient, String), billforge_core::Error> {
    let connection: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT dev_key, org_id, user_name, password, environment
         FROM bill_com_connections
         WHERE tenant_id = $1 AND sync_enabled = true"
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (dev_key, org_id, user_name, password, environment) = connection
        .ok_or_else(|| billforge_core::Error::Validation("Bill.com not connected or sync disabled".to_string()))?;

    let env = parse_environment(&environment)?;

    // Establish session
    let auth = BillComAuth::new(BillComAuthConfig {
        dev_key: dev_key.clone(),
        org_id,
        user_name,
        password,
        environment: env,
    });

    let session = auth.login().await
        .map_err(|e| billforge_core::Error::Validation(format!("Bill.com login failed: {}", e)))?;

    let client = BillComClient::new(session, env, dev_key);
    Ok((client, environment))
}

/// Connect to Bill.com (save credentials & test connection)
#[utoipa::path(
    post,
    path = "/api/v1/bill-com/connect",
    tag = "Bill.com",
    request_body = BillComConnectRequest,
    responses(
        (status = 200, description = "Bill.com connected"),
        (status = 400, description = "Invalid credentials"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn bill_com_connect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<BillComConnectRequest>,
) -> ApiResult<impl IntoResponse> {
    let env = parse_environment(&request.environment)?;

    // Test connection by logging in
    let auth = BillComAuth::new(BillComAuthConfig {
        dev_key: request.dev_key.clone(),
        org_id: request.org_id.clone(),
        user_name: request.user_name.clone(),
        password: request.password.clone(),
        environment: env,
    });

    let session = auth.login().await
        .map_err(|e| billforge_core::Error::Validation(
            format!("Failed to connect to Bill.com: {}. Please verify your credentials.", e)
        ))?;

    // Store credentials in database (encrypted in production)
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query(
        "INSERT INTO bill_com_connections (
            tenant_id, org_id, dev_key, user_name, password, environment,
            sync_enabled, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, true, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            org_id = $2,
            dev_key = $3,
            user_name = $4,
            password = $5,
            environment = $6,
            sync_enabled = true,
            updated_at = NOW()"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&request.org_id)
    .bind(&request.dev_key)
    .bind(&request.user_name)
    .bind(&request.password) // TODO: encrypt in production
    .bind(&request.environment)
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(serde_json::json!({
        "status": "connected",
        "org_id": session.org_id,
        "user_id": session.user_id,
    })))
}

/// Disconnect Bill.com
#[utoipa::path(
    post,
    path = "/api/v1/bill-com/disconnect",
    tag = "Bill.com",
    responses(
        (status = 200, description = "Bill.com disconnected"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn bill_com_disconnect(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query("DELETE FROM bill_com_connections WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(serde_json::json!({ "status": "disconnected" })))
}

/// Get Bill.com connection status
#[utoipa::path(
    get,
    path = "/api/v1/bill-com/status",
    tag = "Bill.com",
    responses(
        (status = 200, description = "Bill.com status", body = BillComStatus),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn bill_com_status(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let connection: Option<(String, String, bool, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT org_id, environment, sync_enabled, last_sync_at
         FROM bill_com_connections
         WHERE tenant_id = $1"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let status = if let Some((org_id, environment, sync_enabled, last_sync_at)) = connection {
        BillComStatus {
            connected: true,
            org_id: Some(org_id),
            environment: Some(environment),
            last_sync_at: last_sync_at.map(|t| t.to_rfc3339()),
            sync_enabled,
        }
    } else {
        BillComStatus {
            connected: false,
            org_id: None,
            environment: None,
            last_sync_at: None,
            sync_enabled: false,
        }
    };

    Ok(Json(status))
}

/// Sync vendors from Bill.com
#[utoipa::path(
    post,
    path = "/api/v1/bill-com/sync/vendors",
    tag = "Bill.com",
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
    let (client, _env) = get_bill_com_client(&pool, &tenant.tenant_id).await?;

    // Create sync log entry
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bill_com_sync_log (id, tenant_id, sync_type, status, started_at)
         VALUES ($1, $2, 'vendors', 'running', NOW())"
    )
    .bind(sync_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .ok();

    // Fetch vendors (paginate through all)
    let mut all_vendors = Vec::new();
    let mut page = 0;
    let page_size = 100;

    loop {
        let result = client.list_vendors(page, page_size).await
            .map_err(|e| billforge_core::Error::Validation(format!("Bill.com API error: {}", e)))?;

        all_vendors.extend(result.data);

        if !result.has_more {
            break;
        }
        page += 1;
    }

    let mut imported = 0u64;
    let mut updated = 0u64;
    let skipped = 0u64;

    // Sync each vendor
    for bc_vendor in &all_vendors {
        let bc_vendor_id = bc_vendor.id.as_deref().unwrap_or_default();
        if bc_vendor_id.is_empty() {
            continue;
        }

        // Check if vendor mapping exists
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v
             INNER JOIN bill_com_vendor_mappings m ON m.billforge_vendor_id = v.id
             WHERE m.tenant_id = $1 AND m.bill_com_vendor_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(bc_vendor_id)
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        let email = bc_vendor.email.as_deref().unwrap_or("");
        let phone = bc_vendor.phone.as_deref().unwrap_or("");

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW()
                 WHERE id = $1"
            )
            .bind(vendor_id)
            .bind(&bc_vendor.name)
            .bind(email)
            .bind(phone)
            .execute(&*pool)
            .await
            .ok();

            updated += 1;
        } else {
            // Create new vendor
            let vendor_id = Uuid::new_v4();

            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at)
                 VALUES ($1, $2, 'business', $3, $4, $5, NOW(), NOW())"
            )
            .bind(vendor_id)
            .bind(&bc_vendor.name)
            .bind(email)
            .bind(phone)
            .bind(bc_vendor.status.as_deref().unwrap_or("active"))
            .execute(&*pool)
            .await
            .ok();

            // Create mapping
            sqlx::query(
                "INSERT INTO bill_com_vendor_mappings
                 (tenant_id, bill_com_vendor_id, billforge_vendor_id, bill_com_vendor_name, last_synced_at, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())"
            )
            .bind(tenant.tenant_id.as_uuid())
            .bind(bc_vendor_id)
            .bind(vendor_id)
            .bind(&bc_vendor.name)
            .execute(&*pool)
            .await
            .ok();

            imported += 1;
        }
    }

    // Update sync log
    sqlx::query(
        "UPDATE bill_com_sync_log
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
    sqlx::query("UPDATE bill_com_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .ok();

    Ok(Json(SyncResponse { imported, updated, skipped }))
}

/// Push approved BillForge invoice to Bill.com as a bill
#[utoipa::path(
    post,
    path = "/api/v1/bill-com/push/bill/{id}",
    tag = "Bill.com",
    request_body = PushBillRequest,
    responses(
        (status = 200, description = "Bill created in Bill.com", body = PushBillResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Invoice not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn push_bill_to_bill_com(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<PushBillRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let (client, _env) = get_bill_com_client(&pool, &tenant.tenant_id).await?;

    // Get invoice from database
    let invoice_id: billforge_core::domain::InvoiceId = request.invoice_id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, due_date, invoice_date
         FROM invoices WHERE id = $1"
    )
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let (vendor_name, invoice_number, total_cents, due_date, invoice_date) = invoice
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: request.invoice_id.clone(),
        })?;

    // Get vendor mapping to Bill.com
    let vendor_mapping: Option<(String,)> = sqlx::query_as(
        "SELECT bill_com_vendor_id FROM bill_com_vendor_mappings
         WHERE tenant_id = $1 AND billforge_vendor_id IN
         (SELECT vendor_id FROM invoices WHERE id = $2)"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let bill_com_vendor_id = vendor_mapping
        .map(|(id,)| id)
        .ok_or_else(|| billforge_core::Error::Validation("Vendor not found in Bill.com. Please sync vendors first.".to_string()))?;

    // Build Bill.com bill
    use billforge_bill_com::{BillComBill, BillComBillLine};

    let amount = total_cents as f64 / 100.0;
    let today = chrono::Utc::now().date_naive();
    let inv_date = invoice_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .unwrap_or(today);

    let bill = BillComBill {
        id: None,
        vendor_id: bill_com_vendor_id,
        invoice_number: Some(invoice_number.clone()),
        invoice_date: inv_date,
        due_date: due_date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        amount,
        description: Some(format!("BillForge export: Invoice {}", invoice_number)),
        line_items: vec![BillComBillLine {
            id: None,
            amount,
            description: Some(format!("Invoice {}", invoice_number)),
            chart_of_account_id: request.chart_of_account_id.clone(),
            department_id: request.department_id.clone(),
            location_id: request.location_id.clone(),
            class_id: None,
        }],
        status: None,
        created_time: None,
        updated_time: None,
    };

    // Create bill in Bill.com
    let result = client.create_bill(&bill).await
        .map_err(|e| billforge_core::Error::Validation(format!("Bill.com API error: {}", e)))?;

    let bill_com_bill_id = result.id.unwrap_or_default();

    // Store export record
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bill_com_bill_exports (id, tenant_id, invoice_id, bill_com_bill_id, exported_at, export_status)
         VALUES ($1, $2, $3, $4, NOW(), 'synced')
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET
            bill_com_bill_id = $4,
            exported_at = NOW(),
            export_status = 'synced'"
    )
    .bind(export_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&bill_com_bill_id)
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(PushBillResponse {
        bill_com_bill_id,
        status: "synced".to_string(),
    }))
}

/// Pay a Bill.com bill (single payment)
#[utoipa::path(
    post,
    path = "/api/v1/bill-com/pay/bill/{id}",
    tag = "Bill.com",
    request_body = PayBillRequest,
    responses(
        (status = 200, description = "Payment created", body = PayBillResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn pay_bill(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<PayBillRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let (client, _env) = get_bill_com_client(&pool, &tenant.tenant_id).await?;

    // Get the bill from Bill.com to verify it exists and get vendor_id
    let bc_bill = client.get_bill(&request.bill_com_bill_id).await
        .map_err(|e| billforge_core::Error::Validation(format!("Bill.com API error: {}", e)))?;

    let process_date = chrono::NaiveDate::parse_from_str(&request.process_date, "%Y-%m-%d")
        .map_err(|_| billforge_core::Error::Validation("Invalid process_date format. Use YYYY-MM-DD.".to_string()))?;

    // Create payment in Bill.com
    use billforge_bill_com::BillComPayment;

    let payment = BillComPayment {
        id: None,
        vendor_id: bc_bill.vendor_id.clone(),
        bill_id: request.bill_com_bill_id.clone(),
        amount: request.amount,
        process_date,
        status: None,
        disbursement_type: Some(request.disbursement_type.clone()),
        funding_account: Some(request.funding_account_id.clone()),
        confirmation_number: None,
        transaction_number: None,
        created_time: None,
    };

    let result = client.create_payment(&payment).await
        .map_err(|e| billforge_core::Error::Validation(format!("Bill.com payment failed: {}", e)))?;

    let bill_com_payment_id = result.id.clone().unwrap_or_default();
    let payment_status = result.status.clone().unwrap_or_else(|| "Scheduled".to_string());

    // Find the BillForge invoice linked to this bill
    let invoice_link: Option<(Uuid,)> = sqlx::query_as(
        "SELECT invoice_id FROM bill_com_bill_exports
         WHERE tenant_id = $1 AND bill_com_bill_id = $2"
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&request.bill_com_bill_id)
    .fetch_optional(&*pool)
    .await
    .ok()
    .flatten();

    let invoice_id = invoice_link.map(|(id,)| id).unwrap_or_else(Uuid::new_v4);
    let amount_cents = (request.amount * 100.0) as i64;

    // Store payment record
    let payment_record_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bill_com_payment_records (
            id, tenant_id, invoice_id, bill_com_payment_id, amount_cents,
            process_date, status, disbursement_type, confirmation_number, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())"
    )
    .bind(payment_record_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(invoice_id)
    .bind(&bill_com_payment_id)
    .bind(amount_cents)
    .bind(&request.process_date)
    .bind(&payment_status)
    .bind(&request.disbursement_type)
    .bind(&result.confirmation_number)
    .execute(&*pool)
    .await
    .ok();

    Ok(Json(PayBillResponse {
        bill_com_payment_id,
        status: payment_status,
    }))
}

/// Bulk pay multiple Bill.com bills
#[utoipa::path(
    post,
    path = "/api/v1/bill-com/pay/bulk",
    tag = "Bill.com",
    request_body = BulkPayRequest,
    responses(
        (status = 200, description = "Bulk payment created"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn pay_bulk(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(request): Json<BulkPayRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let (client, _env) = get_bill_com_client(&pool, &tenant.tenant_id).await?;

    let process_date = chrono::NaiveDate::parse_from_str(&request.process_date, "%Y-%m-%d")
        .map_err(|_| billforge_core::Error::Validation("Invalid process_date format. Use YYYY-MM-DD.".to_string()))?;

    // Build bulk payment request
    use billforge_bill_com::{BillComBulkPaymentRequest, BillComBulkPaymentItem};

    let bulk_request = BillComBulkPaymentRequest {
        process_date,
        funding_account: request.funding_account_id.clone(),
        payments: request.payments.iter().map(|p| BillComBulkPaymentItem {
            bill_id: p.bill_com_bill_id.clone(),
            amount: p.amount,
        }).collect(),
    };

    let result = client.create_bulk_payment(&bulk_request).await
        .map_err(|e| billforge_core::Error::Validation(format!("Bill.com bulk payment failed: {}", e)))?;

    // Store payment records for each bill
    for item in &request.payments {
        let amount_cents = (item.amount * 100.0) as i64;

        // Find the BillForge invoice linked to this bill
        let invoice_link: Option<(Uuid,)> = sqlx::query_as(
            "SELECT invoice_id FROM bill_com_bill_exports
             WHERE tenant_id = $1 AND bill_com_bill_id = $2"
        )
        .bind(tenant.tenant_id.as_uuid())
        .bind(&item.bill_com_bill_id)
        .fetch_optional(&*pool)
        .await
        .ok()
        .flatten();

        let invoice_id = invoice_link.map(|(id,)| id).unwrap_or_else(Uuid::new_v4);

        let payment_record_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO bill_com_payment_records (
                id, tenant_id, invoice_id, bill_com_payment_id, amount_cents,
                process_date, status, disbursement_type, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'Scheduled', 'bulk', NOW(), NOW())"
        )
        .bind(payment_record_id)
        .bind(tenant.tenant_id.as_uuid())
        .bind(invoice_id)
        .bind(&item.bill_com_bill_id) // placeholder until we get individual payment IDs
        .bind(amount_cents)
        .bind(&request.process_date)
        .execute(&*pool)
        .await
        .ok();
    }

    Ok(Json(serde_json::json!({
        "status": "bulk_payment_created",
        "payments_count": request.payments.len(),
        "result": result,
    })))
}

/// List payment records
#[utoipa::path(
    get,
    path = "/api/v1/bill-com/payments",
    tag = "Bill.com",
    responses(
        (status = 200, description = "Payment records", body = Vec<PaymentRecord>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_payments(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let records: Vec<PaymentRecord> = sqlx::query_as::<_, (String, Uuid, String, i64, String, String, String, Option<String>)>(
        "SELECT id::text, invoice_id, bill_com_payment_id, amount_cents,
                process_date, status, disbursement_type, confirmation_number
         FROM bill_com_payment_records
         WHERE tenant_id = $1
         ORDER BY created_at DESC"
    )
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get payment records: {}", e)))?
    .into_iter()
    .map(|(id, invoice_id, bc_payment_id, amount_cents, process_date, status, disbursement_type, confirmation_number)| {
        PaymentRecord {
            id,
            invoice_id: invoice_id.to_string(),
            bill_com_payment_id: bc_payment_id,
            amount_cents,
            process_date,
            status,
            disbursement_type,
            confirmation_number,
        }
    })
    .collect();

    Ok(Json(records))
}

/// List available funding accounts (bank accounts for payment)
#[utoipa::path(
    get,
    path = "/api/v1/bill-com/funding-accounts",
    tag = "Bill.com",
    responses(
        (status = 200, description = "Funding accounts"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_funding_accounts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let (client, _env) = get_bill_com_client(&pool, &tenant.tenant_id).await?;

    let result = client.list_funding_accounts().await
        .map_err(|e| billforge_core::Error::Validation(format!("Bill.com API error: {}", e)))?;

    Ok(Json(result.data))
}

/// Request body for configuring a webhook secret
#[derive(Debug, Deserialize)]
struct ConfigureWebhookRequest {
    webhook_secret: String,
}

/// Configure the webhook secret for Bill.com integration.
async fn configure_bill_com_webhook(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Json(body): Json<ConfigureWebhookRequest>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query(
        "UPDATE bill_com_connections SET webhook_secret = $2 WHERE tenant_id = $1",
    )
    .bind(tenant.tenant_id.as_uuid())
    .bind(&body.webhook_secret)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to update webhook secret: {}", e)))?;

    Ok(Json(serde_json::json!({ "status": "configured" })))
}

/// Receive and verify Bill.com webhook notifications.
///
/// Bill.com signs webhooks with HMAC-SHA256 using a shared secret.
/// The signature is hex-encoded in the `x-bill-signature` header.
/// The tenant_id is embedded in the webhook URL registered with Bill.com.
async fn bill_com_webhook(
    State(state): State<AppState>,
    axum::extract::Path(tenant_id_str): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let signature = headers
        .get("x-bill-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let tenant_id: billforge_core::TenantId = tenant_id_str.parse().map_err(|_| {
        tracing::error!("Bill.com webhook invalid tenant_id in path");
        StatusCode::BAD_REQUEST
    })?;

    let tenant_pool = state.db.tenant(&tenant_id).await.map_err(|e| {
        tracing::error!("Failed to get tenant pool: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let webhook_secret: Option<String> = sqlx::query_scalar(
        "SELECT webhook_secret FROM bill_com_connections WHERE tenant_id = $1",
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
            tracing::warn!("Bill.com webhook rejected: no webhook secret configured");
            StatusCode::UNAUTHORIZED
        })?;

    if !webhook::verify_webhook_signature(&body, signature, &secret) {
        tracing::warn!("Bill.com webhook signature verification failed");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let (event_type, nonce) = match serde_json::from_slice::<WebhookEnvelope>(&body) {
        Ok(envelope) => {
            if !webhook::validate_timestamp_freshness(envelope.timestamp, 300) {
                tracing::warn!(timestamp = %envelope.timestamp, "Bill.com webhook timestamp too old or in future");
                return Err(StatusCode::UNAUTHORIZED);
            }
            let nonce = envelope.nonce.unwrap_or_else(|| webhook::compute_payload_nonce(&body));
            (envelope.event_type, nonce)
        }
        Err(_) => ("provider_native".to_string(), webhook::compute_payload_nonce(&body)),
    };

    if !webhook::check_replay_nonce(&*tenant_pool, "bill_com", *tenant_id.as_uuid(), &nonce)
        .await
        .map_err(|e| {
            tracing::error!("Replay nonce check failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        tracing::warn!(nonce = %nonce, "Bill.com webhook replay detected");
        return Err(StatusCode::CONFLICT);
    }

    tracing::info!(
        event_type = %event_type,
        tenant_id = %tenant_id_str,
        "Bill.com webhook received and verified"
    );

    Ok(StatusCode::OK)
}
