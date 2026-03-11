//! Mobile API routes for device management, dashboard, and quick approvals

mod dto;
mod sync;

pub use dto::*;
pub use sync::*;

use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use crate::ApiResult;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put, delete},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Create mobile routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Device management
        .route("/devices/register", post(register_device))
        .route("/devices", get(list_devices))
        .route("/devices/:device_id", put(update_device))
        .route("/devices/:device_id", delete(unregister_device))
        // Mobile dashboard and quick actions
        .route("/dashboard", get(get_dashboard))
        .route("/invoices", get(list_invoices))
        .route("/invoices/:id", get(get_invoice))
        .route("/approvals", get(list_approvals))
        .route("/approvals/:id/approve", post(approve_invoice))
        .route("/approvals/:id/reject", post(reject_invoice))
        .route("/search", get(search))
        // Sync endpoints
        .route("/sync/invoices", get(sync_invoices))
        .route("/sync/bulk", get(sync_bulk))
}

// ===== Device Management =====

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceRequest {
    pub device_id: String,
    pub platform: String,
    pub token: String,
    pub device_name: Option<String>,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    pub id: Uuid,
    pub device_id: String,
    pub platform: String,
    pub device_name: Option<String>,
    pub is_active: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Register a new device token
async fn register_device(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(payload): Json<RegisterDeviceRequest>,
) -> ApiResult<Json<DeviceResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Validate platform
    if payload.platform != "ios" && payload.platform != "android" {
        return Err(billforge_core::Error::Validation("Invalid platform. Must be 'ios' or 'android'".to_string()).into());
    }

    // Validate token format based on platform
    let platform = if payload.platform == "ios" {
        billforge_mobile_push::DevicePlatform::Ios
    } else {
        billforge_mobile_push::DevicePlatform::Android
    };

    billforge_mobile_push::validate_token(&payload.token, platform)
        .map_err(|e| billforge_core::Error::Validation(format!("Invalid device token: {}", e)))?;

    // Upsert device token
    let row = sqlx::query!(
        r#"
        INSERT INTO device_tokens (tenant_id, user_id, device_id, platform, token, device_name, os_version, app_version)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (tenant_id, user_id, device_id)
        DO UPDATE SET
            token = EXCLUDED.token,
            device_name = EXCLUDED.device_name,
            os_version = EXCLUDED.os_version,
            app_version = EXCLUDED.app_version,
            is_active = true,
            updated_at = NOW()
        RETURNING id, device_id, platform, device_name, is_active, last_used_at, created_at
        "#,
        &tenant.tenant_id.0,
        &user.user_id.0,
        payload.device_id,
        payload.platform,
        payload.token,
        payload.device_name,
        payload.os_version,
        payload.app_version,
    )
    .fetch_one(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(Json(DeviceResponse {
        id: row.id,
        device_id: row.device_id,
        platform: row.platform,
        device_name: row.device_name,
        is_active: row.is_active,
        last_used_at: row.last_used_at,
        created_at: row.created_at,
    }))
}

/// List user's registered devices
async fn list_devices(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<DeviceResponse>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query!(
        r#"
        SELECT id, device_id, platform, device_name, is_active, last_used_at, created_at
        FROM device_tokens
        WHERE tenant_id = $1 AND user_id = $2 AND is_active = true
        ORDER BY created_at DESC
        "#,
        &tenant.tenant_id.0,
        &user.user_id.0,
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let devices = rows
        .into_iter()
        .map(|row| DeviceResponse {
            id: row.id,
            device_id: row.device_id,
            platform: row.platform,
            device_name: row.device_name,
            is_active: row.is_active,
            last_used_at: row.last_used_at,
            created_at: row.created_at,
        })
        .collect();

    Ok(Json(devices))
}

#[derive(Debug, Deserialize)]
pub struct UpdateDeviceRequest {
    pub device_name: Option<String>,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
}

/// Update device information
async fn update_device(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(device_id): Path<String>,
    Json(payload): Json<UpdateDeviceRequest>,
) -> ApiResult<Json<DeviceResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let row = sqlx::query!(
        r#"
        UPDATE device_tokens
        SET
            device_name = COALESCE($4, device_name),
            os_version = COALESCE($5, os_version),
            app_version = COALESCE($6, app_version),
            updated_at = NOW()
        WHERE tenant_id = $1 AND user_id = $2 AND device_id = $3
        RETURNING id, device_id, platform, device_name, is_active, last_used_at, created_at
        "#,
        &tenant.tenant_id.0,
        &user.user_id.0,
        device_id,
        payload.device_name,
        payload.os_version,
        payload.app_version,
    )
    .fetch_one(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(Json(DeviceResponse {
        id: row.id,
        device_id: row.device_id,
        platform: row.platform,
        device_name: row.device_name,
        is_active: row.is_active,
        last_used_at: row.last_used_at,
        created_at: row.created_at,
    }))
}

/// Unregister a device
async fn unregister_device(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(device_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query!(
        r#"
        UPDATE device_tokens
        SET is_active = false, updated_at = NOW()
        WHERE tenant_id = $1 AND user_id = $2 AND device_id = $3
        "#,
        &tenant.tenant_id.0,
        &user.user_id.0,
        device_id,
    )
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ===== Mobile Dashboard =====

/// Get mobile dashboard summary
async fn get_dashboard(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<MobileDashboard>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get counts
    let pending_approvals = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::int
        FROM approval_requests
        WHERE tenant_id = $1
          AND requested_from->>'user_id' = $2
          AND status = 'pending'
        "#,
        tenant.tenant_id.to_string(),
        user.user_id.0.to_string(),
    )
    .fetch_one(&*pool)
    .await
    .unwrap_or(None);

    let pending_review = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::int
        FROM invoices
        WHERE tenant_id = $1
          AND processing_status = 'pending_review'
        "#,
        &tenant.tenant_id.0,
    )
    .fetch_one(&*pool)
    .await
    .unwrap_or(None);

    let requires_attention = pending_approvals.unwrap_or(0) + pending_review.unwrap_or(0);

    // Get upcoming due dates (next 7 days)
    let upcoming_due_dates = sqlx::query!(
        r#"
        SELECT id, vendor_name, invoice_number, total_amount_cents, currency, due_date, processing_status
        FROM invoices
        WHERE tenant_id = $1
          AND due_date IS NOT NULL
          AND due_date <= NOW() + INTERVAL '7 days'
          AND processing_status NOT IN ('paid', 'cancelled')
        ORDER BY due_date ASC
        LIMIT 5
        "#,
        &tenant.tenant_id.0,
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let upcoming: Vec<MobileInvoiceSummary> = upcoming_due_dates
        .into_iter()
        .map(|row| MobileInvoiceSummary {
            id: row.id,
            vendor_name: row.vendor_name,
            invoice_number: row.invoice_number,
            total_amount_cents: row.total_amount_cents,
            currency: row.currency,
            due_date: row.due_date,
            status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
            days_until_due: row.due_date.map(|d| {
                (d - Utc::now().date_naive()).num_days() as i32
            }),
            requires_action: row.processing_status == "pending_approval",
            created_at: Utc::now(), // TODO: Add created_at to invoices table
        })
        .collect();

    Ok(Json(MobileDashboard {
        pending_approvals: pending_approvals.unwrap_or(0) as u32,
        pending_review: pending_review.unwrap_or(0) as u32,
        requires_attention: requires_attention as u32,
        upcoming_due_dates: upcoming,
        recent_activity: vec![], // TODO: Implement activity tracking
    }))
}

// ===== Invoice Endpoints =====

#[derive(Debug, Deserialize)]
pub struct ListInvoicesQuery {
    pub status: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// List invoices (mobile-optimized)
async fn list_invoices(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<ListInvoicesQuery>,
) -> ApiResult<Json<Vec<MobileInvoiceSummary>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let rows = sqlx::query!(
        r#"
        SELECT id, vendor_name, invoice_number, total_amount_cents, currency, due_date, processing_status
        FROM invoices
        WHERE tenant_id = $1
          AND ($2::text IS NULL OR processing_status = $2)
        ORDER BY created_at DESC
        LIMIT $3 OFFSET $4
        "#,
        &tenant.tenant_id.0,
        query.status,
        limit as i64,
        offset as i64,
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let invoices = rows
        .into_iter()
        .map(|row| MobileInvoiceSummary {
            id: row.id,
            vendor_name: row.vendor_name,
            invoice_number: row.invoice_number,
            total_amount_cents: row.total_amount_cents,
            currency: row.currency,
            due_date: row.due_date,
            status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
            days_until_due: row.due_date.map(|d| {
                (d - Utc::now().date_naive()).num_days() as i32
            }),
            requires_action: row.processing_status == "pending_approval",
            created_at: Utc::now(),
        })
        .collect();

    Ok(Json(invoices))
}

#[derive(Debug, Deserialize)]
pub struct GetInvoiceQuery {
    pub fields: Option<String>,
}

/// Get single invoice with field selection
async fn get_invoice(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
    Query(query): Query<GetInvoiceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let row = sqlx::query!(
        r#"
        SELECT id, vendor_name, invoice_number, total_amount_cents, currency, due_date, processing_status
        FROM invoices
        WHERE tenant_id = $1 AND id = $2
        "#,
        &tenant.tenant_id.0,
        id,
    )
    .fetch_one(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let invoice = MobileInvoiceSummary {
        id: row.id,
        vendor_name: row.vendor_name,
        invoice_number: row.invoice_number,
        total_amount_cents: row.total_amount_cents,
        currency: row.currency,
        due_date: row.due_date,
        status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
        days_until_due: row.due_date.map(|d| {
            (d - Utc::now().date_naive()).num_days() as i32
        }),
        requires_action: row.processing_status == "pending_approval",
        created_at: Utc::now(),
    };

    // Sparse fieldset selection
    if let Some(fields) = query.fields {
        let selected_fields: Vec<&str> = fields.split(',').collect();
        let mut map = serde_json::Map::new();

        for field in selected_fields {
            match field.trim() {
                "id" => map.insert("id".to_string(), serde_json::to_value(&invoice.id).unwrap()),
                "vendor_name" => map.insert("vendor_name".to_string(), serde_json::to_value(&invoice.vendor_name).unwrap()),
                "invoice_number" => map.insert("invoice_number".to_string(), serde_json::to_value(&invoice.invoice_number).unwrap()),
                "total_amount" => map.insert("total_amount".to_string(), serde_json::to_value(&invoice.total_amount_cents).unwrap()),
                "currency" => map.insert("currency".to_string(), serde_json::to_value(&invoice.currency).unwrap()),
                "due_date" => map.insert("due_date".to_string(), serde_json::to_value(&invoice.due_date).unwrap()),
                "status" => map.insert("status".to_string(), serde_json::to_value(&invoice.status).unwrap()),
                "days_until_due" => map.insert("days_until_due".to_string(), serde_json::to_value(&invoice.days_until_due).unwrap()),
                "requires_action" => map.insert("requires_action".to_string(), serde_json::to_value(&invoice.requires_action).unwrap()),
                "created_at" => map.insert("created_at".to_string(), serde_json::to_value(&invoice.created_at).unwrap()),
                _ => continue,
            };
        }

        Ok(Json(serde_json::Value::Object(map)))
    } else {
        Ok(Json(serde_json::to_value(invoice).unwrap()))
    }
}

// ===== Approvals =====

/// List pending approvals
async fn list_approvals(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<MobileApprovalRequest>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query!(
        r#"
        SELECT
            ar.id,
            ar.invoice_id,
            i.vendor_name,
            i.invoice_number,
            i.total_amount_cents,
            i.currency,
            i.due_date,
            i.processing_status,
            ar.created_at
        FROM approval_requests ar
        JOIN invoices i ON ar.invoice_id = i.id
        WHERE ar.tenant_id = $1
          AND ar.requested_from->>'user_id' = $2
          AND ar.status = 'pending'
        ORDER BY ar.created_at DESC
        LIMIT 50
        "#,
        tenant.tenant_id.to_string(),
        user.user_id.0.to_string(),
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let approvals = rows
        .into_iter()
        .map(|row| MobileApprovalRequest {
            id: row.id,
            invoice: MobileInvoiceSummary {
                id: row.invoice_id,
                vendor_name: row.vendor_name,
                invoice_number: row.invoice_number,
                total_amount_cents: row.total_amount_cents,
                currency: row.currency,
                due_date: row.due_date,
                status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
                days_until_due: row.due_date.map(|d| {
                    (d - Utc::now().date_naive()).num_days() as i32
                }),
                requires_action: true,
                created_at: row.created_at,
            },
            requested_at: row.created_at,
            expires_at: None,
            can_approve: true,
        })
        .collect();

    Ok(Json(approvals))
}

#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub comment: Option<String>,
}

/// Quick approve an invoice
async fn approve_invoice(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
    Json(payload): Json<ApproveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Update approval request
    sqlx::query!(
        r#"
        UPDATE approval_requests
        SET status = 'approved', responded_at = NOW(), comments = $3
        WHERE tenant_id = $1 AND id = $2 AND requested_from->>'user_id' = $4
        "#,
        tenant.tenant_id.to_string(),
        id,
        payload.comment,
        user.user_id.0.to_string(),
    )
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice status
    // TODO: Get invoice_id from approval_requests
    // TODO: Update invoice status to 'approved'

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub reason: String,
}

/// Quick reject an invoice
async fn reject_invoice(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<Uuid>,
    Json(payload): Json<RejectRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query!(
        r#"
        UPDATE approval_requests
        SET status = 'rejected', responded_at = NOW(), comments = $3
        WHERE tenant_id = $1 AND id = $2 AND requested_from->>'user_id' = $4
        "#,
        tenant.tenant_id.to_string(),
        id,
        payload.reason,
        user.user_id.0.to_string(),
    )
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ===== Search =====

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<i32>,
}

/// Global search (invoices, vendors)
async fn search(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let limit = query.limit.unwrap_or(10).min(50);
    let search_pattern = format!("%{}%", query.q);

    // Search invoices
    let invoice_rows = sqlx::query!(
        r#"
        SELECT id, vendor_name, invoice_number, total_amount_cents, currency
        FROM invoices
        WHERE tenant_id = $1
          AND (vendor_name ILIKE $2 OR invoice_number ILIKE $2)
        ORDER BY created_at DESC
        LIMIT $3
        "#,
        &tenant.tenant_id.0,
        search_pattern,
        limit as i64,
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let invoices: Vec<serde_json::Value> = invoice_rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "type": "invoice",
                "id": row.id,
                "title": format!("{} - {}", row.vendor_name, row.invoice_number),
                "subtitle": format!("{} {}", row.total_amount_cents, row.currency),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "results": invoices,
    })))
}
